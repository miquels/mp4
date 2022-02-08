//! Abstraction for a `File` like object to be served over HTTP.
//!
//! `HttpFile` is a trait with just enough methods for it to be
//! used by an HTTP server to serve `GET` and `HEAD` requests.
//!
use std::cmp;
use std::fs;
use std::fmt::Write;
use std::io;
use std::ops::{Bound, Range, RangeBounds};
use std::os::unix::fs::MetadataExt;
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
pub(crate) fn exe_stamp() -> Option<(SystemTime, u64)> {
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

    #[allow(dead_code)]
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
pub(crate) use impl_http_file;

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

