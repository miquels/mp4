//! Read and write MP4 / ISOBMFF containers.
//!
//! There are several other crates that let you read an MP file into
//! a set of structures, but none of them let you write one.
//!
//! This crate was created for an HTTP server that can rewrite
//! mp4-files on the fly, as they are served:
//!
//! - put the MOOV box at the front of the file
//! - extract tx3g subtitles as vtt or srt.
//! - rearrange the order of tracks.
//! - get details of the file in json format.
//!
//! This prints some `mediainfo` like info for an mp4 file.
//!
//! ```no_run
//! use mp4::{Mp4File, MP4};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let file = std::env::args().next().expect("expected filename");
//!
//!     let mut reader = Mp4File::open(&file)?;
//!     let mp4 = MP4::read(&mut reader)?;
//!     let res = mp4::track::track_info(&mp4);
//!     println!("{:#?}", res);
//!
//!     Ok(())
//!  }
//!
//! ```
//! In general, you start by opening the file with [`Mp4File`](crate::io::Mp4File), then
//! reading it with [`MP4::read`](crate::mp4box::MP4::read). That returns a [`MP4`](crate::mp4box::MP4)
//! struct. The method [`mp4.movie`](crate::mp4box::MP4::movie) gets you a
//! [`MovieBox`](crate::boxes::MovieBox) and from there you can inspect the tracks, etc.
//!
#[macro_use]
mod ioerr;
#[macro_use]
#[doc(hidden)]
pub mod macros;
#[macro_use]
pub mod serialize;
#[macro_use]
pub mod types;
mod bitreader;
//mod global;
pub mod boxes;
pub mod debug;
pub mod fragment;
pub mod io;
pub mod lru_cache;
pub mod mp4box;
pub mod pseudo;
pub mod rewrite;
pub(crate) mod sample_info;
pub mod segment;
pub mod stream;
pub mod subtitle;
pub mod track;

pub use crate::io::Mp4File;
pub use crate::mp4box::MP4;
