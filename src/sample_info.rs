//! Iterate over all samples in a track.
use crate::boxes::*;

use crate::boxes::stsz::SampleSizeIterator;
use crate::boxes::stts::TimeToSampleIterator;
use crate::boxes::stsc::SampleToChunkIterator;
use crate::boxes::ctts::CompositionOffsetIterator;
use crate::boxes::stss::SyncSampleIterator;

/// Information about one sample.
#[derive(Default, Debug)]
pub struct SampleInfo {
    /// File position.
    pub fpos:    u64,
    /// Size.
    pub size:   u32,
    /// Decode time.
    pub dtime:  u64,
    /// Composition time delta.
    pub ctime_d:  i32,
    /// is it a sync sample
    pub is_sync:    bool,
    /// what chunkno is it in.
    pub chunkno:  u32,
}

/// Iterator that yields SampleInfo.
pub struct SampleInfoIterator<'a> {
    stsz_iter:  SampleSizeIterator<'a>,
    stts_iter:  TimeToSampleIterator<'a>,
    stsc_iter:  SampleToChunkIterator<'a>,
    ctts_iter:  Option<CompositionOffsetIterator<'a>>,
    stss_iter:  Option<SyncSampleIterator<'a>>,
    index:          u32,
    media_timescale:  u32,
    comp_time_shift: i32,
    chunk_offset:   &'a ChunkOffsetBox,
    fpos:           u64,
    this_chunk:     u32,
    dtime:          u64,
    is_sync:        bool,
    next_sync:      u32,
}

impl SampleInfoIterator<'_> {
    /// Timescale of the track being iterated over.
    pub fn timescale(&self) -> u32 {
        self.media_timescale
    }
}

/// Return an iterator over the SampleTableBox of this track.
///
/// It iterates over multiple tables within the SampleTableBox, and
/// for each sample returns a SampleInfo.
pub fn sample_info_iter<'a>(trak: &'a TrackBox) -> SampleInfoIterator<'a> {

    let mdhd = trak.media().media_header();
    let stbl = trak.media().media_info().sample_table();
    let media_timescale = mdhd.timescale;

    let comp_time_shift = trak.composition_time_shift().unwrap_or(0) as i32;

    let mut stss_iter = stbl.sync_samples().map(|stss| stss.iter());
    let next_sync = stss_iter.as_mut().and_then(|iter| iter.next()).unwrap_or(0xffffffff);

    SampleInfoIterator {
        stsz_iter: stbl.sample_size().iter(),
        stts_iter: stbl.time_to_sample().iter(),
        stsc_iter: stbl.sample_to_chunk().iter(),
        ctts_iter: stbl.composition_time_to_sample().map(|ctts| ctts.iter()),
        stss_iter,
        index: 0,
        media_timescale,
        comp_time_shift,
        chunk_offset: stbl.chunk_offset(),
        fpos: 0,
        this_chunk: 0xffffffff,
        dtime: 0,
        is_sync: stbl.sync_samples().is_none(),
        next_sync,
    }
}

impl<'a> Iterator for SampleInfoIterator<'a> {
    type Item = SampleInfo;

    fn next(&mut self) -> Option<Self::Item> {
        let size = match self.stsz_iter.next() {
            Some(size) => size,
            None => return None,
        };

        if let Some(chunk) = self.stsc_iter.next() {
            if self.this_chunk != chunk.chunk {
                self.this_chunk = chunk.chunk;
                // XXX FIXME check chunk.chunk for index overflow
                self.fpos = self.chunk_offset.entries[self.this_chunk as usize];
            }
        }

        let mut sample = SampleInfo {
            fpos: self.fpos,
            size,
            chunkno: self.this_chunk,
            is_sync: self.is_sync,
            ..SampleInfo::default()
        };
        self.fpos += size as u64;

        if let Some(time) = self.stts_iter.next() {
            sample.dtime = self.dtime;
            self.dtime += time as u64;
        }
        if let Some(ctts_iter) = self.ctts_iter.as_mut() {
            if let Some(delta) = ctts_iter.next() {
                sample.ctime_d = delta - self.comp_time_shift;
            }
        }
    
        if let Some(stss_iter) = self.stss_iter.as_mut() {
            if self.index == self.next_sync {
                sample.is_sync = true;
                self.next_sync = stss_iter.next().unwrap_or(0xffffffff);
            }
        }
        self.index += 1;

        Some(sample)
    }
}
