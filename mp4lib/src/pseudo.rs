//! HTTP Pseudo Streaming.
//!
//! This optimizes the MP4 file by:
//!
//! - putting the MovieBox at the front
//! - interleaving audio / video in 500ms chunks
//! - only including the audio track(s) you need
//!
//! The source file is not actually rewritten. A [`virtual`](Mp4Stream) file is generated,
//! on which you can call methods like `read`, `read_at`, and more.
//!
//! The main use-case for this is a HTTP server that serves and rewrites
//! files on-the-fly.
//!
use std::convert::TryInto;
use std::fs;
use std::hash::Hash;
use std::io;
use std::mem;
use std::os::unix::fs::MetadataExt;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use memmap::Mmap;
use once_cell::sync::Lazy;

use crate::boxes::*;
use crate::io::DataRef;
use crate::lru_cache::{open_mp4, LruCache};
use crate::mp4box::{MP4Box, MP4};
use crate::serialize::ToBytes;
use crate::types::FourCC;

// Key into INIT_SECTION / DATA_SECTION cache.
#[derive(Hash, PartialEq, Eq, Clone)]
struct SectionKey {
    path:   String,
    tracks: Vec<u32>,
}

/// An on-the-fly generated, streaming-optimized MP4 file.
pub struct Mp4Stream {
    key:          SectionKey,
    file:         fs::File,
    init_section: Option<Vec<u8>>,
    init_size:    u32,
    inode:        u64,
    modified:     SystemTime,
    size:         u64,
    pos:          u64,
    mmap:         Option<Mmap>,
}

impl Mp4Stream {
    /// Open an MP4 file.
    ///
    /// The opened file is virtual. It only contains the tracks indicated
    /// in the `tracks` argument. Also:
    ///
    /// - the tracks are interleaved in 500 ms periods, even if the original file wasn't
    /// - the MovieBox box is located at the front of the file rather than at the back.
    ///
    pub fn open(path: impl Into<String>, tracks: impl Into<Vec<u32>>) -> io::Result<Mp4Stream> {
        let path = path.into();
        let mut tracks = tracks.into();

        let file = fs::File::open(&path)?;
        let meta = file.metadata()?;
        let modified = meta.modified().unwrap();
        let inode = meta.ino();

        // This is kind of aribitraty.
        //
        // Depending on the file size, we either mmap the entire file, or,
        // every `read_at` the part we need. This is supposing that mapping
        // really large files is more expensive than mapping parts of it
        // multiple times. Is that actually true? XXX TESTME
        let mmap = if meta.len() < 750_000_000 && tracks.len() > 0 {
            Some(unsafe { Mmap::map(&file)? })
        } else {
            None
        };

        // If no tracks were selected, we choose the first video and the first audio track.
        if tracks.len() == 0 {
            let mp4 = open_mp4(&path, false)?;
            let info = crate::track::track_info(&mp4);
            if let Some(id) = info.iter().find(|t| t.track_type == "vide").map(|t| t.id) {
                tracks.push(id);
            }
            if let Some(id) = info.iter().find(|t| t.track_type == "soun").map(|t| t.id) {
                tracks.push(id);
            }
        }

        // prime the LRU cache.
        let key = SectionKey { path, tracks };
        let mapping = InitSection::mapping(&key)?;
        let init_size = mapping.init_size;
        let size = init_size as u64 + 16 + mapping.virt_size;

        Ok(Mp4Stream {
            key,
            file,
            init_section: None,
            init_size,
            inode,
            modified,
            size,
            pos: 0,
            mmap,
        })
    }

    /// Return the pathname of the open file.
    pub fn path(&self) -> &str {
        &self.key.path
    }

    /// Returns the size of the (virtual) file.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Returns the last modified time stamp.
    pub fn modified(&self) -> SystemTime {
        self.modified
    }

    /// Returns the HTTP ETag.
    pub fn etag(&self) -> String {
        let d = self.modified.duration_since(SystemTime::UNIX_EPOCH);
        let secs = d.map(|s| s.as_secs()).unwrap_or(0);
        format!("\"{:08x}.{:08x}.{}\"", secs, self.inode, self.size)
    }

    #[doc(hidden)]
    pub fn inode(&self) -> u64 {
        self.inode
    }

    #[doc(hidden)]
    pub fn file(&self) -> &fs::File {
        &self.file
    }

    /// Read data and advance file position.
    pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.read_at(buf, self.pos)?;
        self.pos += n as u64;
        Ok(n)
    }

    /// Read data at a certain offset.
    pub fn read_at(&mut self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
        //println!("0. read_at(buf[0..{}], {})", buf.len(), offset);
        let mut buf = buf;
        let mut offset = offset;
        let mut done = 0;

        // Does the offset start in the init section?
        if offset < self.init_size as u64 {
            // Yep, so read the init section.
            let init_section = match self.init_section.as_ref() {
                Some(data) => data,
                None => {
                    let init_section = InitSection::init_section(&self.key)?;
                    let mut buf = crate::io::MemBuffer::new();
                    init_section.init.write(&mut buf)?;
                    self.init_section = Some(buf.into_vec());
                    self.init_section.as_ref().unwrap()
                },
            };

            // Copy to buf.
            let init_size = self.init_size as usize;
            let u_offset = offset as usize;
            let len = std::cmp::min(buf.len(), init_size - u_offset);
            buf[..len].copy_from_slice(&init_section[u_offset..u_offset + len]);

            // advance buf and offset. if there is space in buf left,
            // we'll start reading from the MdatMapping.
            done = len;
            buf = &mut buf[len..];
            offset += len as u64;
        }

        if buf.len() == 0 {
            return Ok(done);
        }

        // Okay we have to map the mdat section.
        let mapping = InitSection::mapping(&self.key)?;
        let n = mapping.read_at(&self.file, self.mmap.as_ref(), buf, offset)?;
        done += n;
        //println!("read {} bytes ({} via mapping)", done, n);
        Ok(done)
    }
}

impl std::fmt::Debug for Mp4Stream {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut dbg = f.debug_struct("Mp4Stream");
        dbg.field("path", &self.key.path);
        dbg.field("tracks", &self.key.tracks);
        dbg.field("inode", &self.inode);
        dbg.field("modified", &self.modified);
        dbg.field("size", &self.size);
        dbg.field("etag", &self.etag());
        dbg.finish()
    }
}

// One entry per sample.
#[derive(Debug)]
struct MdatEntry {
    // Offset into the original MP4 file.
    mdat_offset: u64,
    // Offset into the generated 'virtual' mp4 file.
    virt_offset: u64,
    // Size of the sample.
    size:        u64,
}

// Mapping of the samples in the virtual metadata to the samples
// in the original metadata.
struct MdatMapping {
    map:       Vec<u8>,
    // virtual initialization section size.
    init_size: u32,
    // offset into original MP4 file.
    offset:    u64,
    // size of the data in the generated MediaDataBox.
    virt_size: u64,
}

impl MdatMapping {
    fn new(offset: u64) -> MdatMapping {
        MdatMapping {
            map: Vec::new(),
            init_size: 0,
            offset,
            virt_size: 0,
        }
    }

    fn push_offset(&mut self, offset: u64, name: &str) {
        let hi = offset >> 32;
        if hi > 0xff {
            panic!("MdatMapping::push: {} {} too large (>2^40)", name, offset);
        }
        self.map.push(hi as u8);
        let lo = (offset & 0xffffffff) as u32;
        self.map.extend_from_slice(&lo.to_ne_bytes());
    }

    fn push(&mut self, mdat_offset: u64, virt_offset: u64, size: u32) {
        self.push_offset(mdat_offset, "mdat_offset");
        self.push_offset(virt_offset, "virt_offset");
        // XXX TODO: optimization: size is virt_offset[index + 1] - virt_offset[index]
        self.map.extend_from_slice(&size.to_ne_bytes());
    }

    fn get(&self, index: usize) -> MdatEntry {
        let offset = index * 14;
        let data = &self.map[offset..offset + 14];
        let hi = data[0] as u64;
        let mdat_offset = u32::from_ne_bytes(data[1..5].try_into().unwrap()) as u64 | hi;
        let hi = data[5] as u64;
        let virt_offset = u32::from_ne_bytes(data[6..10].try_into().unwrap()) as u64 | hi;
        let size = u32::from_ne_bytes(data[10..14].try_into().unwrap());
        MdatEntry {
            virt_offset,
            mdat_offset,
            size: size as u64,
        }
    }

    fn read_at(
        &self,
        file: &fs::File,
        mmap: Option<&Mmap>,
        mut buf: &mut [u8],
        offset: u64,
    ) -> io::Result<usize> {
        // Some range checks.
        if offset < self.init_size as u64 {
            return Err(ioerr!(
                InvalidInput,
                "MdatMapping::read_at: invalid offset (<{})",
                self.offset
            ));
        }
        let mut offset = offset - self.init_size as u64;
        //println!("1. read_at(buf[0..{}], offset {}, size {}", buf.len(), offset, self.virt_size + 16);
        if offset >= self.virt_size + 16 {
            return Ok(0);
        }

        // The first 16 bytes are the MediaDataBox header.
        if offset < 16 {
            let mut data = [0u8; 16];
            let mut writer = &mut data[..];
            1u32.to_bytes(&mut writer)?;
            FourCC::new("mdat").to_bytes(&mut writer)?;
            (self.virt_size + 16).to_bytes(&mut writer)?;
            let len = std::cmp::min(buf.len(), (16 - offset) as usize);
            buf[..len].copy_from_slice(&data[offset as usize..offset as usize + len]);
            offset += len as u64;
            buf = &mut buf[len..];
            if offset < 16 {
                //println!("return Ok({})", len);
                return Ok(len);
            }
        }
        offset -= 16;
        //println!("2. read_at(buf[0..{}], offset {}", buf.len(), offset);

        // Now, start at an index close to where we think we need to be.
        let num_entries = self.map.len() / 14;
        let mut idx = (offset * num_entries as u64 / self.virt_size as u64) as usize;

        // If the target offset < first entry.virt_offset, we need search
        // upwards, otherwise downwards.
        let mut entries = Vec::new();
        let mut entry = self.get(idx);
        let up = entry.virt_offset < offset;
        //println!("idx: {}, up: {:?}", idx, up);

        loop {
            // If 'offset' falls in the range, it is the first matching entry.
            //println!("offset: {}, entry: {:?}", offset, entry);
            if offset >= entry.virt_offset && offset < entry.virt_offset + entry.size {
                // adjust so it starts at 'offset'.
                let delta = offset - entry.virt_offset;
                entry.virt_offset += delta;
                entry.mdat_offset += delta;
                entry.size -= delta;
                break;
            }
            if up {
                idx += 1;
                if idx >= num_entries {
                    panic!("MdatMapping::read_at: can't find entry for offset {}", offset);
                }
            } else {
                if idx == 0 {
                    panic!("MdatMapping::read_at: can't find entry for offset {}", offset);
                }
                idx -= 1;
            }
            entry = self.get(idx);
        }

        // Now collect entries until we have enough to fill 'buf', or reach EOF.
        let mut size = 0;
        loop {
            size += entry.size;
            entries.push(entry);
            if size >= buf.len() as u64 {
                break;
            }
            if idx + 1 >= num_entries {
                break;
            }
            idx += 1;
            entry = self.get(idx);
        }

        // Sort entries on the mdat_offset, so that we always read
        // forwards in the original MP4 file. Minimizes seeks.
        entries.sort_unstable_by(|a, b| a.mdat_offset.cmp(&b.mdat_offset));

        // Mmap the range we need, unless we already mmap'ed the whole file.
        let mut holder = None;
        let (start, data) = match mmap {
            Some(mmap) => (0, mmap),
            None => {
                let start = entries[0].mdat_offset;
                let end = entries[entries.len() - 1].mdat_offset + entries[entries.len() - 1].size;
                let data = unsafe {
                    memmap::MmapOptions::new()
                        .offset(start)
                        .len((end - start) as usize)
                        .map(file)
                }?;
                holder.replace(data);
                (start, holder.as_ref().unwrap())
            },
        };

        // and copy.
        let mut count = 0;
        for entry in &entries {
            let buf_index = (entry.virt_offset - offset) as usize;
            let sample_index = (entry.mdat_offset - start) as usize;
            let left = buf.len() - buf_index;
            let size = std::cmp::min(left, entry.size as usize);
            //println!("buf_index {}, buf.len {}, sample_index {}, data.len {}, size {}",
            //    buf_index, buf.len(), sample_index, data.len(), size);
            buf[buf_index..buf_index + size].copy_from_slice(&data[sample_index..sample_index + size]);
            count += size;
            //if left == size {
            //    break;
            //}
        }

        Ok(count as usize)
    }
}

// Per track rewritten boxes.
#[derive(Default)]
struct InitChunk {
    stsc: SampleToChunkBox,
    stco: ChunkOffsetBox,
}

// Rewritten init sections, for the specific tracks, and with interleaving.
#[rustfmt::skip]
static INIT_SECTIONS: Lazy<LruCache<SectionKey, Arc<InitSection>>> = {
    Lazy::new(|| LruCache::new(Duration::new(30, 0)))
};

// Mapping from the virtual mdat to the real mdat.
#[rustfmt::skip]
static MAPPINGS: Lazy<LruCache<SectionKey, Arc<MdatMapping>>> = {
    Lazy::new(|| LruCache::new(Duration::new(120, 0)))
};

// The InitSection is an MP4 file without the MediaData boxes,
// with only a selected set of tracks, and rewritten
// SampleToChunk boxes and ChunkOffset bxoes.
struct InitSection {
    init: MP4,
}

impl InitSection {
    fn init_section(key: &SectionKey) -> io::Result<Arc<InitSection>> {
        let init = match INIT_SECTIONS.get(key) {
            Some(init) => init,
            None => {
                let mp4 = open_mp4(&key.path, false)?;
                let (init, mapping) = InitSection::init_and_mapping(key, mp4.as_ref())?;
                let init = Arc::new(init);
                let mapping = Arc::new(mapping);
                INIT_SECTIONS.put(key.clone(), init.clone());
                MAPPINGS.put(key.clone(), mapping);
                init
            },
        };
        INIT_SECTIONS.expire();
        Ok(init)
    }

    fn mapping(key: &SectionKey) -> io::Result<Arc<MdatMapping>> {
        let mapping = match MAPPINGS.get(key) {
            Some(mapping) => mapping,
            None => {
                let mp4 = open_mp4(&key.path, false)?;
                let (init, mapping) = InitSection::init_and_mapping(key, mp4.as_ref())?;
                let init = Arc::new(init);
                let mapping = Arc::new(mapping);
                INIT_SECTIONS.put(key.clone(), init.clone());
                MAPPINGS.put(key.clone(), mapping.clone());
                mapping
            },
        };
        MAPPINGS.expire();
        Ok(mapping)
    }

    fn init_and_mapping(key: &SectionKey, mp4: &MP4) -> io::Result<(InitSection, MdatMapping)> {
        let mut tracks = Vec::new();
        let moov = mp4.movie();
        for track in &key.tracks {
            tracks.push(
                moov.track_by_id(*track)
                    .ok_or_else(|| ioerr!(NotFound, "track {} not found", track))?,
            );
        }
        let (chunks, mut mapping) = Self::interleave(mp4, &tracks[..]);
        let mut init = Self::build_init(key, mp4, chunks);
        let size = init.boxes.iter().fold(0, |acc, x| acc + x.size()) as u32;
        for track in init.movie_mut().tracks_mut().iter_mut() {
            track
                .media_mut()
                .media_info_mut()
                .sample_table_mut()
                .chunk_offset_table_mut()
                .add_offset(size as i64 + 16);
        }
        let init_section = InitSection { init };
        mapping.init_size = size;
        Ok((init_section, mapping))
    }

    fn build_init(key: &SectionKey, mp4: &MP4, mut new_chunks: Vec<InitChunk>) -> MP4 {
        let mut boxes = Vec::new();

        // First, loop over the top-level boxes. Copy those that we need.
        if let Some(ftyp) = first_box!(mp4, FileTypeBox) {
            boxes.push(ftyp.clone().to_mp4box());
        }
        if let Some(pdin) = first_box!(mp4, ProgressiveDownloadInfoBox) {
            boxes.push(pdin.clone().to_mp4box());
        }

        // Now process the MovieBox.
        let mut new_moov = MovieBox::default();
        let moov = mp4.movie();
        if let Some(mvhd) = first_box!(moov, MovieHeaderBox) {
            new_moov.boxes.push(mvhd.clone().to_mp4box());
        }
        if let Some(meta) = first_box!(moov, MetaBox) {
            new_moov.boxes.push(meta.clone().to_mp4box());
        }

        let mut new_track_id = 1;
        for track_id in &key.tracks {
            let trak = match moov.track_by_id(*track_id) {
                Some(trak) => trak,
                None => continue,
            };

            let mut chunks = new_chunks.remove(0);

            // Clone the track, then replace the ChunkOffsetBox and SampleToChunkBox.
            //
            // TODO: optimization: don't clone stco / stsc.
            let mut trak = trak.clone();
            let table = trak.media_mut().media_info_mut().sample_table_mut();
            for entry in &mut table.boxes {
                match entry {
                    MP4Box::ChunkOffsetBox(ref mut b) => mem::swap(&mut chunks.stco, b),
                    MP4Box::SampleToChunkBox(ref mut b) => mem::swap(&mut chunks.stsc, b),
                    _ => {},
                }
            }
            trak.set_track_id(new_track_id);
            new_track_id += 1;
            new_moov.boxes.push(trak.to_mp4box());
        }

        boxes.push(new_moov.to_mp4box());

        MP4 {
            data_ref: DataRef::default(),
            input_file: mp4.input_file.clone(),
            boxes,
        }
    }

    fn interleave(mp4: &MP4, tracks: &[&TrackBox]) -> (Vec<InitChunk>, MdatMapping) {
        // Initialize empty chunks vec (one entry per track).
        let mut chunks = Vec::new();
        for _ in tracks {
            chunks.push(InitChunk::default());
        }

        // Store an iterator for each track segment.
        let mut sample_info = Vec::new();
        for track in tracks {
            sample_info.push(track.sample_info_iter());
        }

        // Timescale per track.
        let mut timescale = Vec::new();
        for track in tracks {
            let ts = track.media().media_header().timescale as f64;
            timescale.push(ts);
        }

        let offset = match first_box!(mp4, MediaDataBox) {
            Some(mdat) => mdat.data.offset(),
            None => 0,
        };

        let mut mapping = MdatMapping::new(offset);
        let mut offset = 0_u64;
        let mut until = 0.5_f64;
        let duration = 0.5_f64;
        let mut done = false;

        while !done {
            done = true;

            // Now for each track, add 500ms of samples.
            for t in 0..tracks.len() {
                let mut num_samples = 0u32;
                let mut size = 0u32;

                while let Some(info) = sample_info[t].next() {
                    let decode_time = info.decode_time as f64 / timescale[t];
                    if decode_time >= until {
                        // "un-next" this entry.
                        sample_info[t].push(info);
                        break;
                    }

                    // Mapping
                    mapping.push(info.fpos, offset + size as u64, info.size);

                    num_samples += 1;
                    size += info.size;
                }

                if num_samples > 0 {
                    // add chunk offset entry.
                    chunks[t].stco.push(offset);

                    // and a sample to chunk entry.
                    let chunkno = chunks[t].stco.entries.len() as u32;
                    chunks[t].stsc.entries.push(SampleToChunkEntry {
                        first_chunk:              chunkno,
                        samples_per_chunk:        num_samples,
                        // FIXME; sample_description_index is hardcoded.
                        sample_description_index: 1,
                    });

                    offset += size as u64;
                    done = false;
                }
            }

            until += duration;
        }

        mapping.virt_size = offset;

        (chunks, mapping)
    }
}
