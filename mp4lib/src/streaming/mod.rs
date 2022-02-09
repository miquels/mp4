//! MP4 fragmenting / streaming / rewriting.
//!
//! This module and submodules contain helpers to transmux a MP4
//! file on-the-fly while streaming it over HTTP.
//!
//! You probably want to start at [`pseudo`](crate::streaming::pseudo) or
//! [`hls`](crate::streaming::hls).
//!
//! Note, `transmuxing` is not `transcoding`.
pub mod http_file;
pub mod fragment;
pub mod hls;
pub mod lru_cache;
pub mod pseudo;
pub mod segmenter;
pub mod subtitle;

#[cfg_attr(docsrs, doc(cfg(feature = "http-handler")))]
#[cfg(feature = "http-handler")]
pub mod http_handler;
