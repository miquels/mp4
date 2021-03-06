//! Iterate over all samples in a track.
use std::io;

use crate::boxes::*;

use crate::boxes::ctts::CompositionOffsetIterator;
use crate::boxes::stsc::SampleToChunkIterator;
use crate::boxes::stss::SyncSampleIterator;
use crate::boxes::stsz::SampleSizeIterator;
use crate::boxes::stts::TimeToSampleIterator;

/// Information about one sample.
#[derive(Default, Debug, Clone)]
pub struct SampleInfo {
    /// File position.
    pub fpos:              u64,
    /// Size.
    pub size:              u32,
    /// Duration.
    pub duration:          u32,
    /// Decode time.
    pub decode_time:       u64,
    /// Composition time delta.
    pub composition_delta: i32,
    /// is it a sync sample
    pub is_sync:           bool,
    /// what chunk is it in.
    pub chunk:             u32,
}

/// Iterator that yields SampleInfo.
#[derive(Clone)]
pub struct SampleInfoIterator<'a> {
    stsz_iter:        SampleSizeIterator<'a>,
    stts_iter:        TimeToSampleIterator<'a>,
    stsc_iter:        SampleToChunkIterator<'a>,
    ctts_iter:        Option<CompositionOffsetIterator<'a>>,
    stss_iter:        Option<SyncSampleIterator<'a>>,
    chunk_offset:     &'a ChunkOffsetBox,
    media_timescale:  u32,
    comp_time_shift_: i32,
    comp_time_shift:  i32,
    fpos:             u64,
    cur_sample:       u32,
    cur_chunk:        u32,
    pending:          Option<SampleInfo>,
}

impl SampleInfoIterator<'_> {
    /// Timescale of the media being iterated over.
    pub fn timescale(&self) -> u32 {
        self.media_timescale
    }

    /// For the composition_delta field, also take any edit lists for the track in account.
    pub fn with_edit_list(&mut self, use_elst: bool) {
        if use_elst {
            self.comp_time_shift = self.comp_time_shift_;
        } else {
            self.comp_time_shift = 0;
        }
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

    let comp_time_shift_ = trak.composition_time_shift().unwrap_or(0) as i32;

    SampleInfoIterator {
        stsz_iter: stbl.sample_size().iter(),
        stts_iter: stbl.time_to_sample().iter(),
        stsc_iter: stbl.sample_to_chunk().iter(),
        ctts_iter: stbl.composition_time_to_sample().map(|ctts| ctts.iter()),
        stss_iter: stbl.sync_samples().map(|stss| stss.iter()),
        chunk_offset: stbl.chunk_offset_table(),
        media_timescale,
        comp_time_shift_,
        comp_time_shift: 0,
        fpos: 0,
        cur_sample: 1,
        cur_chunk: 1,
        pending: None,
    }
}

impl<'a> SampleInfoIterator<'a> {
    pub fn seek(&mut self, to_sample: u32) -> io::Result<()> {
        self.stsz_iter.seek(to_sample)?;
        self.stts_iter.seek(to_sample)?;
        self.stsc_iter.seek(to_sample)?;
        if let Some(ctts) = self.ctts_iter.as_mut() {
            ctts.seek(to_sample)?;
        }
        if let Some(stss) = self.stss_iter.as_mut() {
            stss.seek(to_sample)?;
        }

        self.cur_sample = to_sample;

        // peek at chunk info.
        let chunk_info = self.stsc_iter.clone().next().unwrap();
        self.cur_chunk = chunk_info.cur_chunk;

        // calculate file position.
        let idx = self.cur_chunk.saturating_sub(1) as usize;
        self.fpos = self.chunk_offset.entries.get(idx);
        if to_sample > 1 {
            self.fpos += self.stsz_iter.add_sizes(chunk_info.first_sample, to_sample - 1);
        }

        self.pending.take();

        Ok(())
    }

    pub fn push(&mut self, info: SampleInfo) {
        self.pending.replace(info);
    }
}

impl<'a> Iterator for SampleInfoIterator<'a> {
    type Item = SampleInfo;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(pending) = self.pending.take() {
            return Some(pending);
        }

        let size = match self.stsz_iter.next() {
            Some(size) => size,
            None => return None,
        };

        if let Some(chunk_info) = self.stsc_iter.next() {
            if self.cur_sample == chunk_info.first_sample {
                self.cur_chunk = chunk_info.cur_chunk;
                let idx = self.cur_chunk.saturating_sub(1) as usize;
                self.fpos = self.chunk_offset.entries.get(idx);
            }
        }

        let mut sample = SampleInfo {
            fpos: self.fpos,
            size,
            chunk: self.cur_chunk,
            is_sync: true,
            ..SampleInfo::default()
        };
        self.fpos += size as u64;

        if let Some((duration, decode_time)) = self.stts_iter.next() {
            sample.duration = duration;
            sample.decode_time = decode_time;
        }

        if let Some(ctts_iter) = self.ctts_iter.as_mut() {
            if let Some(delta) = ctts_iter.next() {
                sample.composition_delta = delta - self.comp_time_shift;
            }
        }

        if let Some(stss_iter) = self.stss_iter.as_mut() {
            sample.is_sync = stss_iter.next().unwrap();
        }
        self.cur_sample += 1;

        Some(sample)
    }
}
