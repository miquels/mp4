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
use bytes::Bytes;
use headers::{
    ContentLength, ContentRange, ETag, HeaderMapExt, IfMatch, IfModifiedSince, IfNoneMatch, IfRange,
    IfUnmodifiedSince, LastModified, Origin, Range,
};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use percent_encoding::percent_decode_str;
use scan_fmt::scan_fmt;
use structopt::StructOpt;
use tokio::task;
use warp::Filter;

use mp4lib::pseudo::Mp4Stream;

type Response<T = hyper::Body> = http::Response<T>;
type RespBuilder = http::response::Builder;

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
                    let extra = tail.as_str().to_string();
                    let resp = match Request::parse(dir, method, extra, query, headers) {
                        Ok(req) => route_request(req).await.unwrap_or_else(|e| e.into()),
                        Err(e) => e.into(),
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

async fn route_request(req: Request) -> Result<Response, Error> {
    if let Some(pseudo) = mp4pseudo(&req).await? {
        return Ok(pseudo);
    }
    if let Some(sub) = subtitle(&req).await? {
        return Ok(sub);
    }
    if let Some(hls) = hls(&req).await? {
        return Ok(hls);
    }
    if let Some(segment) = segment(&req).await? {
        return Ok(segment);
    }
    serve_file(&req).await
}

// HTTP error type.
#[derive(Debug)]
struct Error {
    status:  StatusCode,
    message: String,
}

impl Error {
    fn new(status: u16, message: impl Display) -> Error {
        let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        Error {
            status:  status.into(),
            message: message.to_string(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.status, self.message)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        match err.kind() {
            ErrorKind::NotFound => Error::new(404, err),
            ErrorKind::InvalidInput => Error::new(400, err),
            _ => Error::new(500, err),
        }
    }
}

impl From<Error> for Response {
    fn from(err: Error) -> Response {
        let body = match err.status.as_u16() {
            204 => hyper::Body::empty(),
            status => hyper::Body::from(format!("{} {}\n", status, err.message)),
        };
        Response::builder()
            .header("content-type", "text/plain")
            .status(err.status)
            .body(body)
            .unwrap()
    }
}

fn decode_path(path: &str, method: &Method) -> Result<String, Error> {
    // Check method.
    if method != &Method::GET && method != &Method::HEAD && method != &Method::OPTIONS {
        return Err(Error::new(405, "Method Not Allowed"));
    }

    // Decode path and check for shenanigans
    let path = match percent_decode_str(path).decode_utf8() {
        Ok(path) => path,
        Err(_) => return Err(Error::new(400, "Bad Request (path not utf-8)")),
    };
    if path
        .split('/')
        .any(|elem| elem == "" || elem == "." || elem == "..")
    {
        return Err(Error::new(400, "Bad Request (path elements invalid)"));
    }
    Ok(path.to_string())
}

// max.is_none(): always return inclusive bound
// max.is_some(): always return exclusive bound
fn bound(bound: std::ops::Bound<u64>, max: Option<u64>) -> u64 {
    match bound {
        Bound::Included(n) => {
            if max.is_some() {
                n + 1
            } else {
                n
            }
        },
        Bound::Excluded(n) => {
            if max.is_some() {
                n
            } else {
                n + 1
            }
        },
        Bound::Unbounded => max.unwrap_or(0),
    }
}

struct Request {
    method:  Method,
    path:    String,
    sep:     &'static str,
    extra:   String,
    params:  HashMap<String, String>,
    headers: HeaderMap,
    fpath:   String,
}

impl Request {
    // parse request.
    fn parse(
        dir: String,
        method: Method,
        path: String,
        query: String,
        headers: HeaderMap,
    ) -> Result<Request, Error> {
        let mut path = decode_path(&path, &method)?;
        let mut extra = String::new();
        let mut sep = "";

        // A path to an mp4 file can be followed by /extra/data.
        if let Some(idx) = path.rfind(".mp4/") {
            if path.len() > idx + 5 {
                extra.push_str(&path[idx + 5..]);
                path.truncate(idx + 4);
                sep = "/";
            }
        }

        // Some files can have :extra:data following. For example:
        //
        // - subtitles.srt:into.vtt
        // - subtitles.vtt:media.m3u8
        //
        if extra == "" {
            for ext in &[".srt:", ".vtt:"] {
                if let Some(idx) = path.rfind(ext) {
                    if path.len() > idx + 5 {
                        extra.push_str(&path[idx + 5..]);
                        path.truncate(idx + 4);
                        sep = ":";
                        break;
                    }
                }
            }
        }

        // query parameter
        let mut params = HashMap::new();
        for q in query.split('&') {
            let mut kv = q.splitn(2, '=');
            if let Some(key) = kv.next() {
                params.insert(key.to_string(), kv.next().unwrap_or("").to_string());
            }
        }

        // filesystem path
        let mut fpath = dir;
        while fpath.ends_with("/") {
            fpath.truncate(fpath.len() - 1);
        }
        if !path.starts_with("/") {
            fpath.push('/');
        }
        fpath.push_str(&path);

        Ok(Request {
            method,
            path,
            sep,
            extra,
            params,
            headers,
            fpath,
        })
    }

    // parse the Range: and If-Range: headers.
    fn parse_range(&self, fs: &FileServer) -> Result<Option<ops::Range<u64>>, Error> {
        if let Some(range) = self.headers.typed_get::<Range>() {
            let do_range = match self.headers.typed_get::<IfRange>() {
                Some(if_range) => !if_range.is_modified(Some(&fs.etag_hdr), Some(&fs.lastmod_hdr)),
                None => true,
            };
            if do_range {
                let ranges: Vec<_> = range.iter().collect();
                if ranges.len() > 1 {
                    return Err(Error::new(400, "multiple ranges not supported"));
                }
                let start = bound(ranges[0].0, None);
                let end = bound(ranges[0].1, Some(fs.size));
                if start >= end || start >= fs.size {
                    return Err(Error::new(416, "invalid range"));
                }
                return Ok(Some(ops::Range {
                    start: start,
                    end:   cmp::min(end, fs.size),
                }));
            }
        }
        Ok(None)
    }
}

struct FileServer {
    path:        String,
    file:        fs::File,
    modified:    SystemTime,
    size:        u64,
    etag_hdr:    ETag,
    lastmod_hdr: LastModified,
}

impl FileServer {
    // Open file.
    async fn open(path: impl Into<String>) -> io::Result<FileServer> {
        // open file.
        let path = path.into();
        let file = task::block_in_place(|| fs::File::open(&path))?;

        // get last_modified / inode / size.
        let meta = file.metadata()?;
        let modified = meta.modified().unwrap();
        let inode = meta.ino();
        let size = meta.len();

        // create etag
        let d = modified.duration_since(SystemTime::UNIX_EPOCH);
        let secs = d.map(|s| s.as_secs()).unwrap_or(0);
        let etag = format!("\"{:08x}.{:08x}.{}\"", secs, inode, size);
        let etag_hdr = ETag::from_str(&etag).unwrap();

        let lastmod_hdr = LastModified::from(modified);

        Ok(FileServer {
            path,
            file,
            modified,
            size,
            etag_hdr,
            lastmod_hdr,
        })
    }

    fn from_mp4stream(strm: &Mp4Stream) -> FileServer {
        FileServer {
            path:        strm.path().to_owned(),
            file:        strm.file().try_clone().unwrap(),
            modified:    strm.modified(),
            size:        strm.size(),
            etag_hdr:    ETag::from_str(&strm.etag()).unwrap(),
            lastmod_hdr: LastModified::from(strm.modified()),
        }
    }

    // check conditionals
    fn check_conditionals(&self, req: &Request) -> Result<(), Error> {
        // Check If-Match.
        if let Some(im) = req.headers.typed_get::<IfMatch>() {
            if !im.precondition_passes(&self.etag_hdr) {
                return Err(Error::new(412, "ETag does not match"));
            }
        } else {
            // Check If-Unmodified-Since.
            if let Some(iums) = req.headers.typed_get::<IfUnmodifiedSince>() {
                if !iums.precondition_passes(self.modified) {
                    return Err(Error::new(412, "resource was modified"));
                }
            }
        }

        // Check If-None-Match.
        if let Some(inm) = req.headers.typed_get::<IfNoneMatch>() {
            if !inm.precondition_passes(&self.etag_hdr) {
                return Err(Error::new(304, "Match"));
            }
        } else {
            if let Some(ims) = req.headers.typed_get::<IfModifiedSince>() {
                if !ims.is_modified(self.modified) {
                    return Err(Error::new(304, "Not modified"));
                }
            }
        }

        Ok(())
    }

    // build initial response headers.
    fn build_response(&self, req: &Request, cors: bool, range: bool) -> Result<RespBuilder, Error> {
        // build initial response.
        let mut response = http::Response::builder();
        let resp_headers = response.headers_mut().unwrap();
        resp_headers.typed_insert(self.etag_hdr.clone());
        resp_headers.typed_insert(self.lastmod_hdr.clone());

        if !cors {
            return Ok(response);
        }

        if let Some(host) = req.headers.typed_get::<Origin>() {
            resp_headers.insert(
                "access-control-allow-origin",
                HeaderValue::from_str(&host.to_string()).unwrap(),
            );
        } else {
            resp_headers.insert("access-control-allow-origin", HeaderValue::from_static("*"));
        }

        let (aca, ace) = if range {
            (
                "if-match, if-unmodified-since, if-range, range",
                "content-type, content-length, content-range",
            )
        } else {
            ("if-none-match, if-modified-since", "content-type, content-length")
        };
        resp_headers.insert("access-control-allow-headers", HeaderValue::from_static(aca));
        resp_headers.insert("access-control-expose-headers", HeaderValue::from_static(ace));

        Ok(response)
    }
}

async fn mp4pseudo(req: &Request) -> Result<Option<Response>, Error> {
    // get tracks, then open mp4stream.
    let tracks = match req.params.get("track") {
        Some(val) => {
            let tracks: Vec<_> = val.split(',').filter_map(|t| t.parse::<u32>().ok()).collect();
            if tracks.len() == 0 {
                return Err(Error::new(400, "bad track parameter"));
            }
            tracks
        },
        None => return Ok(None),
    };
    let mut mp4stream = mp4lib::pseudo::Mp4Stream::open(&req.fpath, tracks)?;

    // use FileServer for conditionals and initial response.
    let fs = FileServer::from_mp4stream(&mp4stream);
    fs.check_conditionals(req)?;

    let mut response = fs.build_response(req, false, false)?;
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
    resp_headers.insert("content-type", HeaderValue::from_static("video/mp4"));
    resp_headers.typed_insert(ContentLength(end - start));
    response = response.status(status);

    // if HEAD quit now
    if req.method == Method::HEAD {
        return Ok(response.body(hyper::Body::empty()).ok());
    }

    // Run content generation in a separate task
    let (mut tx, body) = hyper::Body::channel();
    task::spawn(async move {
        loop {
            let buflen = std::cmp::min(end - start, 128000) as usize;
            let mut buf = Vec::<u8>::new();
            buf.resize(buflen, 0);
            let result = task::block_in_place(|| mp4stream.read_at(&mut buf, start));
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

    Ok(response.body(body).ok())
}

async fn lookup_subtitles(mp4path: &str, subs: &mut Vec<String>) -> io::Result<()> {
    let dir = match std::path::Path::new(mp4path).parent() {
        Some(dir) => dir,
        None => return Ok(()),
    };
    task::block_in_place(|| {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if let Some(filename) = entry.file_name().to_str() {
                if filename.ends_with(".srt") || filename.ends_with(".vtt") {
                    subs.push(filename.to_string());
                }
            }
        }
        Ok::<_, io::Error>(())
    })
}

// serve m3u8 playlist files:
//
// - master.m3u8 / main.m3u8
// - media.<TRACK_ID>.m3u8
//
async fn hls(req: &Request) -> Result<Option<Response>, Error> {
    if !req.extra.ends_with(".m3u8") {
        return Ok(None);
    }

    // use FileServer for conditionals and initial response.
    let fs = FileServer::open(&req.fpath).await?;

    let track = if req.extra == "main.m3u8" || req.extra == "master.m3u8" {
        None
    } else {
        if let Ok(track) = scan_fmt!(&req.extra, "media.{}.m3u8{e}", u32) {
            Some(track)
        } else {
            return Err(Error::new(404, "playlist not found"));
        }
    };

    fs.check_conditionals(req)?;
    let mut response = fs.build_response(req, true, false)?;
    let resp_headers = response.headers_mut().unwrap();

    // if OPTIONS quit now
    if req.method == Method::OPTIONS {
        return Err(Error::new(204, ""));
    }

    // open and parse mp4 file.
    let mp4 = mp4lib::lru_cache::open_mp4(&req.fpath)?;

    let body = if let Some(track) = track {
        mp4lib::stream::hls_track(&mp4, track)?
    } else {
        let mut subs = Vec::new();
        let _ = lookup_subtitles(&fs.path, &mut subs).await;
        mp4lib::stream::hls_master(&mp4, Some(&subs))
    };
    resp_headers.insert("content-type", HeaderValue::from_static("application/x-mpegurl"));
    resp_headers.typed_insert(ContentLength(body.len() as u64));

    // if HEAD quit now
    if req.method == Method::HEAD {
        return Ok(response.body(hyper::Body::empty()).ok());
    }

    Ok(response.body(body.into()).ok())
}

async fn segment(req: &Request) -> Result<Option<Response>, Error> {
    let e = &req.extra;
    if !e.starts_with("a/") && !e.starts_with("v/") && !e.starts_with("s/") && !e.starts_with("init.") {
        return Ok(None);
    }

    // use FileServer for conditionals and initial response.
    let fs = FileServer::open(&req.fpath).await?;
    fs.check_conditionals(req)?;
    let mut response = fs.build_response(req, true, true)?;
    let resp_headers = response.headers_mut().unwrap();

    // if OPTIONS quit now
    if req.method == Method::OPTIONS {
        return Err(Error::new(204, ""));
    }

    // open and parse mp4 file.
    let mp4 = mp4lib::lru_cache::open_mp4(&req.fpath)?;

    let range = req.parse_range(&fs)?;
    let (mime, body, size) = mp4lib::stream::fragment_from_uri(&mp4, &req.extra, range.clone())?;
    resp_headers.insert("content-type", HeaderValue::from_static(mime));
    resp_headers.typed_insert(ContentLength(body.len() as u64));

    if let Some(mut range) = range {
        // adjust range to what we actually got.
        range.end = range.start + body.len() as u64;
        let cr = match ContentRange::bytes(range, size) {
            Ok(cr) => cr,
            Err(_) => return Err(Error::new(416, "Invalid Range")),
        };
        resp_headers.typed_insert(cr);
        response = response.status(206);
    }

    // if HEAD quit now
    if req.method == Method::HEAD {
        return Ok(response.body(hyper::Body::empty()).ok());
    }

    Ok(response.body(body.into()).ok())
}

async fn subtitle(req: &Request) -> Result<Option<Response>, Error> {
    if !req.path.ends_with(".srt") && !req.path.ends_with(".vtt") {
        return Ok(None);
    }
    if req.extra == "" {
        return Ok(None);
    }

    let fs = FileServer::open(&req.fpath).await?;
    fs.check_conditionals(req)?;
    let mut response = fs.build_response(req, true, false)?;
    let resp_headers = response.headers_mut().unwrap();

    // if OPTIONS quit now
    if req.method == Method::OPTIONS {
        return Err(Error::new(204, ""));
    }

    let (mime, body) = if req.extra.ends_with(".m3u8") {
        let duration = task::block_in_place(|| mp4lib::subtitle::duration(&fs.path))?;
        let mut path = fs.path.rsplit('/').next().unwrap().to_string();
        if !path.ends_with(".vtt") {
            path += ":into.vtt";
        }
        (
            "application/x-mpegurl",
            mp4lib::stream::hls_subtitle(&path, duration).into_bytes(),
        )
    } else {
        task::block_in_place(|| mp4lib::subtitle::external(&req.fpath, &req.extra))?
    };
    resp_headers.insert("content-type", HeaderValue::from_static(mime));
    resp_headers.typed_insert(ContentLength(body.len() as u64));

    // if HEAD quit now
    if req.method == Method::HEAD {
        return Ok(response.body(hyper::Body::empty()).ok());
    }

    Ok(response.body(body.into()).ok())
}

async fn serve_file(req: &Request) -> Result<Response, Error> {
    if req.sep != "" {
        return Err(Error::new(415, "Unsupported media type"));
    }

    let fs = FileServer::open(&req.fpath).await?;
    fs.check_conditionals(req)?;

    let mut response = fs.build_response(req, false, false)?;
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
