//! MP4 fragmenting / streaming / rewriting.
//!
//! This module and submodules contain helpers to transmux a MP4
//! file on-the-fly while streaming it over HTTP.
//!
//! Note, `transmuxing` is not `transcoding`.
pub mod fragment;
pub mod hls;
pub mod http_file;
pub mod lru_cache;
pub mod pseudo;
pub mod rewrite;
pub mod segment;
pub mod subtitle;
