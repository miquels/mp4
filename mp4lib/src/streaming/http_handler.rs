//! Handler for HTTP requests.
//!
//! This module has functions that handle HTTP requests. These so-called
//! `handlers` take as arguments a `http::Request`, a filesystem `base directory`, and
//! then generate a `http::Response`. You can use this with an HTTP server like `axum` or
//! `hyper` or `warp` etcetera.
//!
//! - [`handle_hls`](handle_hls)
//!
//! When passed an URL like `..../movie.mp4/master.m3u8`, serves the `movie.mp4`
//! file as a `HLS` stream.
//!
//! - [`handle_pseudo`](handle_pseudo)
//!
//! This serves an MP4 file, but re-interleaved and web-optimized. It also
//! serves json information about the file's tracks, and it can serve
//! `srt` files in `vtt` format.
//!
//! - [`handle_file`](handle_file)
//!
//! Just for completeness, we can also serve regular files.
//!
use std::cmp;
use std::fs;
use std::future::Future;
use std::io::{self, Error as IoError, ErrorKind};
use std::ops::{Bound, Range, RangeBounds};
use std::os::unix::fs::FileExt;
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};
use std::time::SystemTime;

use bytes::Bytes;
use futures_core::Stream;
use headers::{AcceptRanges, ContentLength, ContentRange, Date, ETag, HeaderMapExt};
use headers::{IfModifiedSince, IfNoneMatch, IfRange, LastModified, Range as HttpRange, UserAgent};
use http::{header, Method, Request, Response, StatusCode};
use percent_encoding::percent_decode_str;
use tokio::task;

use super::http_file::{self, HttpFile, MemFile};
use super::{hls, pseudo};

macro_rules! regex {
    ($re:expr $(,)?) => {{
        static RE: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
        RE.get_or_init(|| regex::Regex::new($re).unwrap())
    }};
}

trait BoxResponseBody {
    fn box_body(self) -> http::Response<BoxBody>;
}

#[cfg(feature = "axum-box-body")]
mod box_response_body {
    use http_body::Body as HttpBody;
    use super::BoxResponseBody;
    pub type BoxBody = axum::body::BoxBody;

    // The `axum::body::boxed` helper has an optimization where, if
    // a body is already boxed, it won't be boxed again. So if possible,
    // enable the "axum-box-body" feature and enjoy that optimization.
    impl<B> BoxResponseBody for http::Response<B>
    where
        B: HttpBody<Data = bytes::Bytes, Error = std::io::Error> + Send + 'static,
    {
        fn box_body(self) -> http::Response<BoxBody> {
            let (parts, body) = self.into_parts();
            let body = axum::body::boxed(body);
            http::Response::from_parts(parts, body)
        }
    }
}

#[cfg(feature = "hyper-body")]
mod box_response_body {
    use futures_core::Stream;
    use super::BoxResponseBody;
    pub type BoxBody = hyper::Body;

    // We simply use hyper::Body, which is also pretty efficient.
    // This interoperates well with the 'poem' framework.
    impl<B> BoxResponseBody for http::Response<B>
    where
        B: Stream<Item = std::io::Result<bytes::Bytes>> + Send + 'static,
    {
        fn box_body(self) -> http::Response<BoxBody> {
            let (parts, body) = self.into_parts();
            let body = hyper::Body::wrap_stream(body);
            http::Response::from_parts(parts, body)
        }
    }
}

#[cfg(not(any(feature = "axum-box-body", feature = "hyper-body")))]
mod box_response_body {
    use http_body::Body as HttpBody;
    use super::BoxResponseBody;
    pub type BoxBody = http_body::combinators::UnsyncBoxBody<bytes::Bytes, std::io::Error>;

    // This double-boxes, unfortunately.
    impl<B> BoxResponseBody for http::Response<B>
    where
        B: HttpBody<Data = bytes::Bytes, Error = std::io::Error> + Send + 'static,
    {
        fn box_body(self) -> http::Response<BoxBody> {
            let (parts, body) = self.into_parts();
            let body = body.boxed_unsync();
            http::Response::from_parts(parts, body)
        }
    }
}

use box_response_body::*;

/// The type of path used by the handler.
#[derive(Clone, Copy)]
pub enum FsPath<'a> {
    /// Use the path from the request URI as filesystem path.
    FromRequest,
    /// Append the path from the request URI to this base directory.
    BaseDir(&'a str),
    /// Combine this basedirectory and path.
    Combine((&'a str, &'a str)),
    /// Use this absolute filesystem path.
    Absolute(&'a str),
}

impl<'a> FsPath<'a> {
    fn resolve(&self, req: &http::Request<()>) -> io::Result<String> {
        match *self {
            FsPath::FromRequest => decode_path(req.uri().path()),
            FsPath::BaseDir(base) => join_paths(base, &decode_path(req.uri().path())?),
            FsPath::Combine((base, path)) => join_paths(base, path),
            FsPath::Absolute(path) => Ok(path.to_string()),
        }
    }
}

/// Handle `HLS` `URLs`.
///
/// Handles the main entry point `...../movie.mp4/master.m3u8`.
///
/// That is a playlist which contains URLs to a playlist per track which
/// contain URLs to media segments, all of which are of the form  
/// `...../movie.mp4/<url_tail>`.
///
/// Returns `Ok(None)` if this was not a `HLS` related request.
///
pub async fn handle_hls(
    req: &Request<()>,
    path: FsPath<'_>,
    filter_subs: bool,
) -> io::Result<Option<Response<BoxBody>>> {
    let path = path.resolve(req)?;

    const PATH_AND_EXTRA: &'static str = r#"^(.*\.mp4)/(.*\.(?:m3u8|mp4|m4a|vtt))$"#;
    let caps = match regex!(PATH_AND_EXTRA).captures(&path) {
        Some(caps) => caps,
        None => return Ok(None),
    };
    let (path, extra) = (&caps[1], &caps[2]);

    if let Some(response) = not_modified(&req, path).await {
        return Ok(Some(response));
    }

    // Chromecast cannot handle segments > 8M
    // Should we handle this here, or should it be an
    // argument to `handle_hls` ?
    let max_segment_size = match req.headers().typed_get::<UserAgent>() {
      Some(ua) if ua.as_str().contains("CrKey/") => Some(8_000_000),
      _ => None,
    };

    // HLS manifest.
    if extra.ends_with(".m3u8") {
        let data = task::block_in_place(|| {
            let mp4 = super::lru_cache::open_mp4(path, false)?;
            hls::HlsManifest::from_uri(&*mp4, extra, filter_subs, max_segment_size)
        })?;
        return Ok(Some(serve_file(req, data.0).await.box_body()));
    }

    // Media data.
    if extra.ends_with(".mp4") || extra.ends_with(".m4a") || extra.ends_with(".vtt") {
        let data = task::block_in_place(|| {
            let mp4 = super::lru_cache::open_mp4(path, false)?;
            hls::MediaSegment::from_uri(&*mp4, extra, range_end(req))
        })?;
        return Ok(Some(serve_file(req, data.0).await.box_body()));
    }

    Ok(None)
}

fn range_end(req: &Request<()>) -> Option<u64> {
    let range = req.headers().typed_get::<HttpRange>()?.iter().next()?;
    use std::ops::Bound::*;
    match range.1 {
        Included(n) => Some(n + 1),
        Excluded(n) => Some(n),
        Unbounded => Some(0),
    }
}

/// Handle `pseudostreaming` `URLs`.
///
/// This handler handles three types of URLs:
///
/// - `/...../movie.mp4/info.json`  
///   return movie and track information in `JSON` format
///
/// - `/...../movie.srt:into.vtt`  
///   translate `srt` subtitle file into `vtt` format.
///
/// - `/...../movie.mp4?track_id=1,2,3`  
///   serve a version of the MP4 file with only tracks `1`, `2`, and `3`.
///   See also the [`pseudo`](crate::streaming::pseudo) module.
///
pub async fn handle_pseudo(req: &Request<()>, path: FsPath<'_>) -> io::Result<Option<Response<BoxBody>>> {
    use std::collections::HashMap;
    use std::iter::FromIterator;

    let path = path.resolve(req)?;

    // External subtitle format translation (subtitle.srt:into.vtt).
    const SUBTITLE: &'static str = r#"^(.*\.(?:srt|vtt)):into\.(srt|vtt)$"#;
    if let Some(caps) = regex!(SUBTITLE).captures(&path) {
        let (path, extra) = (&caps[1], &caps[2]);
        if let Some(response) = not_modified(&req, path).await {
            return Ok(Some(response));
        }
        let data = task::block_in_place(|| {
            let file = fs::File::open(&path)?;
            let (mime, body) = super::subtitle::external(path, extra)?;
            http_file::MemFile::from_file(body, mime, &file)
        })?;
        return Ok(Some(serve_file(req, data).await.box_body()));
    }

    // Info.
    const INFO: &'static str = r#"^(.*\.mp4)/info.json$"#;
    if let Some(caps) = regex!(INFO).captures(&path) {
        let path = &caps[1];
        if let Some(response) = not_modified(&req, path).await {
            return Ok(Some(response));
        }
        let file = task::block_in_place(|| fs::File::open(&caps[1]))?;
        let mp4 = task::block_in_place(|| super::lru_cache::open_mp4(path, false))?;

        let info = crate::track::track_info(&mp4);
        let body = serde_json::to_string_pretty(&info).unwrap();

        let data = http_file::MemFile::from_file(body.into_bytes(), "text/json", &file)?;
        return Ok(Some(serve_file(req, data).await.box_body()));
    }

    if !path.ends_with(".mp4") {
        return Ok(None);
    }

    // Pseudo streaming.
    let q = match req.uri().query() {
        Some(q) => q,
        None => return Ok(None),
    };
    let params = q.split('&').map(|p| {
        let mut kv = p.splitn(2, '=');
        (kv.next().unwrap(), kv.next().unwrap_or(""))
    });
    let query = HashMap::<&str, &str>::from_iter(params);
    let track_id = match query.get("track_id") {
        Some(t) => t,
        None => return Ok(None),
    };

    // get tracks, then open mp4stream.
    let tracks: Vec<_> = track_id
        .split(',')
        .filter_map(|t| t.parse::<u32>().ok())
        .collect();
    if tracks.len() == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "400 bad track parameter"));
    }

    if let Some(response) = not_modified(&req, &path).await {
        return Ok(Some(response));
    }

    let mp4stream = task::block_in_place(|| pseudo::Mp4Stream::open(path, tracks))?;
    Ok(Some(serve_file(req, mp4stream).await.box_body()))
}

/// Handle a standard file.
pub async fn handle_file(req: &Request<()>, path: FsPath<'_>, index: Option<&str>) -> io::Result<Response<BoxBody>> {
    let mut path = path.resolve(req)?;

    let file = match FsFile::open(&path) {
        Ok(file) => file,
        Err(err) => {
            // We could not open the file. Might be a directory. Check if we care.
            let index = match index {
                Some(index) => index,
                None => return Err(err),
            };

            // If we're going to use index.html this _must_ be a directory.
            let meta_res = task::block_in_place(|| fs::metadata(&path));
            let is_dir = meta_res.map(|m| m.is_dir()).unwrap_or(false);
            if !is_dir {
                return Err(err);
            }

            // If the path did not end with '/', send a redirect.
            if !path.ends_with("/") {
                path.push('/');
                let response = http::response::Builder::new()
                    .header("Location", &path)
                    .status(StatusCode::TEMPORARY_REDIRECT)
                    .body::<Body>(Body::empty())
                    .unwrap();
                return Ok(response.box_body());
            }

            // Try to open the index.
            path.push_str(index);
            FsFile::open(&path)?
        },
    };

    Ok(serve_file(req, file).await.box_body())
}

/// Early check to see if we can send a `HTTP` `Not Modified` response.
///
/// Functions like `HlsManifest::from_uri` and `MediaData::from_uri` generate
/// data based on the contents of an MP4 file. The `Last-Modified` header
/// is the exact same as that of the file, and the `ETag` header is derived
/// from the etag of the file. We can calculate those values in advance,
/// without actually processing the mp4 file.
///
/// This function calculates and checks those values and returns a
/// complete `Not Modified` response if appropriate.
///
pub async fn not_modified<R>(req: &http::Request<R>, file_path: &str) -> Option<http::Response<BoxBody>>
where
    http::Request<R>: Send + 'static,
{
    // If we have a If-None-Match header with an ETag in it,
    // then use the ETag parts indicated by the first hex number.
    let mut etag_parts = http_file::E::FILE;
    if let Some(inm) = req.headers().get("if-none-match") {
        if let Ok(val) = inm.to_str() {
            if let Some(caps) = regex!(r#"\.E([0-9a-fA-F]{2,8})""#).captures(val) {
                etag_parts = u32::from_str_radix(&caps[1], 16).unwrap();
            }
        }
    }

    // Now open the file.
    let mut file = task::block_in_place(|| FsFile::open2(file_path, etag_parts)).ok()?;

    // If this is a generated file, the timestamp cannot be earlier than
    // that of the executable.
    if req.uri().path().contains(".mp4/")
        || req.uri().path().contains(".into:")
        || req.uri().query().is_some()
    {
        if let Some(m) = file.modified.as_mut() {
            if let Some((exe, _)) = http_file::exe_stamp() {
                if *m < exe {
                    *m = exe;
                }
            }
        }
    }

    // And check.
    let (response, not_modified) = check_modified(req, &file);
    not_modified.then(move || response.body::<Body>(Body::empty()).unwrap().box_body())
}

/// Implementation of `HttpFile` for a plain filesystem file.
pub struct FsFile {
    file: fs::File,
    path: String,
    size: u64,
    start: u64,
    end: u64,
    pos: u64,
    modified: Option<SystemTime>,
    etag: Option<String>,
    mime_type: String,
}

impl FsFile {
    /// Open file.
    pub fn open(path: &str) -> io::Result<FsFile> {
        FsFile::open2(path, http_file::E::FILE)
    }

    fn open2(path: &str, etag_parts: u32) -> io::Result<FsFile> {
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(err) => return Err(map_io_error(err)),
        };
        let meta = file.metadata()?;
        let modified = meta.modified().ok();
        let size = meta.len();

        let etag = http_file::build_etag(meta, etag_parts);
        let mime_type = mime_guess::from_path(path).first_or_octet_stream().to_string();

        Ok(FsFile {
            file,
            path: path.to_string(),
            size,
            start: 0,
            end: size,
            pos: 0,
            modified,
            etag: Some(etag),
            mime_type,
        })
    }
}

use http_file::impl_http_file;
impl_http_file!(FsFile {
    /// Return the pathname of the open file.
    fn path(&self) -> Option<&str> {
        Some(self.path.as_str())
    }

    /// Read data and advance file position.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos == self.end {
            return Ok(0);
        }
        let max = cmp::min(buf.len() as u64, self.end - self.pos) as usize;
        let n = self.file.read_at(&mut buf[..max], self.pos)?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.pos += n as u64;
        Ok(n)
    }
});

fn check_modified<F, R>(req: &http::Request<R>, file: &F) -> (http::response::Builder, bool)
where
    F: HttpFile + Unpin + Send + 'static,
    http::Request<R>: Send + 'static,
{
    // build minimal response headers.
    let mut response = http::Response::builder();
    let resp_headers = response.headers_mut().unwrap();
    if let Some(etag) = file.etag() {
        let etag = ETag::from_str(&format!(r#""{}""#, etag)).unwrap();
        resp_headers.typed_insert(etag);
    }
    if let Some(modified) = file.modified() {
        resp_headers.typed_insert(LastModified::from(modified));
    }
    resp_headers.typed_insert(Date::from(SystemTime::now()));
    if let Some(status) = check_conditionals(req, file) {
        return (response.status(status), true);
    }
    (response, false)
}

/// Serve a `HttpFile`.
///
/// This function takes care of:
///
/// - `GET` and `HEAD` methods.
/// - checking conditionals (`If-Modified-Since`, `If-Range`, etc)
/// - rejecting invalid requests
/// - serving a range
///
/// It does not handle `OPTIONS` and it does not set `CORS` headers.
///
/// `CORS` headers can be set on the `Response` after this function returns,
/// or it can be handled by middleware.
///
pub async fn serve_file<B, F>(req: &http::Request<B>, mut file: F) -> http::Response<Body<F>>
where
    F: HttpFile + Unpin + Send + 'static,
    B: Send + 'static,
    http::Request<B>: Send + 'static,
{
    let (mut response, not_modified) = check_modified(req, &file);
    if not_modified {
        return response.body(Body::empty()).unwrap();
    }
    let resp_headers = response.headers_mut().unwrap();
    let mut status = StatusCode::OK;

    // ranges.
    resp_headers.typed_insert(AcceptRanges::bytes());
    match parse_range(req, &mut file) {
        Ok(false) => {},
        Ok(true) => {
            resp_headers.typed_insert(ContentRange::bytes(file.get_range(), file.size()).unwrap());
            status = StatusCode::PARTIAL_CONTENT;
        },
        Err(status) => {
            if status == StatusCode::RANGE_NOT_SATISFIABLE {
                resp_headers.typed_insert(ContentRange::unsatisfied_bytes(file.size()));
            }
            return response.status(status).body(Body::empty()).unwrap();
        },
    }
    resp_headers.typed_insert(ContentLength(file.range_size()));
    resp_headers.insert(header::CONTENT_TYPE, file.mime_type().parse().unwrap());

    // just HEAD?
    if *req.method() == Method::HEAD {
        return response.status(status).body(Body::empty()).unwrap();
    }

    // GET response.
    response.status(status).body(Body::new(file)).unwrap()
}

fn parse_range<F, R>(req: &http::Request<R>, file: &mut F) -> Result<bool, StatusCode>
where
    F: HttpFile + Unpin + Send + 'static,
{
    if let Some(range) = req.headers().typed_get::<HttpRange>() {
        let do_range = match req.headers().typed_get::<IfRange>() {
            Some(if_range) => {
                let lms = file.modified().map(|m| LastModified::from(m));
                let etag = file
                    .etag()
                    .and_then(|t| ETag::from_str(&format!(r#""{}""#, t)).ok());
                !if_range.is_modified(etag.as_ref(), lms.as_ref())
            },
            None => true,
        };
        if do_range {
            let ranges: Vec<_> = range.iter().collect();
            if ranges.len() > 1 {
                // Should this be 416?
                return Err(StatusCode::BAD_REQUEST);
            }
            if file.set_range(ranges[0]).is_err() {
                return Err(StatusCode::RANGE_NOT_SATISFIABLE);
            }
            return Ok(true);
        }
    }
    Ok(false)
}

fn check_conditionals<F, R>(req: &http::Request<R>, file: &F) -> Option<StatusCode>
where
    F: HttpFile + Unpin + Send + 'static,
    http::Request<R>: Send + 'static,
{
    // Check If-None-Match.
    if let Some(etag) = file.etag() {
        if let Some(inm) = req.headers().typed_get::<IfNoneMatch>() {
            let etag = ETag::from_str(&format!(r#""{}""#, etag)).unwrap();
            if !inm.precondition_passes(&etag) {
                return Some(StatusCode::NOT_MODIFIED);
            }
        }
    }

    // Check If-Modified-Since.
    if let Some(lms) = file.modified() {
        if let Some(ims) = req.headers().typed_get::<IfModifiedSince>() {
            if !ims.is_modified(lms) {
                return Some(StatusCode::NOT_MODIFIED);
            }
        }
    }

    None
}

/// Body that implements `Stream<Item=Bytes>`, as wel as `http_body::Body`.
// pub struct Body<F: HttpFile + Unpin + Send + 'static> {
pub struct Body<F=MemFile> {
    file: Option<F>,
    todo: u64,
    in_place: bool,
    join_handle: Option<task::JoinHandle<(F, Option<io::Result<Bytes>>)>>,
}

impl<F> Body<F>
where
    F: HttpFile + Unpin + Send + 'static,
{
    pub fn new(http_file: F) -> Body<F> {
        Body {
            todo: http_file.range_size(),
            in_place: true,
            join_handle: None,
            file: Some(http_file),
        }
    }

    pub fn empty() -> Body<F> {
        Body {
            todo: 0,
            in_place: true,
            join_handle: None,
            file: None,
        }
    }
}

fn do_read<F: HttpFile + Unpin + Send + 'static>(file: &mut F, todo: u64) -> Option<io::Result<Bytes>> {
    let mut buf = Vec::<u8>::new();
    buf.resize(cmp::min(todo, 128000u64) as usize, 0);

    match file.read(&mut buf) {
        Ok(0) => None,
        Ok(n) => {
            buf.truncate(n);
            Some(Ok(Bytes::from(buf)))
        },
        Err(e) => Some(Err(e)),
    }
}

impl<F> http_body::Body for Body<F>
where
    F: HttpFile + Unpin + Send + 'static,
{
    type Data = Bytes;
    type Error = io::Error;

    fn poll_data(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        self.poll_next(cx)
    }

    fn poll_trailers(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<Option<http::HeaderMap>, Self::Error>> {
        Poll::Ready(Ok(None))
    }

    fn size_hint(&self) -> http_body::SizeHint {
        if let Some(file) = self.file.as_ref() {
            if let Some(left) = file.bytes_left() {
                return http_body::SizeHint::with_exact(left);
            }
        }
        http_body::SizeHint::new()
    }
}

impl<F> Stream for Body<F>
where
    F: HttpFile + Unpin + Send + 'static,
{
    type Item = io::Result<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.as_mut().get_mut();
        if this.join_handle.is_none() && this.file.is_none() {
            return Poll::Ready(None);
        }

        let res = if this.in_place {
            task::block_in_place(|| do_read(this.file.as_mut().unwrap(), this.todo))
        } else {
            if this.join_handle.is_none() {
                let mut file = this.file.take().unwrap();
                let todo = this.todo;
                this.join_handle = Some(task::spawn_blocking(move || {
                    let res = do_read(&mut file, todo);
                    (file, res)
                }));
            }
            let handle = this.join_handle.as_mut().unwrap();
            tokio::pin!(handle);
            let res = match handle.poll(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Ok((file, res))) => {
                    this.file.get_or_insert(file);
                    res
                },
                Poll::Ready(Err(e)) => Some(Err(io::Error::new(io::ErrorKind::Other, e))),
            };
            this.join_handle.take();
            res
        };

        if let Some(Ok(buf)) = res.as_ref() {
            this.todo -= buf.len() as u64;
        }

        Poll::Ready(res)
    }
}

fn decode_path(path: &str) -> io::Result<String> {
    match percent_decode_str(path).decode_utf8() {
        Ok(path) => Ok(path.to_string()),
        Err(_) => return Err(IoError::new(ErrorKind::InvalidData, "400 Bad Request (path not utf-8)")),
    }
}

fn join_paths(dir: &str, path: &str) -> io::Result<String> {
    let mut elems = Vec::new();
    for elem in path.split('/').filter(|e| !e.is_empty()) {
        match elem {
            "." => continue,
            ".." => {
                if elems.is_empty() {
                    return Err(IoError::new(ErrorKind::InvalidData, "400 Bad Request (invalid path)"));
                }
                elems.pop();
            },
            _ => elems.push(elem),
        }
    }
    let mut path = dir.to_string();
    if !path.ends_with("/") {
        path.push('/');
    }
    path.push_str(&elems.join("/"));
    Ok(path)
}

// remap EISDIR to ENOENT so that requests for
// .../movie.mp4/typo correctly return "404 not found"
// instead of "500 internal server error".
fn map_io_error(err: io::Error) -> io::Error {
    match err.raw_os_error() {
        Some(libc::ENOTDIR) => io::Error::from_raw_os_error(libc::ENOENT),
        Some(libc::EISDIR) => io::Error::from_raw_os_error(libc::ENOENT),
        _ => return err,
    }
}
