//! Abstraction for a `File` like object to be served over HTTP.
//!
//! `HttpFile` is a trait with just enough methods for it to be
//! used by an HTTP server to serve `GET` and `HEAD` requests.
//!
//! If the `http-file-server` feature is enabled, you also get
//! `FsFile` and `serve_file`.
//!
use std::cmp;
use std::fs;
use std::fmt::Write;
use std::io;
use std::ops::{Bound, Range, RangeBounds};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::time::SystemTime;

use once_cell::sync::Lazy;

/// Methods for a struct that can be served via HTTP.
///
/// Several structs in this library are meant to be served over HTTP.
/// The `serve_file` function can serve structs that implement this trait.
pub trait HttpFile {
    /// Return the pathname of the open file (if any).
    fn path(&self) -> Option<&str> {
        None
    }

    /// Returns the size of the (virtual) file.
    fn size(&self) -> u64;

    /// Returns the size of the range.
    fn range_size(&self) -> u64;

    /// Returns the last modified time stamp.
    fn modified(&self) -> Option<std::time::SystemTime>;

    /// Returns the HTTP ETag.
    fn etag(&self) -> Option<&str>;

    /// Get the limited range we're serving.
    fn get_range(&self) -> std::ops::Range<u64>;

    /// Limit reading to a range.
    fn set_range(&mut self, range: impl std::ops::RangeBounds<u64>) -> std::io::Result<()>;

    /// Read data and advance file position.
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize>;

    /// How much data is left to read.
    fn bytes_left(&self) -> Option<u64> {
        None
    }

    /// MIME type.
    fn mime_type(&self) -> &str {
        "application/octet-stream"
    }
}

// Delegate macro, used by newtypes wrapping MemFile (such as HlsManifest).
macro_rules! delegate_http_file {
    ($type:ty) => {
        impl HttpFile for $type {
            fn path(&self) -> Option<&str> {
                self.0.path()
            }
            fn size(&self) -> u64 {
                self.0.size()
            }
            fn range_size(&self) -> u64 {
                self.0.range_size()
            }
            fn modified(&self) -> Option<std::time::SystemTime> {
                self.0.modified()
            }
            fn etag(&self) -> Option<&str> {
                self.0.etag()
            }
            fn get_range(&self) -> std::ops::Range<u64> {
                self.0.get_range()
            }
            fn set_range(&mut self, range: impl std::ops::RangeBounds<u64>) -> std::io::Result<()> {
                self.0.set_range(range)
            }
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                self.0.read(buf)
            }
            fn mime_type(&self) -> &str {
                self.0.mime_type()
            }
        }
    }
}
pub(crate) use delegate_http_file;

// Return timestamp of current executable, both as SystemTime and unix timestamp.
fn exe_stamp() -> Option<(SystemTime, u64)> {
    static EXE_STAMP: Lazy<Option<(SystemTime, u64)>> = Lazy::new(|| {
        std::env::current_exe()
            .and_then(|p| p.metadata())
            .map(|m| (m.modified().unwrap(), m.mtime() as u64))
            .ok()
    });
    *EXE_STAMP
}

// Return crate version as a single number.
fn crate_version() -> u64 {
    static CRATE_VERSION: Lazy<u64> = Lazy::new(|| {
        let mut v = 0;
        let mut r = 0;
        for n in env!("CARGO_PKG_VERSION").split(|c| c == '.' || c == '-' || c == '_') {
            if let Ok(x) = n.parse::<u16>() {
                r  = r * 256 + (x as u64);
                v += 1;
                if v == 3 {
                    break;
                }
            }
        }
        r
    });
    *CRATE_VERSION
}

// Build an etag from file metadata.
//
// The etag is formed of a set of fields separated by dots. The
// last field is in the form `E<hex-number`. The `hex-number` is
// a bitfield that indicates what fields are present.
//
// This enables us to re-create the etag from parts on a `If-Non-Match`
// check without interpreting the URL.
pub(crate) struct E(u32);
impl E {
    pub const MODIFIED: u32 = 1;
    pub const INODE: u32 = 2;
    pub const SIZE: u32 = 4;
    pub const EXE_STAMP: u32 = 8;
    pub const CRATE_VERSION: u32 = 16;

    pub const FILE: u32 = Self::MODIFIED | Self::INODE | Self::SIZE;
    pub const GENERATED: u32 = Self::MODIFIED | Self::INODE | Self::SIZE | Self::EXE_STAMP;

    pub fn has(&self, part: u32) -> bool {
        (self.0 & part) > 0
    }
}

// Build the tag from parts.
pub(crate) fn build_etag(meta: fs::Metadata, parts: u32) -> String {
    let parts = E(parts);
    let mut used = 0;
    let mut tag = String::new();

    let dot = |used| if used > 0 { "." } else { "" };

    if parts.has(E::MODIFIED) {
        if let Ok(d) = meta.modified().map(|m| m.duration_since(SystemTime::UNIX_EPOCH)) {
            if let Ok(secs) = d.map(|s| s.as_secs()) {
                let _ = write!(&mut tag, "{}{:x}", dot(used), secs);
                used |= E::MODIFIED;
            }
        }
    }
    if parts.has(E::INODE) {
        let _ = write!(&mut tag, "{}{:x}", dot(used), meta.ino());
        used |= E::INODE;
    }
    if parts.has(E::SIZE) {
        let _ = write!(&mut tag, "{}{:x}", dot(used), meta.len());
        used |= E::SIZE;
    }
    if parts.has(E::EXE_STAMP) {
        if let Some((_, tm)) = exe_stamp() {
            let _ = write!(&mut tag, "{}{:x}", dot(used), tm);
            used |= E::EXE_STAMP;
        }
    }
    if parts.has(E::CRATE_VERSION) {
        let _ = write!(&mut tag, "{}{:x}", dot(used), crate_version());
        used |= E::CRATE_VERSION;
    }
    let _ = write!(&mut tag, ".E{:02x}", used);

    tag
}

// MemFile and FsFile share much of the same members and methods,
// so put the common implementation in a macro for re-use.
// This is where inheritance would come in handy, really.
macro_rules! impl_http_file {
    ($type:ty { $($methods:tt)* }) => {

        impl HttpFile for $type {
            /// Returns the size of the (virtual) file.
            fn size(&self) -> u64 {
                self.size
            }

            /// Returns the size of the range.
            fn range_size(&self) -> u64 {
                self.end - self.start
            }

            /// Returns the last modified time stamp.
            fn modified(&self) -> Option<SystemTime> {
                self.modified
            }

            /// Returns the HTTP ETag.
            fn etag(&self) -> Option<&str> {
                self.etag.as_ref().map(|t| t.as_str())
            }

            /// Get the limited range we're serving.
            fn get_range(&self) -> Range<u64> {
                Range{ start: self.start, end: self.end }
            }

            /// Limit reading to a range.
            fn set_range(&mut self, range: impl RangeBounds<u64>) -> io::Result<()> {
                self.start = match range.start_bound() {
                    Bound::Included(&n) => n,
                    Bound::Excluded(&n) =>  n + 1,
                    Bound::Unbounded => 0,
                };
                self.end = match range.end_bound() {
                    Bound::Included(&n) => n + 1,
                    Bound::Excluded(&n) => n,
                    Bound::Unbounded => self.size,
                };

                if self.start >= self.size {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, "range out of bounds"));
                }
                if self.end > self.size {
                    self.end = self.size;
                }
                self.pos = self.start;

                Ok(())
            }

            /// Return the MIME type of this content.
            fn mime_type(&self) -> &str {
                self.mime_type.as_str()
            }

            /// How much data is left to read.
            fn bytes_left(&self) -> Option<u64> {
                Some(self.end - self.pos)
            }

            $($methods)*
        }
    }
}

/// Implementation of `HttpFile` for an in-memory file.
pub struct MemFile {
    start: u64,
    end: u64,
    size: u64,
    pos: u64,
    modified: Option<SystemTime>,
    etag: Option<String>,
    mime_type: String,
    pub(crate) content: Vec<u8>,
}

impl MemFile {
    /// New MemFile.
    pub fn new(content: Vec<u8>, mime_type: &str) -> MemFile {
        MemFile {
            start: 0,
            end: content.len() as u64,
            size: content.len() as u64,
            pos: 0,
            modified: None,
            etag: None,
            mime_type: mime_type.to_string(),
            content,
        }
    }

    /// Referring to an already opened file for modified time / etag.
    pub fn from_file<'a>(
        content: Vec<u8>,
        mime_type: impl Into<String>,
        file: &fs::File,
    ) -> io::Result<MemFile> {
        let mime_type = mime_type.into();
        let meta = file.metadata()?;
        let mut modified = meta.modified().ok();
        let etag = Some(build_etag(meta, E::GENERATED));

        // Timestamp of generated file is never older than that
        // of the current executable.
        if let Some(m) = modified.as_mut() {
            if let Some((exe, _)) = exe_stamp() {
                if *m < exe {
                    *m = exe;
                }
            }
        }

        Ok(MemFile {
            start: 0,
            end: content.len() as u64,
            size: content.len() as u64,
            etag,
            pos: 0,
            modified,
            mime_type,
            content,
        })
    }
}

impl_http_file!(MemFile {
    /// Read data and advance file position.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos == self.end {
            return Ok(0);
        }
        let size = cmp::min(buf.len() as u64, self.end - self.pos) as usize;
        let pos = self.pos as usize;
        buf[..size].copy_from_slice(&self.content[pos .. pos + size]);
        self.pos += size as u64;
        Ok(size)
    }
});

#[cfg_attr(docsrs, doc(cfg(feature = "http-file-server")))]
#[cfg(feature = "http-file-server")]
mod http_file_server {
    use std::cmp;
    use std::fs;
    use std::future::Future;
    use std::io;
    use std::pin::Pin;
    use std::str::FromStr;
    use std::task::{Context, Poll};
    use std::time::SystemTime;

    use bytes::Bytes;
    use futures_core::Stream;
    use headers::{
        AcceptRanges, ContentLength, ContentRange, Date, ETag, HeaderMapExt, IfModifiedSince, IfNoneMatch,
        IfRange, LastModified, Range as HttpRange,
    };
    use http::{Method, StatusCode};

    use super::*;

    type EmptyBody = http_body::Empty<Bytes>;

    macro_rules! regex {
        ($re:expr $(,)?) => {{
            static RE: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
            RE.get_or_init(|| regex::Regex::new($re).unwrap())
        }};
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
    pub async fn not_modified<R>(req: &http::Request<R>, file_path: &str) -> Option<http::Response<EmptyBody>>
    where
        http::Request<R>: Send + 'static,
    {
        // If we have a If-None-Match header with an ETag in it,
        // then use the ETag parts indicated by the first hex number.
        let mut etag_parts = E::FILE;
        if let Some(inm) = req.headers().get("if-none-match") {
            if let Ok(val) = inm.to_str() {
                if let Some(caps) = regex!(r#"\.E([0-9a-fA-F]{2,8})""#).captures(val) {
                    etag_parts = u32::from_str_radix(&caps[1], 16).unwrap();
                }
            }
        }

        // Now open the file.
        let mut file = tokio::task::block_in_place(|| FsFile::open2(file_path, etag_parts)).ok()?;

        // If this is a generated file, the timestamp cannot be earlier than
        // that of the executable.
        if req.uri().path().contains(".mp4/")
            || req.uri().path().contains(".into:")
            || req.uri().query().is_some()
        {
            if let Some(m) = file.modified.as_mut() {
                if let Some((exe, _)) = exe_stamp() {
                    if *m < exe {
                        *m = exe;
                    }
                }
            }
        }

        // And check.
        file.not_modified(req)
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
            FsFile::open2(path, E::FILE)
        }

        fn open2(path: &str, etag_parts: u32) -> io::Result<FsFile> {
            let file = fs::File::open(path)?;
            let meta = file.metadata()?;
            let modified = meta.modified().ok();
            let size = meta.len();

            let etag = build_etag(meta, etag_parts);
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

        // Check if the file was not modified (based on LastModified / ETag).
        // If so, returns a complete not-modified response.
        fn not_modified<R>(&self, req: &http::Request<R>) -> Option<http::Response<EmptyBody>>
        where
            http::Request<R>: Send + 'static,
        {
            let (response, not_modified) = check_modified(req, self);
            not_modified.then(move || response.body(EmptyBody::new()).unwrap())
        }
    }

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
    pub struct Body<F: HttpFile + Unpin + Send + 'static> {
        file: Option<F>,
        todo: u64,
        in_place: bool,
        join_handle: Option<tokio::task::JoinHandle<(F, Option<io::Result<Bytes>>)>>,
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
                tokio::task::block_in_place(|| do_read(this.file.as_mut().unwrap(), this.todo))
            } else {
                if this.join_handle.is_none() {
                    let mut file = this.file.take().unwrap();
                    let todo = this.todo;
                    this.join_handle = Some(tokio::task::spawn_blocking(move || {
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
}
#[cfg(feature = "http-file-server")]
pub use self::http_file_server::{not_modified, serve_file, Body, FsFile};
