//! All the boxes we known.
//!
//! This module does not only contain boxes, but also the types
//! that are used in the boxes, and helper types like iterators.
//!
use std::fmt::Debug;
use std::io;

pub(crate) mod misc;
pub(crate) mod prelude;

pub use self::misc::*;
use self::prelude::*;

use crate::mp4box::{BoxHeader, GenericBox};

def_boxes! {
    BaseMediaInformationHeaderBox, b"gmhd";
    CleanApertureBox, b"clap";
    ExtendedLanguageBox, b"elng";
    FileTypeBox, b"ftyp";
    InitialObjectDescriptionBox, b"iods";
    MediaHeaderBox, b"mdhd";
    MetaBox, b"meta";
    MovieExtendsBox, b"mvex";
    MovieExtendsHeaderBox, b"mehd";
    MovieFragmentHeaderBox, b"mfhd";
    MovieHeaderBox, b"mvhd";
    NameBox, b"name";
    NullMediaHeaderBox, b"nmhd";
    PixelAspectRatioBox, b"pasp";
    SegmentTypeBox, b"styp";
    SoundMediaHeaderBox, b"smhd";
    SubtitleMediaHeaderBox, b"sthd";
    TrackExtendsBox, b"trex";
    TrackFragmentBaseMediaDecodeTimeBox, b"tfdt";
    TrackSelectionBox, b"tsel";
    UserDataBox, b"udta";

    // Below are boxes that are defined manually in boxes/ *.rs
    AvcSampleEntry, b"avc1" => avc1;
    AvcConfigurationBox, b"avcC" => avcc;

    AacSampleEntry, b"mp4a" => mp4a;
    ESDescriptorBox, b"esds";

    Ac3SampleEntry, b"ac-3" => ac_3;
    AC3SpecificBox, b"dac3";

    AppleItemListBox, b"ilst" => ilst;

    ChunkOffsetBox, b"stco" => stco;
    ChunkLargeOffsetBox, b"co64";
    CompositionOffsetBox, b"ctts" => ctts;

    DataInformationBox, b"dinf" => dinf;
    DataEntryUrlBox, b"url ";
    DataEntryUrnBox, b"urn ";
    DataReferenceBox, b"dref";

    EditBox, b"edts" => edts;
    EditListBox, b"elst";

    HandlerBox, b"hdlr" => hdlr;
    MediaBox, b"mdia" => mdia;
    MediaDataBox, b"mdat" => mdat;
    MediaInformationBox, b"minf" => minf;
    MovieBox, b"moov" => moov;
    MovieFragmentBox, b"moof" => moof;

    Free, b"free" => free;
    Skip, b"skip";
    Wide, b"wide";

    ProgressiveDownloadInfoBox, b"pdin" => pdin;

    SampleDescriptionBox, b"stsd" => stsd;
    SampleGroupDescriptionBox, b"sgpd" => sgpd;
    SampleSizeBox, b"stsz" => stsz;
    CompactSampleSizeBox, b"stz2" => stz2;
    SampleTableBox, b"stbl" => stbl;
    SampleToChunkBox, b"stsc" => stsc;
    SampleToGroupBox, b"sbgp" => sbgp;
    SegmentIndexBox, b"sidx" => sidx;
    SyncSampleBox, b"stss" => stss;
    TrackBox, b"trak" => trak;
    TrackHeaderBox, b"tkhd" => tkhd;
    TrackFragmentBox, b"traf" => traf;
    TrackFragmentHeaderBox, b"tfhd" => tfhd;
    TrackRunBox, b"trun" => trun;
    TimeToSampleBox, b"stts" => stts;

    Tx3gTextSampleEntry, b"tx3g" => sbtl;
    Tx3gFontTableBox, b"ftab";
    Tx3gTextStyleBox, b"styl";
    Tx3gTextHighlightBox, b"hlit";
    Tx3gTextHighlightColorBox, b"hclr";
    Tx3gTextKaraokeBox, b"krok";
    Tx3gTextScrollDelayBox, b"dlay";
    Tx3gTextHyperTextBox, b"href";
    TextSubtitleSampleEntry, b"sbtt";
    XMLSubtitleSampleEntry, b"stpp";

    VideoMediaHeaderBox, b"vmhd" => vmhd;
}
