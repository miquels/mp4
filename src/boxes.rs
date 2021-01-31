use std::fmt::Debug;
use std::io;

pub(crate) mod misc;
pub (crate) mod prelude;

pub use self::misc::*;
use self::prelude::*;

use crate::mp4box::{BoxHeader, GenericBox};

def_boxes! {
    BaseMediaInformationHeaderBox, b"gmhd";
    CleanApertureBox, b"clap";
    CompositionOffsetBox, b"ctts";
    DataEntryUrlBox, b"url ";
    DataEntryUrnBox, b"urn ";
    DataInformationBox, b"dinf";
    DataReferenceBox, b"dref";
    EditBox, b"edts";
    EditListBox, b"elst";
    ExtendedLanguageBox, b"elng";
    FileTypeBox, b"ftyp";
    HandlerBox, b"hdlr";
    InitialObjectDescriptionBox, b"iods";
    MediaHeaderBox, b"mdhd";
    MetaBox, b"meta";
    MovieExtendsBox, b"mvex";
    MovieExtendsHeaderBox, b"mehd";
    MovieFragmentBox, b"moof";
    MovieFragmentHeaderBox, b"mfhd";
    MovieHeaderBox, b"mvhd";
    NameBox, b"name";
    NullMediaHeaderBox, b"nmhd";
    PixelAspectRatioBox, b"pasp";
    SampleToChunkBox, b"stsc";
    SegmentTypeBox, b"styp";
    SoundMediaHeaderBox, b"smhd";
    SubtitleMediaHeaderBox, b"sthd";
    SyncSampleBox, b"stss";
    TimeToSampleBox, b"stts";
    TrackExtendsBox, b"trex";
    TrackFragmentBaseMediaDecodeTimeBox, b"tfdt";
    TrackFragmentBox, b"traf";
    TrackHeaderBox, b"tkhd";
    TrackSelectionBox, b"tsel";
    UserDataBox, b"udta";
    VideoMediaInformationBox, b"vmhd";

    // Below are boxes that are defined manually in boxes/ *.rs
    AvcSampleEntry, b"avc1" => avc1;
    AvcConfigurationBox, b"avcC";

    AacSampleEntry, b"mp4a" => mp4a;
    ESDescriptorBox, b"esds";

    Ac3SampleEntry, b"ac-3" => ac_3;
    AC3SpecificBox, b"dac3";

    AppleItemListBox, b"ilst" => ilst;

    ChunkOffsetBox, b"stco" => stco;
    ChunkLargeOffsetBox, b"co64";

    MediaBox, b"mdia" => mdia;
    MediaDataBox, b"mdat" => mdat;
    MediaInformationBox, b"minf" => minf;
    MovieBox, b"moov" => moov;

    Free, b"free" => free;
    Skip, b"skip";
    Wide, b"wide";

    SampleDescriptionBox, b"stsd" => stsd;
    SampleGroupDescriptionBox, b"sgpd" => sgpd;
    SampleSizeBox, b"stsz" => stsz;
    CompactSampleSizeBox, b"stz2" => stz2;
    SampleTableBox, b"stbl" => stbl;
    SampleToGroupBox, b"sbgp" => sbgp;
    SegmentIndexBox, b"sidx" => sidx;
    TrackBox, b"trak" => trak;
    TrackFragmentHeaderBox, b"tfhd" => tfhd;
    TrackRunBox, b"trun" => trun;

    TextSubtitleSampleEntry, b"sbtt" => sbtl;
    Tx3gSubtitleSampleEntry, b"tx3g";
    XMLSubtitleSampleEntry, b"stpp";
    FontTableBox, b"ftab";
}
