use std::fs;
use std::io;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};

use anyhow::Result;
use axum::{Router, body, response::Response, routing::get};
use axum::extract::{Path, Query};
use bytes::Bytes;
use headers::{HeaderMapExt, UserAgent};
use http::{Method, Request, StatusCode};
use serde::Deserialize;
use structopt::StructOpt;
use tokio::task;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tower_http::cors::{self, CorsLayer};

use mp4lib::streaming::{pseudo, hls, http_file};

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

// Helper. Straight from the documentation of once_cell.
macro_rules! regex {
    ($re:expr $(,)?) => {{
        static RE: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
        RE.get_or_init(|| regex::Regex::new($re).unwrap())
    }};
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

    tracing_subscriber::fmt::init();

    match opts.cmd {
        Command::Serve(opts) => return serve(opts).await,
    }
}

// Serve files.
async fn serve(opts: ServeOpts) -> Result<()> {
    use http::header::{HeaderName, ORIGIN, RANGE};

    // XXX FIXME in shared state!
    let dir = opts.dir.clone();

    let middleware_stack = ServiceBuilder::new()
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::new()
            .allow_origin(cors::any())
            .allow_methods(vec![Method::GET, Method::HEAD])
            .allow_headers(vec![HeaderName::from_static("x-application"), ORIGIN, RANGE ])
            .expose_headers(cors::any())
            .max_age(std::time::Duration::from_secs(86400)));

    let app = Router::new()
        .route("/data/*path", get(handle_request).head(handle_request))
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
            axum::Server::bind(&sockaddr_v4).serve(app.into_make_service())
            axum::Server::bind(&sockaddr_v6).serve(app2.into_make_service())
        );
    }

    #[cfg(not(target_os = "freebsd"))]
    {
        let addr = IpAddr::V6(Ipv6Addr::from(0u128));
        let sockaddr = SocketAddr::new(addr, opts.port);
        axum::Server::bind(&sockaddr).serve(app.into_make_service()).await.unwrap();
    }

    Ok(())
}

#[derive(Deserialize)]
pub struct Params {
    pub track_id:   Option<String>,
}

async fn handle_request(
    Path(path): Path<String>,
    params: Option<Query<Params>>,
    req: Request<body::Body>,
) -> Result<Response, StatusCode> {
    let (parts, _) = req.into_parts();
    let req = Request::from_parts(parts, ());
    let params = match params {
        None => None,
        Some(Query(params)) => Some(params),
    };
    handle_request2(path, params, req).await.map_err(|e| translate_io_error(e))
}

async fn handle_request2(
    path: String,
    params: Option<Params>,
    req: Request<()>
) -> io::Result<Response> {

    if let Some(response) = http_file::not_modified(&req, &path).await {
        return Ok(box_response(response));
    }

    // Pseudo-streaming request.
    if path.ends_with(".mp4") && params.is_some() {
        return mp4pseudo(&req, &path, params.unwrap()).await;
    }

    // Subtitle format translation (subtitle.srt:into.vtt).
    const SUBTITLE: &'static str = r#"^(.*\.(?:srt|vtt)):into\.(srt|vtt))$"#;
    if let Some(caps) = regex!(SUBTITLE).captures(&path) {
        return subtitle(&req, &caps[1], &caps[2]).await;
    }

    // HLS manifest.
    const MANIFEST: &'static str = r#"^(.*\.mp4)/(.*\.m3u8)$"#;
    if let Some(caps) = regex!(MANIFEST).captures(&path) {
        return manifest(&req, &caps[1], &caps[2]).await;
    }

    // Media data.
    const MEDIA_SEGMENT: &'static str = r#"^(.*\.mp4)/(.*\.(?:mp4|m4a))$"#;
    if let Some(caps) = regex!(MEDIA_SEGMENT).captures(&path) {
        return media_segment(&req, &caps[1], &caps[2]).await;
    }

    // Media info.
    const MEDIA_INFO: &'static str = r#"^(.*\.mp4)/info"#;
    if let Some(caps) = regex!(MEDIA_INFO).captures(&path) {
        return media_info(&req, &caps[1]).await;
    }

    // A normal file.
    let file = http_file::FsFile::open(&path)?;
    Ok(box_response(http_file::serve_file(&req, file).await))
}
    
async fn mp4pseudo(req: &Request<()>, path: &str, params: Params) -> io::Result<Response> {
    // get tracks, then open mp4stream.
    let tracks = match params.track_id.as_ref() {
        Some(val) => {
            let tracks: Vec<_> = val.split(',').filter_map(|t| t.parse::<u32>().ok()).collect();
            if tracks.len() == 0 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "bad track parameter"));
            }
            tracks
        },
        None => return Err(io::Error::new(io::ErrorKind::InvalidData, "need track parameter")),
    };
    let mp4stream = pseudo::Mp4Stream::open(path, tracks)?;
    Ok(box_response(http_file::serve_file(req, mp4stream).await))
}

// serve m3u8 playlist files:
//
// - master.m3u8 / main.m3u8
// - media.<TRACK_ID>.m3u8
//
async fn manifest(req: &Request<()>, path: &str, extra: &str) -> io::Result<Response> {
    let is_notflix = match req.headers().get("x-application").map(|v| v.to_str()) {
        Some(Ok(v)) => v.contains("Notflix"),
        _ => false,
    };
    let is_cast = match req.headers().typed_get::<UserAgent>() {
        Some(ua) => ua.as_str().contains("CrKey/"),
        None => false,
    };
    let simple_subs = is_cast && !is_notflix;

    let data = task::block_in_place(|| {
        let mp4 = mp4lib::streaming::lru_cache::open_mp4(path, false)?;
        hls::HlsManifest::from_uri(&*mp4, extra, simple_subs)
    })?;
    Ok(box_response(http_file::serve_file(req, data).await))
}

// MP4 fragments / segments.
async fn media_segment(req: &Request<()>, path: &str, extra: &str) -> io::Result<Response> {
    let data = task::block_in_place(|| {
        let mp4 = mp4lib::streaming::lru_cache::open_mp4(path, false)?;
        hls::MediaSegment::from_uri(&*mp4, extra)
    })?;
    Ok(box_response(http_file::serve_file(req, data).await))
}

// MP4 media info, json.
async fn media_info(req: &Request<()>, path: &str) -> io::Result<Response> {
    let file = task::block_in_place(|| fs::File::open(path))?;
    let mp4 = task::block_in_place(|| {
        mp4lib::streaming::lru_cache::open_mp4(path, false)
    })?;

    let info = mp4lib::track::track_info(&mp4);
    let body = serde_json::to_string_pretty(&info).unwrap();

    let data = http_file::MemFile::from_file(body.into_bytes(), "text/json", &file)?;
    Ok(box_response(http_file::serve_file(req, data).await))
}

async fn subtitle(req: &Request<()>, path: &str, into: &str) -> io::Result<Response> { 
    let file = task::block_in_place(|| fs::File::open(path))?;
    let (mime, body) =
        task::block_in_place(|| mp4lib::streaming::subtitle::external(path, into))?;
    let data = http_file::MemFile::from_file(body, mime, &file)?;
    Ok(box_response(http_file::serve_file(req, data).await))
}

fn translate_io_error(err: io::Error) -> StatusCode {
    use http::StatusCode as SC;
    match err.kind() {
        io::ErrorKind::NotFound => SC::NOT_FOUND,
        io::ErrorKind::PermissionDenied => SC::FORBIDDEN,
        io::ErrorKind::TimedOut => SC::REQUEST_TIMEOUT,
        io::ErrorKind::InvalidData => SC::BAD_REQUEST,
        _ => SC::INTERNAL_SERVER_ERROR,
    }
}

// helper to tunr the body in the response into axum's body type.
fn box_response<B>(resp: Response<B>) -> Response
where
    B: 'static + body::HttpBody<Data = Bytes> + Send,
    <B as body::HttpBody>::Error: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
{
    let (parts, body) = resp.into_parts();
    let body = axum::body::boxed(body);
    http::Response::from_parts(parts, body)
}
