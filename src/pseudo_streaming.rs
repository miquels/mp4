//! HTTP Pseudo Streaming.
//!
//! This optimizes the MP4 file by:
//!
//! - putting the MovieBox at the front
//! - interleaving of audio / video in 500ms chunks
//! - only including the audio track(s) you need
//!
use std::borrow::Borrow;
use std::convert::TryInto;
use std::hash::Hash;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};

use once_cell::sync::Lazy;

use crate::mp4box::{MP4, MP4Box};
use crate::io::Mp4File;
use crate::boxes::{SampleToChunkBox, SampleToChunkEntry, ChunkOffsetBox, TrackBox, MediaDataBox};

// The original, unmodified MP4s.
static MP4_FILES: Lazy<LruCache<String, Arc<MP4>>> = Lazy::new(|| LruCache::new(Duration::new(60, 0)));

// Rewritten init sections, for the specific tracks, and with interleaving.
static INIT_SECTIONS: Lazy<LruCache<SectionKey, Arc<InitSection>>> = Lazy::new(|| LruCache::new(Duration::new(10, 0)));

// Mappings of the rewritten data sections, for the specific tracks, and with interleaving.
static DATA_SECTIONS: Lazy<LruCache<SectionKey, Arc<DataSection>>> = Lazy::new(|| LruCache::new(Duration::new(60, 0)));

// Key into INIT_SECTION / DATA_SECTION cache.
#[derive(Hash, PartialEq, Eq, Clone)]
struct SectionKey {
    path:   String,
    tracks: Vec<u32>,
}

/// An on-the-fly streaming-optimized MP4 file.
pub struct Mp4Stream {
    path:   String,
    init:   Arc<InitSection>,
    mp4:    Arc<MP4>,
    pos:    u64,
}

impl Mp4Stream {
    // Open an MP4 file.
    pub fn open(path: impl Into<String>, tracks: impl Into<Vec<u32>>) -> io::Result<Mp4Stream> {
        let path = path.into();
        let tracks = tracks.into();

        let mp4 = match MP4_FILES.get(&path) {
            Some(mp4) => mp4,
            None => {
                let mut reader = Mp4File::open(&path)?;
                let mp4 = Arc::new(MP4::read(&mut reader)?);
                MP4_FILES.put(path.clone(), mp4.clone());
                mp4
            },
        };

        let init_key = SectionKey {
            path: path.clone(),
            tracks,
        };
        let init = match INIT_SECTIONS.get(&init_key) {
            Some(init) => init,
            None => {
                let init = Arc::new(InitSection::new(&init_key, mp4.as_ref())?);
                INIT_SECTIONS.put(init_key, init.clone());
                init
            },
        };

        Ok(Mp4Stream{ path, init, mp4, pos: 0 })
    }

    // Read data and advance file position.
    pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.read_at(self.pos, buf)?;
        self.pos += n as u64;
        Ok(n)
    }

    // Read data at a certain offset.
    pub fn read_at(&mut self, pos: u64, buf: &mut [u8]) -> io::Result<usize> {
        unimplemented!()
    }
}

// Mapping of the samples in the virtual metadata to the samples
// in the original metadata.
struct MdatMapping {
    map:    Vec<u8>,
    offset: u64,
    size:   u64,
}

impl MdatMapping {
    fn new(offset: u64, size: u64) -> MdatMapping {
        MdatMapping {
            map:    Vec::new(),
            offset,
            size,
        }
    }

    fn push_offset(&mut self, offset: u64, name: &str) {
        let hi = offset >> 32;
        if hi > 0xff {
            panic!("MdatMapping::push: {} {} too large (>2^40)", name, offset);
        }
        self.map.push(hi as u8);
        let lo = offset & 0xffffffff;
        self.map.extend_from_slice(&lo.to_ne_bytes());
    }

    fn push(&mut self, mdat_offset: u64, sample_offset: u64, sample_size: u32) {
        self.push_offset(mdat_offset, "mdat_offset");
        self.push_offset(sample_offset, "sample_offset");
        // XXX TODO: optimization: size is mdat_offset[index + 1] - mdat_offset[index]
        self.map.extend_from_slice(&sample_size.to_ne_bytes());
    }

    fn get(&self, index: usize) -> (u64, u64, u32) {
        let offset = index * 14;
        let data = &self.map[offset .. offset + 14];
        let hi = data[0] as u64;
        let mdat_offset = u32::from_ne_bytes(data[1 .. 5].try_into().unwrap()) as u64 | hi;
        let hi = data[5] as u64;
        let sample_offset = u32::from_ne_bytes(data[6 .. 10].try_into().unwrap()) as u64 | hi;
        let sample_size = u32::from_ne_bytes(data[10 .. 14].try_into().unwrap());
        (mdat_offset, sample_offset, sample_size)
    }
}

// Per track rewritten boxes.
#[derive(Default)]
struct InitChunk {
    stsc:   SampleToChunkBox,
    stco:   ChunkOffsetBox,
}

// The InitSection is an MP4 file without the MediaData boxes,
// with only a selected set of tracks, and rewritten
// SampleToChunk boxes and ChunkOffset bxoes.
struct InitSection {
    init:   MP4,
    size:   u32,
}

impl InitSection {
    fn new(key: &SectionKey, mp4: &MP4) -> io::Result<InitSection> {
        unimplemented!()
    }

    fn interleave(mp4: &MP4, tracks: &[TrackBox]) -> (Vec<InitChunk>, MdatMapping) {

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

        let (offset, len) = match first_box!(mp4, MediaDataBox) {
            Some(mdat) => (mdat.data.offset(), mdat.data.len()),
            None => (0, 0),
        };

        let mut mapping = MdatMapping::new(offset, len);
        let mut offset = 0_u64;
        let mut until = 0.5_f64;
        let mut duration = 0.5_f64;
        let mut done = false;

        while !done {
            done = true;

            // Now for each track, add 500ms of samples.
            for t in 0 .. tracks.len() {

                let mut num_samples = 0u32;
                let mut size = 0u32;
                until += duration;

                while let Some(info) = sample_info[t].next() {

                    let decode_time = info.decode_time as f64 / timescale[t];
                    if decode_time >= until {
                        // "un-next" this entry.
                        sample_info[t].push(info);
                        break;
                    }

                    num_samples += 1;
                    size += info.size;

                    // Mapping
                    mapping.push(offset + size as u64, info.fpos, info.size);
                }

                if num_samples > 0 {

                    // add chunk offset entry.
                    chunks[t].stco.push(offset);

                    // and a sample to chunk entry.
                    let chunkno = chunks[t].stco.entries.len() as u32;
                    chunks[t].stsc.entries.push(SampleToChunkEntry {
                        first_chunk: chunkno,
                        samples_per_chunk: num_samples,
                        // FIXME; sample_description_index is hardcoded.
                        sample_description_index: 1,
                    });

                    offset += size as u64;
                    done = false;
                }
            }
        }

        (chunks, mapping)
    }
}

// The DataSection is a mapping from file offsets in the virtual, on-the-fly
// generated MediaDataBox to a sample number. Using the original sampletables
// in the source MP4 file we can then map that to one or more ranges in the
// source MediaDataBox.
struct DataSection {
    // Size of the InitSection.
    init_size:  u64,
    // Mapping. First vec is track, second is (offset, sample).
    mapping: MdatMapping,
}

impl DataSection {
}







struct LruCacheEntry<T> {
    item:   T,
    last_used:  Instant,
}

struct LruCache<K, V> {
    cache:  Mutex<lru::LruCache<K, LruCacheEntry<V>>>,
    max_unused: Duration,
}

impl<K, V> LruCache<K, V>
where
    K: Hash + Eq,
    V: Clone,
{
    fn new(max_unused: Duration) -> LruCache<K, V> {
        LruCache{
            cache: Mutex::new(lru::LruCache::unbounded()),
            max_unused,
        }
    }

    fn put(&self, item_key: K, item_value: V)
    where
        K: Hash + Eq + Clone,
    {
        let mut cache = self.cache.lock().unwrap();
        cache.put(item_key, LruCacheEntry{
            item:   item_value,
            last_used: Instant::now(),
        });
    }

    fn get<Q: ?Sized>(&self, item_key: &Q) -> Option<V>
    where
        lru::KeyRef<K>: Borrow<Q>,
        Q: Hash + Eq,
    {
        let mut cache = self.cache.lock().unwrap();
        cache.get_mut(item_key).map(|e| {
            let v = e.item.clone();
            e.last_used = Instant::now();
            v
        })
    }

    fn expire(&self) {
        let mut cache = self.cache.lock().unwrap();
        let now = Instant::now();
        while let Some((_, peek)) = cache.peek_lru() {
            if now.duration_since(peek.last_used) >= self.max_unused {
                cache.pop_lru();
            }
        }
    }
}
