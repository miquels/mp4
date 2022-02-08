use std::fs;
use std::io;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};

use anyhow::Result;
use axum::{AddExtensionLayer, Router, body, response::Response, routing::get};
use axum::extract::{Extension, Path, Query};
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

    if let Some(ref log_opts) = opts.log {
        let opts = if !log_opts.contains("=") {
            format!("tower={0},tower_http={0},mp4lib={0},mp4server={0}", log_opts)
        } else {
            log_opts.to_string()
        };
        tracing_subscriber::fmt().with_env_filter(&opts).init();
    } else if let Ok(ref log_opts) = std::env::var("RUST_LOG") {
        tracing_subscriber::fmt().with_env_filter(log_opts).init();
    } else {
        tracing_subscriber::fmt().with_env_filter("info").init();
    }

    match opts.cmd {
        Command::Serve(opts) => return serve(opts).await,
    }
}

// Serve files.
async fn serve(opts: ServeOpts) -> Result<()> {
    use http::header::{HeaderName, ORIGIN, RANGE};

    let dir = opts.dir.clone();

    let middleware_stack = ServiceBuilder::new()
        .layer(TraceLayer::new_for_http())
        .layer(AddExtensionLayer::new(dir))
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
        let (r1, r2) = tokio::join!(
            axum::Server::bind(&sockaddr_v4).serve(app.into_make_service()),
            axum::Server::bind(&sockaddr_v6).serve(app2.into_make_service()),
        );
        r1.expect("IPv4 server");
        r2.expect("IPv6 server");
    }

    #[cfg(not(target_os = "freebsd"))]
    {
        let addr = IpAddr::V6(Ipv6Addr::from(0u128));
        let sockaddr = SocketAddr::new(addr, opts.port);
        axum::Server::bind(&sockaddr).serve(app.into_make_service()).await.unwrap();
    }

    Ok(())
}

#[derive(Deserialize, Debug)]
pub struct Params {
    pub track_id:   String,
}

async fn handle_request(
    Path(path): Path<String>,
    params: Option<Query<Params>>,
    Extension(dir): Extension<String>,
    req: Request<body::Body>,
) -> Result<Response, StatusCode> {
    let (parts, _) = req.into_parts();
    let req = Request::from_parts(parts, ());
    let params = match params {
        None => None,
        Some(Query(params)) => Some(params),
    };
    handle_request2(path, params, dir, req).await.map_err(|e| translate_io_error(e))
}

// This is not the best way to join paths, but it is reasonably secure.
fn join_paths(dir: &str, path: &str) -> String {
    let mut elems = Vec::new();
    for elem in path.split('/').filter(|e| !e.is_empty()) {
        match elem {
            "." => continue,
            ".." => { elems.pop(); },
            _ => elems.push(elem),
        }
    }
    let mut path = dir.to_string();
    if !path.ends_with("/") {
        path.push('/');
    }
    path.push_str(&elems.join("/"));
    path
}

async fn handle_request2(
    path: String,
    params: Option<Params>,
    dir: String,
    req: Request<()>
) -> io::Result<Response> {

    let path = join_paths(&dir, &path);

    const PATH_AND_EXTRA: &'static str = r#"^(.*\.mp4)/(info|.*\.(?:m3u8|mp4|m4a|vtt))$"#;
    if let Some(caps) = regex!(PATH_AND_EXTRA).captures(&path) {
        let (path, extra) = (&caps[1], &caps[2]);

        if let Some(response) = http_file::not_modified(&req, path).await {
            return Ok(box_response(response));
        }

        // HLS manifest.
        if extra.ends_with(".m3u8") {
            return manifest(&req, path, extra).await;
        }

        // Media data.
        if extra.ends_with(".mp4") || extra.ends_with(".m4a") {
            return media_segment(&req, path, extra).await;
        }

        // Media info.
        if extra == "info" {
            return media_info(&req, path).await;
        }
    }

    // External subtitle format translation (subtitle.srt:into.vtt).
    const SUBTITLE: &'static str = r#"^(.*\.(?:srt|vtt)):into\.(srt|vtt)$"#;
    if let Some(caps) = regex!(SUBTITLE).captures(&path) {
        if let Some(response) = http_file::not_modified(&req, &caps[1]).await {
            return Ok(box_response(response));
        }
        return subtitle(&req, &caps[1], &caps[2]).await;
    }

    // Normal file (or pseudo streaming, still file path is normal). Check.
    if let Some(response) = http_file::not_modified(&req, &path).await {
        return Ok(box_response(response));
    }

    // Pseudo-streaming request.
    if path.ends_with(".mp4") && params.is_some() {
        return mp4pseudo(&req, &path, params.unwrap()).await;
    }

    // A normal file.
    let file = http_file::FsFile::open(&path)?;
    Ok(box_response(http_file::serve_file(&req, file).await))
}
    
async fn mp4pseudo(req: &Request<()>, path: &str, params: Params) -> io::Result<Response> {

    // get tracks, then open mp4stream.
    let tracks: Vec<_> = params.track_id.split(',').filter_map(|t| t.parse::<u32>().ok()).collect();
    if tracks.len() == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad track parameter"));
    }

    let mp4stream = pseudo::Mp4Stream::open(path, tracks)?;
    Ok(box_response(http_file::serve_file(req, mp4stream).await))
}

// serve m3u8 playlist files:
//
// - master.m3u8 / main.m3u8
// - media.<TRACK_ID>.m3u8
//
async fn manifest(req: &Request<()>, path: &str, extra: &str) -> io::Result<Response> {

    // See if this is the Notflix custom receiver running on Chromecast.
    let is_notflix = match req.headers().get("x-application").map(|v| v.to_str()) {
        Some(Ok(v)) => v.contains("Notflix"),
        _ => false,
    };
    // Is it a chromecast?
    let is_cast = match req.headers().typed_get::<UserAgent>() {
        Some(ua) => ua.as_str().contains("CrKey/"),
        None => false,
    };
    // Chromecast and not Notflix, filter subs.
    let filter_subs = is_cast && !is_notflix;

    let data = task::block_in_place(|| {
        let mp4 = mp4lib::streaming::lru_cache::open_mp4(path, false)?;
        hls::HlsManifest::from_uri(&*mp4, extra, filter_subs)
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
