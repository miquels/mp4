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
use std::io;
use std::ops::{Bound, Range, RangeBounds};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::time::SystemTime;

use ambassador::delegatable_trait;
use once_cell::sync::Lazy;

#[delegatable_trait]
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
    fn get_etag(&self) -> Option<&str>;

    /// Set the HTTP ETag.
    fn set_etag(&mut self, tag: &str);

    /// Get the limited range we're serving.
    fn get_range(&self) -> std::ops::Range<u64>;

    /// Limit reading to a range.
    fn set_range(&mut self, range: impl std::ops::RangeBounds<u64>) -> std::io::Result<()>;

    /// Read data and advance file position.
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize>;

    /// MIME type.
    fn mime_type(&self) -> &str {
        "application/octet-stream"
    }
}

// Build an etag from file metadata.
//
// Include a version-specific field, so that different versions of
// this crate generate different etags. This is useful for content
// we generate from an mp4 file, that migh differ in different
// versions of this library.
//
// For now, during development, we use the timestamp of the executable
// that was built. TODO- use package version when we are stable.
pub(crate) fn build_etag(meta: fs::Metadata, version_specific: bool) -> String {
    let mut secs = 0u64;
    if let Ok(d) = meta.modified().map(|m| m.duration_since(SystemTime::UNIX_EPOCH)) {
        secs = d.map(|s| s.as_secs()).unwrap_or(0);
    };

    static EXE_STAMP: Lazy<u64> = Lazy::new(|| {
        std::env::current_exe()
            .and_then(|p| p.metadata())
            .map(|m| m.mtime() as u64)
            .unwrap_or(0)
    });
    let exe_stamp = *EXE_STAMP;

    if version_specific && exe_stamp > 0 {
        format!(
            "\"{:08x}.{:08x}.{:08x}.{}\"",
            exe_stamp,
            secs,
            meta.ino(),
            meta.size()
        )
    } else {
        format!("\"{:08x}.{:08x}.{}\"", secs, meta.ino(), meta.size())
    }
}

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
            fn get_etag(&self) -> Option<&str> {
                self.etag.as_ref().map(|t| t.as_str())
            }

            /// Set the HTTP ETag.
            fn set_etag(&mut self, tag: &str) {
                self.etag = Some(tag.to_string());
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
        let modified = meta.modified().ok();
        let etag = Some(build_etag(meta, true));

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

    /// Implementation of `HttpFile` for a plain filesystem file.
    pub struct FsFile {
        file: fs::File,
        path: Option<String>,
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
            let mut file = FsFile::from_file(fs::File::open(path)?, path)?;
            file.path = Some(path.to_string());
            Ok(file)
        }

        /// From an already opened file.
        pub fn from_file<'a>(file: fs::File, path: impl Into<Option<&'a str>>) -> io::Result<FsFile> {
            let path = path.into();
            let meta = file.metadata()?;
            let modified = meta.modified()?;
            let size = meta.len();

            let etag = build_etag(meta, false);
            let mime_type = match path.as_ref() {
                Some(path) => mime_guess::from_path(path).first_or_octet_stream().to_string(),
                None => "application/octet_stream".to_string(),
            };

            Ok(FsFile {
                file,
                path: path.map(|p| p.to_string()),
                size,
                start: 0,
                end: size,
                pos: 0,
                modified: Some(modified),
                etag: Some(etag),
                mime_type,
            })
        }

        /*
        /// Check if the file was not modified (based on LastModified / ETag).
        /// If so, returns a complete not-modified response.
        pub fn not_modified(&self) -> Option<Response> {
            let (response, not_modified) = check_modified(req, file);
            not_modified.then(move || response.body(Body::empty()).unwrap())
        }
        */
    }

    impl_http_file!(FsFile {
        /// Return the pathname of the open file.
        fn path(&self) -> Option<&str> {
            self.path.as_ref().map(|s| s.as_str())
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
        if let Some(etag) = file.get_etag() {
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
    pub async fn serve_file<F, R>(req: &http::Request<R>, mut file: F) -> http::Response<Body<F>>
    where
        F: HttpFile + Unpin + Send + 'static,
        http::Request<R>: Send + 'static,
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
                        .get_etag()
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
        if let Some(etag) = file.get_etag() {
            if let Some(inm) = req.headers().typed_get::<IfNoneMatch>() {
                let etag = ETag::from_str(&format!(r#""{}""#, etag)).unwrap();
                if inm.precondition_passes(&etag) {
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
pub use self::http_file_server::{serve_file, Body, FsFile};
