use std::io::{self, ErrorKind};
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::ops::Bound;
use std::str::FromStr;

use anyhow::Result;
use bytes::Bytes;
use headers::{ContentLength, ContentRange, ETag, HeaderMapExt, IfMatch, IfNoneMatch, IfModifiedSince, IfUnmodifiedSince, IfRange, LastModified, Range, Origin};
use http::{HeaderMap, HeaderValue, Method, Response};
use once_cell::sync::Lazy;
use percent_encoding::percent_decode_str;
use regex::Regex;
use scan_fmt::scan_fmt;
use structopt::StructOpt;
use tokio::task;
use warp::Filter;

use mp4lib::pseudo::Mp4Stream;

#[derive(StructOpt, Debug)]
#[structopt(setting = clap::AppSettings::VersionlessSubcommands)]
pub struct MainOpts {
    #[structopt(long)]
    /// Log options (like RUSTLOG; trace, debug, info etc)
    pub log: Option<String>,
    #[structopt(subcommand)]
    pub cmd: Command,
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub enum Command {
    #[structopt(display_order = 1)]
    /// Media information.
    Serve(ServeOpts),
}

#[derive(StructOpt, Debug)]
pub struct ServeOpts {
    #[structopt(short, long)]
    /// Port to listen on.
    pub port: u16,

    #[structopt(short, long)]
    /// Root directory.
    pub dir: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let opts = MainOpts::from_args();

    let mut builder = env_logger::Builder::new();
    if let Some(ref log_opts) = opts.log {
        builder.parse_filters(log_opts);
    } else if let Ok(ref log_opts) = std::env::var("RUST_LOG") {
        builder.parse_filters(log_opts);
    } else {
        builder.parse_filters("info");
    }
    builder.init();

    match opts.cmd {
        Command::Serve(opts) => return serve(opts).await,
    }
}

async fn serve(opts: ServeOpts) -> Result<()> {
    let dir = opts.dir.clone();

    let data = warp::any()
        .map(move || dir.clone())
        .and(warp::path("data"))
        .and(warp::method())
        .and(warp::header::headers_cloned())
        .and(warp::path::tail())
        .and(
            warp::filters::query::raw()
                .or(warp::any().map(|| String::default()))
                .unify(),
        )
        .and_then(
            |dir: String, method: Method, headers: HeaderMap, tail: warp::path::Tail, query: String| {
                async move {
                    let resp = match hls(&dir, &method, &headers, tail.as_str(), &query).await {
                        Some(resp) => resp,
                        None => mp4stream(&dir, &method, &headers, tail.as_str(), &query).await,
                    };
                    Ok::<_, warp::Rejection>(resp)
                }
            },
        );

    let log = warp::log("mp4");
    let data = data.with(log);

    let addr = IpAddr::V6(Ipv6Addr::from(0u128));

    warp::serve(data).run(SocketAddr::new(addr, opts.port)).await;

    Ok(())
}

fn bound(bound: std::ops::Bound<u64>, max: u64) -> u64 {
    match bound {
        Bound::Included(n) => {
            if max > 0 {
                n + 1
            } else {
                n
            }
        },
        Bound::Excluded(n) => {
            if max > 0 {
                n
            } else {
                n + 1
            }
        },
        Bound::Unbounded => max,
    }
}

fn error(code: u16, text: impl Into<String>) -> http::Response<hyper::Body> {
    let text = text.into() + "\n";
    Response::builder()
        .header("content-type", "text/plain")
        .status(code)
        .body(hyper::Body::from(text))
        .unwrap()
}

fn io_error(err: io::Error) -> http::Response<hyper::Body> {
    match err.kind() {
        ErrorKind::NotFound => error(404, "Not Found"),
        ErrorKind::InvalidInput => error(400, format!("{}", err)),
        _ => error(500, format!("{}", err)),
    }
}

fn decode_path(path: &str, method: &Method) -> Result<String, http::Response<hyper::Body>> {
    // Check method.
    if method != &Method::GET && method != &Method::HEAD && method != &Method::OPTIONS {
        return Err(error(405, "Method Not Allowed"));
    }

    // Decode path and check for shenanigans
    let path = match percent_decode_str(path).decode_utf8() {
        Ok(path) => path,
        Err(_) => return Err(error(400, "Bad Request (path not utf-8)")),
    };
    if path
        .split('/')
        .any(|elem| elem == "" || elem == "." || elem == "..")
    {
        return Err(error(400, "Bad Request (path elements invalid)"));
    }
    Ok(path.to_string())
}

type RespBuilder = http::response::Builder;

async fn open_mp4(
    req_headers: &HeaderMap,
    dir: &str,
    path: &str,
    tracks: &[u32],
) -> Result<(Mp4Stream, ETag, LastModified, RespBuilder), http::Response<hyper::Body>> {

    // Open mp4 file.
    let path = format!("{}/{}", dir, path);
    let result = task::block_in_place(move || Mp4Stream::open(path, tracks));
    let strm = match result {
        Ok(strm) => strm,
        Err(e) => return Err(io_error(e)),
    };

    let mut response = http::Response::builder();

    // add headers.
    let resp_headers = response.headers_mut().unwrap();
    let etag = ETag::from_str(&strm.etag()).unwrap();
    let last_mod = LastModified::from(strm.modified());
    resp_headers.typed_insert(etag.clone());
    resp_headers.typed_insert(last_mod.clone());

    // Check If-Match.
    if let Some(im) = req_headers.typed_get::<IfMatch>() {
        if !im.precondition_passes(&etag) {
            return Err(error(412, "ETag does not match"));
        }
    } else {
        // Check If-Unmodified-Since.
        if let Some(iums) = req_headers.typed_get::<IfUnmodifiedSince>() {
            if !iums.precondition_passes(strm.modified()) {
                return Err(error(412, "resource was modified"));
            }
        }
    }

    // Check If-None-Match.
    if let Some(inm) = req_headers.typed_get::<IfNoneMatch>() {
        if !inm.precondition_passes(&etag) {
            response = response.status(304);
            return Err(response.body(hyper::Body::empty()).unwrap());
        }
    } else {
        if let Some(ims) = req_headers.typed_get::<IfModifiedSince>() {
            if !ims.is_modified(strm.modified()) {
                response = response.status(304);
                return Err(response.body(hyper::Body::empty()).unwrap());
            }
        }
    }

    Ok((strm, etag, last_mod, response))
}

async fn mp4stream(
    dir: &str,
    method: &Method,
    req_headers: &HeaderMap,
    path: &str,
    query: &str,
) -> http::Response<hyper::Body> {

    let path = match decode_path(path, &method) {
        Ok(path) => path,
        Err(resp) => return resp,
    };

    // Decode query.
    let mut tracks = Vec::new();
    for q in query.split('&') {
        let mut kv = q.splitn(2, '=');
        let key = kv.next().unwrap();
        let val = kv.next().unwrap_or("");
        match key {
            "track" => {
                let val = match u32::from_str(val) {
                    Ok(val) => val,
                    Err(_) => return error(500, "invalid track"),
                };
                tracks.push(val);
            },
            _ => {},
        }
    }

    let (mut strm, etag, last_mod, mut response) = match open_mp4(&req_headers, &dir, &path, &tracks[..]).await {
        Ok(strm) => strm,
        Err(resp) => return resp,
    };
    let resp_headers = response.headers_mut().unwrap();
    let mut status = 200;

    // Check ranges.
    let mut start = 0;
    let mut end = strm.size();

    if let Some(range) = req_headers.typed_get::<Range>() {
        let do_range = match req_headers.typed_get::<IfRange>() {
            Some(if_range) => !if_range.is_modified(Some(&etag), Some(&last_mod)),
            None => true,
        };
        if do_range {
            let ranges: Vec<_> = range.iter().collect();
            if ranges.len() > 1 {
                return error(416, "multiple ranges not supported");
            }
            start = bound(ranges[0].0, 0);
            end = bound(ranges[0].1, strm.size());
            let cr = match ContentRange::bytes(start .. end, strm.size()) {
                Ok(cr) => cr,
                Err(_) => return error(416, "invalid range"),
            };
            resp_headers.typed_insert(cr);
            status = 206;
        }
    }

    resp_headers.insert("content-type", HeaderValue::from_static("video/mp4"));
    resp_headers.typed_insert(ContentLength(end - start));
    response = response.status(status);

    // if HEAD quit now
    if method == &Method::HEAD {
        return response.body(hyper::Body::empty()).unwrap();
    }

    // Run content generation in a separate task
    let (mut tx, body) = hyper::Body::channel();
    task::spawn(async move {
        loop {
            let mut buf = Vec::<u8>::new();
            buf.resize(128000, 0);
            let result = task::block_in_place(|| strm.read_at(&mut buf, start));
            let n = match result {
                Ok(n) if n == 0 => break,
                Ok(n) => n,
                Err(_) => break,
            };
            buf.truncate(n);
            start += n as u64;
            if let Err(_) = tx.send_data(Bytes::from(buf)).await {
                break;
            }
        }
    });

    response.body(body).unwrap()
}


async fn hls(
    dir: &str,
    method: &Method,
    req_headers: &HeaderMap,
    path: &str,
    _query: &str,
) -> Option<http::Response<hyper::Body>> {

    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"^(?x)
            (.*\.mp4)/(
                main.m3u8|
                media.\d+\.m3u8|
                init\.\d+\.mp4|
                init\.\d+\.vtt|
                [asv]/c\.\d+\.\d+.*
            )$"#).unwrap()
    });

    let path = match decode_path(path, &method) {
        Ok(path) => path,
        Err(resp) => return Some(resp),
    };
    let caps = RE.captures(&path)?;
    let filename = caps.get(1)?.as_str();
    let extra = caps.get(2)?.as_str();

    let (strm, _, _, mut response) = match open_mp4(&req_headers, &dir, &filename, &[]).await {
        Ok(strm) => strm,
        Err(resp) => return Some(resp),
    };
    let resp_headers = response.headers_mut().unwrap();

    if let Some(host) = req_headers.typed_get::<Origin>() {
        resp_headers.insert("access-control-allow-origin", HeaderValue::from_str(&host.to_string()).unwrap());
    } else {
        resp_headers.insert("access-control-allow-origin", HeaderValue::from_static("*"));
    }
    resp_headers.insert("access-control-allow-headers", HeaderValue::from_static("if-match, if-unmodified-since, if-range, range"));
    resp_headers.insert("access-control-expose-headers", HeaderValue::from_static("content-type, content-length, content-range"));

    // if OPTIONS quit now
    if method == &Method::OPTIONS {
        return response.status(204).body(hyper::Body::empty()).ok();
    }

    let mp4 = match mp4lib::lru_cache::open_mp4(strm.path()) {
        Ok(mp4) => mp4,
        Err(e) => return Some(io_error(e)),
    };
    
    let (mime, body) = if extra.ends_with(".m3u8") {

        let m3u8 = if extra == "main.m3u8" {
            mp4lib::stream::hls_master(&mp4)
        } else {
            let track = match scan_fmt!(&extra, "media.{}.m3u8", u32) {
                Ok(t) => t,
                Err(_) => return Some(error(400, "invalid filename")),
            };
            match mp4lib::stream::hls_track(&mp4, track) {
                Ok(t) => t,
                Err(e) => return Some(io_error(e)),
            }
        };
        ("application/x-mpegurl ", m3u8.into_bytes())

    } else {

        match mp4lib::stream::fragment_from_uri(&mp4, extra) {
            Ok(t) => t,
            Err(e) => return Some(io_error(e)),
        }

    };

    resp_headers.insert("content-type", HeaderValue::from_static(mime));
    resp_headers.typed_insert(ContentLength(body.len() as u64));

    // if HEAD quit now
    if method == &Method::HEAD {
        return response.body(hyper::Body::empty()).ok();
    }

    response.body(body.into()).ok()
}
