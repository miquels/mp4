use std::io;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};

use anyhow::Result;
use axum::extract::{ConnectInfo, Extension, Path};
use axum::{body, response::Response, routing::get, Router};
use headers::{HeaderMapExt, UserAgent};
use http::{Method, Request, StatusCode};
use http_body::Body as _;
use structopt::StructOpt;
use tower::ServiceBuilder;
use tower_http::compression::{
    predicate::{DefaultPredicate, NotForContentType, Predicate},
    CompressionLayer,
};
use tower_http::cors::{self, CorsLayer};
use tower_http::trace::TraceLayer;

use mp4lib::streaming::http_handler::{self, FsPath};

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

    let x_app = HeaderName::from_static("x-application");
    let x_plb = HeaderName::from_static("x-playback-session-id");

    let compress_predicate = DefaultPredicate::new()
        .and(NotForContentType::const_new("movie/"))
        .and(NotForContentType::const_new("audio/"));

    let middleware_stack = ServiceBuilder::new()
        .layer(TraceLayer::new_for_http())
        .layer(Extension(dir))
        .layer(CompressionLayer::new().compress_when(compress_predicate))
        .layer(
            CorsLayer::new()
                .allow_origin(cors::AllowOrigin::mirror_request())
                .allow_methods(vec![Method::GET, Method::HEAD])
                .allow_headers(vec![x_app, x_plb, ORIGIN, RANGE])
                .expose_headers(cors::Any)
                .max_age(std::time::Duration::from_secs(86400)),
        );

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
        let svc_v4 = app.clone().into_make_service_with_connect_info::<SocketAddr>();
        let svc_v6 = app.into_make_service_with_connect_info::<SocketAddr>();
        let (r1, r2) = tokio::join!(
            axum::Server::bind(&sockaddr_v4).serve(svc_v4),
            axum::Server::bind(&sockaddr_v6).serve(svc_v6),
        );
        r1.expect("IPv4 server");
        r2.expect("IPv6 server");
    }

    #[cfg(not(target_os = "freebsd"))]
    {
        let addr = IpAddr::V6(Ipv6Addr::from(0u128));
        let sockaddr = SocketAddr::new(addr, opts.port);
        let svc = app.into_make_service_with_connect_info::<SocketAddr>();
        axum::Server::bind(&sockaddr).serve(svc).await.unwrap();
    }

    Ok(())
}

async fn handle_request(
    Path(path): Path<String>,
    Extension(dir): Extension<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<body::Body>,
) -> Result<Response, StatusCode> {
    let (parts, _) = req.into_parts();
    let req = Request::from_parts(parts, ());
    let start = std::time::Instant::now();

    let res = handle_request2(path, dir, &req)
        .await
        .map_err(|e| translate_io_error(e));

    let now = time::OffsetDateTime::now_local().unwrap_or(time::OffsetDateTime::now_utc());
    let (size, status) = match res.as_ref() {
        Ok(resp) => (resp.body().size_hint().exact().unwrap_or(0), resp.status()),
        Err(sc) => (0, *sc),
    };
    let pnq = req
        .uri()
        .path_and_query()
        .map(|p| p.to_string())
        .unwrap_or(String::from("-"));
    println!(
        "{} {} \"{} {} {:?}\" {} {} {:?}",
        now,
        addr,
        req.method(),
        pnq,
        req.version(),
        status.as_u16(),
        size,
        start.elapsed(),
    );

    res
}

async fn handle_request2(path: String, dir: String, req: &Request<()>) -> io::Result<Response> {
    let path = FsPath::Combine((&dir, &path));

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

    if let Some(response) = http_handler::handle_hls(&req, path, filter_subs).await? {
        return Ok(response);
    }

    if let Some(response) = http_handler::handle_pseudo(&req, path).await? {
        return Ok(response);
    }

    http_handler::handle_file(&req, path, None).await
}

fn translate_io_error(err: io::Error) -> StatusCode {
    use http::StatusCode as SC;
    match err.kind() {
        io::ErrorKind::NotFound => SC::NOT_FOUND,
        io::ErrorKind::PermissionDenied => SC::FORBIDDEN,
        io::ErrorKind::TimedOut => SC::REQUEST_TIMEOUT,
        _ => {
            let e = err.to_string();
            let field = e.split_whitespace().next().unwrap();
            if let Ok(status) = field.parse::<u16>() {
                SC::from_u16(status).unwrap_or(SC::INTERNAL_SERVER_ERROR)
            } else {
                SC::INTERNAL_SERVER_ERROR
            }
        },
    }
}
