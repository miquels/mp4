use std::cmp;
use std::collections::HashMap;
use std::fmt::{self, Display};
use std::fs;
use std::io::{self, ErrorKind};
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::ops::{self, Bound};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::str::FromStr;
use std::time::SystemTime;

use anyhow::Result;
use axum::{
    Router,
    body::Body,
    error_handling::HandleErrorLayer,
    http::Request,
    response::Response,
    routing::{get, head, options},
};
use bytes::Bytes;
use headers::{
    CacheControl, HeaderMapExt,
    Origin, UserAgent,
};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use regex::Regex;
use structopt::StructOpt;
use tokio::task;
use tower::{
    filter::AsyncFilterLayer,
    util::AndThenLayer,
    ServiceBuilder,
};
use tower_http::trace::TraceLayer;

use mp4lib::streaming::pseudo::Mp4Stream;
use mp4lib::streaming::http_file::HttpFile;

static EXE_STAMP: OnceCell<u32> = OnceCell::new();

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

    let exe_stamp = std::env::current_exe()
        .and_then(|p| p.metadata())
        .map(|m| m.mtime() as u32)
        .unwrap_or(0);
    EXE_STAMP.set(exe_stamp).unwrap();

    match opts.cmd {
        Command::Serve(opts) => return serve(opts).await,
    }
}

// When creating ETags, add the timestamp of the current executable,
// so that every time we recompile we get new unique tags.
fn update_etag(item: &mut impl HttpFile) {
    if let Some(tag) = item.get_etag() {
        let stamp = EXE_STAMP.get().expect("EXE_STAMP unset");
        tag.set_etag(&format!("{}.{:08x}", tag, stamp);
    }
}

// Straight from the documentation of once_cell.
macro_rules! regex {
    ($re:expr $(,)?) => {{
        static RE: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
        RE.get_or_init(|| regex::Regex::new($re).unwrap())
    }};
}

// helper to tunr the body in the response into axum's body type.
fn box_response<B>(resp: Response<B>) -> Response {
where
    B: 'static + http_Body<Data = Bytes> + Send,
    <B as http_Body>::Error: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
{
    let (parts, body) = resp.into_parts();
    let body = axum::body::boxed(body);
    http::Response::from_parts(parts, body)
}

#[derive(Deserialize)]
pub struct QueryParams {
    pub track_id:   Option<u32>,
}

async fn handle(
    Path(path): Path(String),
    params: Option<Query<Params>>,
    req: Request<Body>,
) -> Response<StreamBody> {
    let params = match params {
        None => None,
        Some(Query(params)) => Some(params),
    };

    // Pseudo-streaming request.
    if path.ends_with(".mp4") && params.is_some() {
        return mp4pseudo(req, &path, params);
    }

    // Subtitle format translation (subtitle.srt:into.vtt).
    const SUBTITLE: &'static str = r#"^(.*\.(?:srt|vtt)):into\.(srt|vtt))$"#;
    if let Some(caps) = regex!(MEDIA_DATA).captures(path) {
        return subtitles(req, &caps[1], &caps[2]);
    }

    // HLS manifest.
    const MANIFEST: &'static str = r#"^(.*\.mp4)/(.*\.m3u8)$"#;
    if let Some(caps) = regex!(MANIFEST).captures(path) {
        return manifest(req, &caps[1], &caps[2]);
    }

    // Media data.
    const MEDIA_DATA: &'static str = r#"^(.*\.mp4)/(.*\.(?:mp4|m4a))$"#;
    if let Some(caps) = regex!(MEDIA_DATA).captures(path) {
        return media_data(req, &caps[1], &caps[2]);
    }

    // A normal file.
    serve_file(&req)
}
    
async fn serve(opts: ServeOpts) -> Result<()> {
    let dir = opts.dir.clone();

    let middleware_stack = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(handle_error))
        .layer(TraceLayer::new_for_http());

    let app = Router::new()
        .route("/data/*path", get(handle).head(handle).options(options))
        .layer(middleware_stack);

    #[cfg(target_os = "freebsd")]
    {
        use std::net::Ipv4Addr;
        let addrv4 = IpAddr::V4(Ipv4Addr::from(0u32));
        let addrv6 = IpAddr::V6(Ipv6Addr::from(0u128));
        let sockaddr_v4 = SocketAddr::new(addrv4, opts.port);
        let sockaddr_v6 = SocketAddr::new(addrv6, opts.port);
        let app2 = app.clone();
        tokio::join!(
            axum::Server::bind(sockaddr_v4).serve(app.into_make_service())
            axum::Server::bind(sockaddr_v6).serve(app2.into_make_service())
        );
    }

    #[cfg(not(target_os = "freebsd"))]
    {
        let addr = IpAddr::V6(Ipv6Addr::from(0u128));
        let sockaddr = SocketAddr::new(addr, opts.port);
        axum::Server::bind(sockaddr).serve(app.into_make_service()).await.unwrap();
    }

    Ok(())
}

fn cors_headers(req: &Request, response: &mut RespBuilder) {
    let resp_headers = response.headers_mut().unwrap();

    if let Some(host) = req.headers.typed_get::<Origin>() {
        resp_headers.insert(
            "access-control-allow-origin",
            HeaderValue::from_str(&host.to_string()).unwrap(),
        );
    } else {
        resp_headers.insert("access-control-allow-origin", HeaderValue::from_static("*"));
    }

    let h = HeaderValue::from_static("x-application, origin, range");
    resp_headers.insert("access-control-allow-headers", h);

    let h = HeaderValue::from_static("server, content-range, accept-ranges");
    resp_headers.insert("access-control-expose-headers", h);

    let h = HeaderValue::from_static("GET, HEAD, OPTIONS");
    resp_headers.insert("access-control-allow-methods", h);
}

fn options(req: &Request, cors: bool) -> Response {
    let mut response = Response::builder().status(204);
    let resp_headers = response.headers_mut().unwrap();

    resp_headers.insert("allow", HeaderValue::from_static("GET, HEAD, OPTIONS"));
    if cors {
        cors_headers(req, &mut response);
    }
    response.body(hyper::Body::empty()).unwrap()
}

async fn mp4pseudo(req: &Request, path: &str, params: Params) -> io::Result<Response> {
    // get tracks, then open mp4stream.
    let tracks = match params.tracks.as_ref() {
        Some(val) => {
            let tracks: Vec<_> = val.split(',').filter_map(|t| t.parse::<u32>().ok()).collect();
            if tracks.len() == 0 {
                return Err(Error::new(400, "bad track parameter"));
            }
            tracks
        },
        None => return Err(Error::new(400, "need track parameter")),
    };
    let mp4stream = mp4lib::streaming::pseudo::Mp4Stream::open(&req.fpath, tracks)?;
    Ok(box_response(m4lib::streaming::http_file::serve_file(req, mp4stream)))
}

// serve m3u8 playlist files:
//
// - master.m3u8 / main.m3u8
// - media.<TRACK_ID>.m3u8
//
async fn manifest(req: &Request, path: &str, extra: &str) -> io::Result<Response> {
    let is_notflix = match req.headers.get("x-application").map(|v| v.to_str()) {
        Some(Ok(v)) => v.contains("Notflix"),
        _ => false,
    };
    let is_cast = match req.headers.typed_get::<UserAgent>() {
        Some(ua) => ua.as_str().contains("CrKey/"),
        None => false,
    };
    let simple_subs = is_cast && !is_notflix;

    let data = task::block_in_place(|| {
        let mp4 = mp4lib::streaming::lru_cache::open_mp4(path, false))?;
        mp4lib::streaming::hls::media_from_uri(mp4, extra)
    })?;
    Ok(box_response(m4lib::streaming::http_file::serve_file(req, data)))
}

async fn media(req: &Request, path: &str, extra: &str) -> io::Result<Response> {
    let data = task::block_in_place(|| {
        let mp4 = mp4lib::streaming::lru_cache::open_mp4(path, false))?;
        mp4lib::streaming::hls::media_from_uri(mp4, extra)
    })?;
    Ok(box_response(m4lib::streaming::http_file::serve_file(req, data)))
}

async fn info(req: &Request, path: &str) -> io::Result<Response> {
    let mp4 = task::block_in_place(|| {
        mp4lib::streaming::lru_cache::open_mp4(path, false))
    })?;

    let info = mp4lib::track::track_info(&mp4);
    let body = serde_json::to_string_pretty(&info).unwrap();

    let data = mp4lib::streaming::http_file::MemFile::from_file(body, "text/json", mp4)?;
    Ok(box_response(m4lib::streaming::http_file::serve_file(req, data)))
}

async fn subtitle(req: &Request, path: &str, into: &str) -> io::Result<Response> { 

    // if OPTIONS quit now
    if req.method == Method::OPTIONS {
        return Ok(Some(options(req, true)));
    }

    let fs = FileServer::open(&req.fpath).await?;
    fs.check_conditionals(req)?;
    let mut response = fs.build_response(req, true);
    let resp_headers = response.headers_mut().unwrap();

    let (mime, body) =
        task::block_in_place(|| mp4lib::streaming::subtitle::external(path, info))?;
    resp_headers.insert("content-type", HeaderValue::from_static(mime));
    resp_headers.typed_insert(ContentLength(body.len() as u64));

    // if HEAD quit now
    if req.method == Method::HEAD {
        return Ok(response.body(hyper::Body::empty()).ok());
    }

    Ok(response.body(body.into()).ok())
}

async fn serve_file(req: &Request) -> Result<Response, Error> {
    // if OPTIONS quit now
    if req.method == Method::OPTIONS {
        return Ok(options(req, true));
    }

    if req.sep != "" {
        return Err(Error::new(415, "Unsupported media type"));
    }

    let fs = FileServer::open(&req.fpath).await?;
    fs.check_conditionals(req)?;

    let mut response = fs.build_response(req, false);
    let resp_headers = response.headers_mut().unwrap();
    let mut status = 200;

    // Check ranges.
    let mut start = 0;
    let mut end = fs.size;
    if let Some(range) = req.parse_range(&fs)? {
        if let Ok(cr) = ContentRange::bytes(range.clone(), fs.size) {
            start = range.start;
            end = range.end;
            resp_headers.typed_insert(cr);
            status = 206;
        }
    }

    // Set Content-Type, Content-Length, and StatusCode.
    let mime = mime_guess::from_path(&req.path)
        .first_or_octet_stream()
        .to_string();

    resp_headers.insert("content-type", HeaderValue::from_str(&mime).unwrap());
    resp_headers.typed_insert(ContentLength(end - start));
    response = response.status(status);

    // if HEAD quit now
    if req.method == Method::HEAD {
        return Ok(response.body(hyper::Body::empty()).unwrap());
    }

    // Run content generation in a separate task
    let (mut tx, body) = hyper::Body::channel();
    task::spawn(async move {
        loop {
            let buflen = std::cmp::min(end - start, 128000) as usize;
            let mut buf = Vec::<u8>::new();
            buf.resize(buflen, 0);
            let result = task::block_in_place(|| fs.file.read_at(&mut buf, start));
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
            if start >= end {
                break;
            }
        }
    });

    Ok(response.body(body).unwrap())
}
