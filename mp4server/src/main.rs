use std::io::ErrorKind;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::ops::Bound;
use std::str::FromStr;

use anyhow::Result;
use bytes::Bytes;
use headers::{Range, HeaderMapExt};
use http::header::HeaderMap;
use http::{Method, Response};
use percent_encoding::percent_decode_str;
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
        .and(warp::filters::query::raw().or(warp::any().map(|| String::default())).unify())
        .and_then(|dir: String, method: Method, headers: HeaderMap, tail: warp::path::Tail, query: String| async move {
            Ok::<_, warp::Rejection>(mp4stream(dir, method, headers, tail.as_str(), query).await)
        });

    let addr = IpAddr::V6(Ipv6Addr::from(0u128));

    warp::serve(data)
        .run(SocketAddr::new(addr, opts.port))
        .await;

    Ok(())
}

fn bound(bound: std::ops::Bound<u64>, max: u64) -> u64 {
    match bound {
        Bound::Included(n) => if max > 0 { n + 1 } else { n },
        Bound::Excluded(n) => if max > 0 { n } else { n + 1 },
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

async fn mp4stream(dir: String, method: Method, headers: HeaderMap, path: &str, query: String) -> http::Response<hyper::Body> {

    // Check method.
    if method != Method::GET && method != Method::HEAD {
        return error(405, "Method Not Allowed");
    }

    // Decode path and check for shenanigans
    let path = match percent_decode_str(path).decode_utf8() {
        Ok(path) => path,
        Err(_) => return error(400, "Bad Request (path not utf-8)"),
    };
    if path.split('/').any(|elem| elem == "" || elem == "." || elem == "..") {
        return error(400, "Bad Request (path elements invalid)");
    }

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

    // Open mp4 file.
    let path = format!("{}/{}", dir, path);
    let result = task::block_in_place(move || {
        Mp4Stream::open(path, tracks)
    });
    let mut strm = match result {
        Ok(strm) => strm,
        Err(e) => {
            if e.kind() == ErrorKind::NotFound {
                return error(404, "Not Found");
            } else {
                return error(500, format!("{}", e));
            }
        },
    };

    let mut response = http::Response::builder();

    // Check ranges.
    let mut start = 0;
    let mut end = strm.size();

    if let Some(range) = headers.typed_get::<Range>() {
        let ranges: Vec<_> = range.iter().collect();
        if ranges.len() > 1 {
            return error(416, "multiple ranges not supported");
        }
        start = bound(ranges[0].0, 0);
        end = bound(ranges[0].1, strm.size());
        if start >= strm.size() || end > strm.size() {
            return error(416, "invalid range");
        }
        let cr = format!("bytes {}-{}/{}", start, end - 1, strm.size());
        response = response.header("content-range", cr);
        response = response.status(206);
    }

    // add headers.
    response = response.header("content-type", "video/mp4");
    response = response.header("content-length", format!("{}", end - start));
    response = response.header("etag", strm.etag());

    // if HEAD quit now
    if method == Method::HEAD {
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
