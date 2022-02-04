//! MP4 fragmenting / streaming / rewriting.
//!
//! This module and submodules contain helpers to transmux a MP4
//! file on-the-fly while streaming it over HTTP.
//!
//! You probably want to start at [`pseudo`](crate::streaming::pseudo) or
//! [`hls`](crate::streaming::hls).
//!
//! Note, `transmuxing` is not `transcoding`.
#[macro_use]
pub mod http_file;
pub mod fragment;
pub mod hls;
pub mod lru_cache;
pub mod pseudo;
pub mod segment;
pub mod subtitle;
