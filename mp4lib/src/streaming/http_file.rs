//! Abstraction for a `File` like object to be served over HTTP.
//!
use std::fs;
use std::io;
use std::ops::{Bound, Range, RangeBounds};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::time::SystemTime;

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
    fn modified(&self) -> Option<SystemTime>;

    /// Returns the HTTP ETag.
    fn get_etag(&self) -> Option<&str>;

    /// Set the HTTP ETag.
    fn set_etag(&mut self, tag: &str);

    /// Get the limited range we're serving.
    fn get_range(&self) -> Range<u64>;

    /// Limit reading to a range.
    fn set_range(&mut self, range: impl RangeBounds<u64>) -> io::Result<()>;

    /// Read data and advance file position.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
}

/// Implementation of `HttpFile` for a plain filesystem file.
pub struct FsFile {
    file:   Option<fs::File>,
    path:   Option<String>,
    size:   u64,
    etag:   Option<String>,
    start:  u64,
    end:    u64,
    pos:    u64,
    modified: Option<SystemTime>,
    content:    Vec<u8>,
}

impl FsFile {
    /// Open file.
    pub fn open(path: &str) -> io::Result<FsFile> {
        let mut file = FsFile::from_file(fs::File::open(path)?)?;
        file.path = Some(path.to_string());
        Ok(file)
    }

    /// From an already opened file.
    pub fn from_file(file: fs::File) -> io::Result<FsFile> {
        let meta = file.metadata()?;
        let modified = meta.modified()?;

        let d = modified.duration_since(SystemTime::UNIX_EPOCH);
        let secs = d.map(|s| s.as_secs()).unwrap_or(0);
        let etag = format!("\"{:08x}.{:08x}.{}\"", secs, meta.ino(), meta.size());

        Ok(FsFile {
            file: Some(file),
            path: None,
            size: meta.len(),
            etag: Some(etag),
            start: 0,
            end: meta.len(),
            pos: 0,
            modified: Some(modified),
            content: Vec::new(),
        })
    }

    /// Serve different content.
    ///
    /// This is used when you open a file, but then serve data generated
    /// from the file - such as a CMAF segment. Timestamp and etag remain
    /// the same, content differs.
    pub fn set_content(&mut self, content: Vec<u8>) {
        self.content = content;
        self.file = None;
        self.size = self.content.len() as u64;
        self.pos = 0;
        self.start = 0;
        self.end = self.content.len() as u64;
    }
}

impl HttpFile for FsFile {
    /// Return the pathname of the open file.
    fn path(&self) -> Option<&str> {
        self.path.as_ref().map(|s| s.as_str())
    }

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

    /// Read data and advance file position.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos == self.end {
            return Ok(0);
        }
        let mut buf = buf;
        if self.pos + buf.len() as u64 > self.end {
            let max = (self.end - self.pos as u64) as usize;
            buf = &mut buf[..max];
        }
        let n = match self.file.as_ref() {
            Some(file) => file.read_at(buf, self.pos)?,
            None => {
                buf.copy_from_slice(&self.content[self.pos as usize .. self.pos as usize + buf.len()]);
                buf.len()
            },
        };
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.pos += n as u64;
        Ok(n)
    }
}

#[cfg(feature = "http-file-server")]
mod http {
   use std::cmp;
   use std::future::Future;
   use std::io;
   use std::pin::Pin;
   use std::str::FromStr;
   use std::task::{Context, Poll};
   use std::time::SystemTime;
   
   use bytes::Bytes;
   use futures_core::Stream;
   use headers::{
       AcceptRanges, ContentLength, ContentRange, ETag, HeaderMapExt,  IfModifiedSince,
       IfNoneMatch, IfRange, LastModified, Range as HttpRange, Date,
   };
   use http::{Method, StatusCode};

   use super::HttpFile;

   /// Serve a `HttpFile`.
   ///
   /// This function takes care of:
   ///
   /// - `GET` and `HEAD` methods.
   /// - checking conditionals (If-Modified-Since, If-Range, etc)
   /// - rejecting invalid requests
   /// - serving a range
   ///
   /// It does not handle OPTIONS and it does not set CORS headers.
   ///
   /// CORS headers can be set after this function returns, or
   /// it can be done by middleware.
   ///
   pub async fn serve_file<F, R>(req: &http::Request<R>, mut file: F) -> http::Response<Body<F>>
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
   
       // not modified?
       if let Some(status) = check_conditionals(req, &file) {
           return response.status(status).body(Body::empty()).unwrap();
       }
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
           }
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
                   let etag = file.get_etag().and_then(|t| ETag::from_str(&format!(r#""{}""#, t)).ok());
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
   
   /// Body that implements Stream<Item=Bytes>.
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
               file:None,
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
   
   impl<F> Stream for Body<F>
   where
       F: HttpFile + Unpin + Send + 'static,
   {
       type Item = io::Result<Bytes>;
   
       fn poll_next(
           mut self: Pin<&mut Self>,
           cx: &mut Context<'_>
       ) -> Poll<Option<Self::Item>> {
   
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
pub use self::http::{serve_file, Body};

