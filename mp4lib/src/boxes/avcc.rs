//! Avc1 decoder configuration.

use std::io;

use crate::boxes::prelude::*;
use crate::bitreader::BitReader;

def_box! {
    /// AvcConfigurationBox (ISO/IEC 14496-15)
    AvcConfigurationBox {
        configuration: AvcDecoderConfigurationRecord,
    },
    fourcc => "avcC",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

// aligned(8) class AVCDecoderConfigurationRecord {
//     unsigned int(8) configurationVersion = 1;
//     unsigned int(8) AVCProfileIndication;
//     unsigned int(8) profile_compatibility;
//     unsigned int(8) AVCLevelIndication;
//     bit(6) reserved = ‘111111’b;
//     unsigned int(2) lengthSizeMinusOne;
//     bit(3) reserved = ‘111’b;
//     unsigned int(5) numOfSequenceParameterSets;
//     for (i=0; i< numOfSequenceParameterSets; i++) {
//       unsigned int(16) sequenceParameterSetLength ;
//       bit(8*sequenceParameterSetLength) sequenceParameterSetNALUnit;
//     }
//     unsigned int(8) numOfPictureParameterSets;
//     for (i=0; i< numOfPictureParameterSets; i++) {
//       unsigned int(16) pictureParameterSetLength;
//       bit(8*pictureParameterSetLength) pictureParameterSetNALUnit;
//     }
// }
def_struct! {
    /// AVC Decoder Configuration Record.
    AvcDecoderConfigurationRecord,
        configuration_version:  u8,
        profile_indication:     u8,
        profile_compatibility:  u8,
        level_indication:       u8,
        data:                   Data,
}

impl AvcDecoderConfigurationRecord {
    /// Return human name of codec, like "Baseline" or "High".
    pub fn codec_name(&self) -> &'static str {
        match self.profile_indication {
            0x2c => "AVC CAVLC 4:4:4",
            0x42 => "AVC Baseline",
            0x4d => "AVC Main",
            0x58 => "AVC Extended",
            0x64 => "AVC High",
            0x6e => "AVC High 10",
            0x7a => "AVC High 4:2:2",
            0xf4 => "AVC High 4:4:4",

            0x53 => "AVC Scalable Baseline",
            0x56 => "AVC Scalable High",

            0x76 => "AVC Multiview High",
            0x80 => "AVC Stereo High",
            0x8a => "AVC Multiview Depth High",
            _ => "AVC",
        }
    }

    /// Return codec id as avc1.4d401f
    pub fn codec_id(&self) -> String {
        format!("avc1.{:02x}{:02x}{:02x}",
                    self.profile_indication, self.profile_compatibility, self.level_indication)
    }

    /// Decode the frame rate.
    pub fn frame_rate(&self) -> io::Result<Option<f64>> {
        let parameter_sets = ParameterSet::parse(&self.data.0)?;
        let seq_parameter_sets = parameter_sets.sequence_parameters_sets()?;
        for sps in &seq_parameter_sets {
            if let Some(t_inf) = sps.vui_parameters.as_ref().and_then(|v| v.timing_info.as_ref()) {
                let fr = t_inf.frame_rate();
                if fr > 120.0 {
                    log::warn!("AvcDecoderConfigurationRecord::frame_rate: impossible rate {}, ignoring", fr);
                    return Ok(None);
                }
                return Ok(Some(fr));
            }
        }
        Ok(None)
    }
}

/// delegated to AvcDecoderConfigurationRecord::codec_id().
impl std::fmt::Display for AvcDecoderConfigurationRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.codec_id())
    }
}

// All of the code below was written based on several open source h.264 parsers
// that I found,such as:
//
// - https://android.googlesource.com/platform/external/mp4parser/+/master/isoparser/src/main/java/com/googlecode/mp4parser/h264/model/
// - https://github.com/dholroyd/h264-reader
//
// All of that code looks alike, so they where either all written off the
// standard, or everyone is copying one another.


// The avcc 'data' member consists of an array of SequenceParamerteSets,
// followed by an array of PictureParameterSets.
//
// We only decode the SequenceParamerteSets, for now.
//
#[derive(Debug, Default)]
pub(crate) struct ParameterSet<'a> {
    sps: Vec<&'a [u8]>,
    _pps: Vec<&'a [u8]>,
    _length_size_minus_one: u8,
}

impl<'a> ParameterSet<'a> {
    pub(crate) fn parse(data: &'a [u8]) -> io::Result<ParameterSet<'a>> {

        if data.len() < 2 {
            return Err(ioerr!(UnexpectedEof, "ParameterSet::parse: EOF (1)"));
        }

        let nalu_length_size_minus_one = data[0] & 0b00000011;
        let number_of_sps_nalus = data[1] & 0b00011111;
        let mut idx: usize = 2;

        let mut sps_items = Vec::new();
        for _ in 0 .. number_of_sps_nalus {
            if idx + 2 >= data.len() {
                return Err(ioerr!(UnexpectedEof, "ParameterSet::parse: EOF (2)"));
            }
            let sps_length = u16::from_be_bytes([data[idx], data[idx + 1]]) as usize;
            idx += 2;
            if idx + sps_length > data.len() {
                return Err(ioerr!(UnexpectedEof, "ParameterSet::parse: EOF (3)"));
            }
            let sps = &data[idx .. idx + sps_length];
            sps_items.push(sps);
            idx += sps_length;
        }

        if idx >= data.len() {
            return Err(ioerr!(UnexpectedEof, "ParameterSet::parse: EOF (4)"));
        }
        let number_of_pps_nalus = data[idx];
        idx += 1;

        let mut pps_items = Vec::new();
        for _ in 0 .. number_of_pps_nalus {
            if idx + 2 >= data.len() {
                return Err(ioerr!(UnexpectedEof, "ParameterSet::parse: EOF (5)"));
            }
            let pps_length = u16::from_be_bytes([data[idx], data[idx + 1]]) as usize;
            idx += 2;
            if idx + pps_length > data.len() {
                return Err(ioerr!(UnexpectedEof, "ParameterSet::parse: EOF (6)"));
            }
            let pps = &data[idx .. idx + pps_length];
            pps_items.push(pps);
            idx += pps_length;
        }

        Ok(ParameterSet {
            sps: sps_items,
            _pps: pps_items,
            _length_size_minus_one: nalu_length_size_minus_one,
        })
    }

    // Decode the SequenceParametersSets.
    pub(crate) fn sequence_parameters_sets(&self) -> io::Result<Vec<SeqParameterSet>> {
        let mut v = Vec::new();
        for sps in &self.sps {
            if sps.len() < 4 {
                continue;
            }
            let mut idx = 0;
            let nal_unit_type = sps[0] & 0x1f;
            if nal_unit_type != 7 {
                // Not a SeqParameterSet NAL.
                continue;
            }
            idx += 1;
            // FIXME: unescape, so 00 00 03 01 -> 00 00 01
            let mut reader = BitReader::new(&sps[idx..]);
            let parsed = SeqParameterSet::read(&mut reader)?;
            v.push(parsed);
        }
        Ok(v)
    }
}

// Helper.
fn cond<F, T, E>(pred: bool, mut f: F) -> Result<Option<T>, E>
where
    F: FnMut() -> Result<T, E>
{
    if pred {
        Some(f()).transpose()
    } else {
        Ok(None)
    }
}

/// Sequence Parameter Set.
#[derive(Clone, Debug)]
pub struct SeqParameterSet {
    pub profile_idc: u8,
    pub constraint_flags: u8,
    pub level_idc: u8,
    pub seq_parameter_set_id: u8,
    pub chroma_format: Option<ChromaFormat>,
    pub log2_max_frame_num_minus4: u8,
    pub pic_order_cnt_type: PicOrderCntType,
    pub num_ref_frames: u32,
    pub gaps_in_frame_num_value_allowed_flag: bool,
    pub pic_width_in_mbs_minus1: u32,
    pub pic_height_in_map_units_minus1: u32,
    pub frame_mbs_flags: FrameMbsFlags,
    pub direct_8x8_inference_flag: bool,
    pub frame_cropping: Option<FrameCroppingFlags>,
    pub vui_parameters: Option<VuiParameters>,
}

impl SeqParameterSet {
    fn read(reader: &mut BitReader) -> io::Result<SeqParameterSet> {

        let profile_idc = reader.read_u8()?;
        Ok(SeqParameterSet {
            profile_idc,
            constraint_flags: reader.read_u8()?,
            level_idc: reader.read_u8()?,
            seq_parameter_set_id: reader.read_ue_max(31)? as u8,
            chroma_format: ChromaFormat::read(reader, profile_idc)?,
            log2_max_frame_num_minus4: reader.read_ue_max(255)? as u8,
            pic_order_cnt_type: PicOrderCntType::read(reader)?,
            num_ref_frames: reader.read_ue()?,
            gaps_in_frame_num_value_allowed_flag: reader.read_bit()?,
            pic_width_in_mbs_minus1: reader.read_ue()?,
            pic_height_in_map_units_minus1: reader.read_ue()?,
            frame_mbs_flags: FrameMbsFlags::read(reader)?,
            direct_8x8_inference_flag: reader.read_bit()?,
            frame_cropping: cond(reader.read_bit()?, || FrameCroppingFlags::read(reader))?,
            vui_parameters: cond(reader.read_bit()?, || VuiParameters::read(reader))?,
        })
    }
}

/// Picture Order Count Type.
#[derive(Clone, Debug)]
pub enum PicOrderCntType {
    Zero {
        log2_max_pic_order_cnt_lsb_minus4: u8,
    },
    One {
        delta_pic_order_always_zero_flag: bool,
        offset_for_non_ref_pic: i32,
        offset_for_top_to_bottom_field: i32,
        offset_for_ref_frame: Vec<i32>,
    },
    Two,
}

impl PicOrderCntType {
    fn read(reader: &mut BitReader) -> io::Result<PicOrderCntType> {
        let pic_order_cnt_type = reader.read_ue()?;
        match pic_order_cnt_type {
            0 => {
                Ok(PicOrderCntType::Zero {
                    log2_max_pic_order_cnt_lsb_minus4: reader.read_ue_max(12)? as u8,
                })
            },
            1 => {
                let delta_pic_order_always_zero_flag = reader.read_bit()?;
                let offset_for_non_ref_pic = reader.read_se()?;
                let offset_for_top_to_bottom_field = reader.read_se()?;
                let num_ref_frames_in_pic_order_cnt_cycle = reader.read_ue()?;
                let mut offset_for_ref_frame = Vec::new();
                for _ in 0 .. num_ref_frames_in_pic_order_cnt_cycle {
                    offset_for_ref_frame.push(reader.read_se()?);
                }
                Ok(PicOrderCntType::One {
                    delta_pic_order_always_zero_flag,
                    offset_for_non_ref_pic,
                    offset_for_top_to_bottom_field,
                    offset_for_ref_frame,
                })
            },
            2 => Ok(PicOrderCntType::Two),
            other => Err(ioerr!(InvalidData, "unknown pic_order_cnt_type: {}", other)),
        }
    }
}

/// Frame Cropping Flags.
#[derive(Clone, Debug)]
pub struct FrameCroppingFlags {
    pub frame_crop_left_offset: u32,
    pub frame_crop_right_offset: u32,
    pub frame_crop_top_offset: u32,
    pub frame_crop_bottom_offset: u32,
}

impl FrameCroppingFlags {
    fn read(reader: &mut BitReader) -> io::Result<FrameCroppingFlags> {
        Ok(FrameCroppingFlags {
            frame_crop_left_offset: reader.read_ue()?,
            frame_crop_right_offset: reader.read_ue()?,
            frame_crop_top_offset: reader.read_ue()?,
            frame_crop_bottom_offset: reader.read_ue()?,
        })
    }
}

/// Frame Mbs Flags.
#[derive(Debug, Clone)]
pub enum FrameMbsFlags {
    Frames,
    Fields {
        mb_adaptive_frame_field_flag: bool,
    }
}

impl FrameMbsFlags {
    fn read(r: &mut BitReader) -> io::Result<FrameMbsFlags> {
        let frame_mbs_only_flag = r.read_bit()?;
        if frame_mbs_only_flag {
            Ok(FrameMbsFlags::Frames)
        } else {
            Ok(FrameMbsFlags::Fields {
                mb_adaptive_frame_field_flag: r.read_bit()?
            })
        }
    }
}

/// Chroma format information.
#[derive(Clone, Debug)]
pub struct ChromaFormat {
    pub chroma_format_idc: u32,
    pub residual_color_transform_flag: Option<bool>,
    pub bit_depth_luma_minus8: u32,
    pub bit_depth_chroma_minus8: u32,
    pub qpprime_y_zero_transform_bypass_flag: bool,
    pub scaling_matrix: Option<ScalingMatrix>,
}

impl ChromaFormat {
    fn read(reader: &mut BitReader, profile_indication: u8) -> io::Result<Option<ChromaFormat>> {
        match profile_indication {
            100|110|122|144 => {},
            _ => return Ok(None),
        }

        let chroma_format_idc = reader.read_ue()?;
        Ok(Some(ChromaFormat {
            chroma_format_idc,
            residual_color_transform_flag: cond(chroma_format_idc == 3, || reader.read_bit())?,
            bit_depth_luma_minus8: reader.read_ue()?,
            bit_depth_chroma_minus8: reader.read_ue()?,
            qpprime_y_zero_transform_bypass_flag: reader.read_bit()?,
            scaling_matrix: cond(reader.read_bit()?, || ScalingMatrix::read(reader, chroma_format_idc))?,
        }))
    }
}


/// Scaling Matrix.
#[derive(Clone, Debug)]
pub struct ScalingMatrix {
    pub scaling_list_4x4: Vec<ScalingList>,
    pub scaling_list_8x8: Vec<ScalingList>,
}

impl ScalingMatrix {
    fn read(reader: &mut BitReader, chroma_format_idc: u32) -> io::Result<ScalingMatrix> {
        let mut scaling_list_4x4 = Vec::new();
        let mut scaling_list_8x8 = Vec::new();

        let size = if chroma_format_idc == 3 { 12 } else { 8 };
        for i in 0 .. size {
            let seq_scaling_list_present_flag = reader.read_bit()?;
            if seq_scaling_list_present_flag {
                if i < 6 {
                    scaling_list_4x4.push(ScalingList::read(reader, 16)?);
                } else {
                    scaling_list_8x8.push(ScalingList::read(reader, 64)?);
                }
            }
        }

        Ok(ScalingMatrix {
            scaling_list_4x4,
            scaling_list_8x8,
        })
    }
}

/// Scaling List.
///
/// Part of Scaling Matrix.
#[derive(Clone, Debug)]
pub struct ScalingList {
    pub use_default_scaling_matrix_flag: bool,
    pub scaling_list: Vec<u32>,
}

impl ScalingList {
    fn read(reader: &mut BitReader, size_of_list: usize) -> io::Result<ScalingList> {
        let mut last_scale = 8_u32;
        let mut next_scale = 8_u32;
        let mut use_default_scaling_matrix_flag = false;
        let mut scaling_list = Vec::new();

        for j in 0 .. size_of_list {
            if next_scale != 0 {
                let delta_scale = reader.read_se()?;
                next_scale = (last_scale as i32 + delta_scale + 256) as u32 % 256;
                use_default_scaling_matrix_flag = j == 0 && next_scale == 0;
            }
            let val = if next_scale == 0 { last_scale } else { next_scale };
            scaling_list.push(val);
            last_scale = val;
        }
        Ok(ScalingList {
            use_default_scaling_matrix_flag,
            scaling_list,
        })
    }
}

/// Vui Parameters.
#[derive(Clone, Debug)]
pub struct VuiParameters {
    pub aspect_ratio_info: Option<AspectRatioInfo>,
    pub overscan_appropriate: Option<bool>,
    pub video_signal_type: Option<VideoSignalType>,
    pub chroma_loc_info: Option<ChromaLocInfo>,
    pub timing_info: Option<TimingInfo>,
    //pub nal_hrd_parameters: Option<NalHrdParameters>,
    //pub vcl_hrd_parameters: Option<VclHrdParameters>,
    //pub bitstream_restriction: Option<BitstreamRestriction>,
}

impl VuiParameters {
    fn read(reader: &mut BitReader) -> io::Result<VuiParameters> {
        Ok(VuiParameters {
            aspect_ratio_info: cond(reader.read_bit()?, || AspectRatioInfo::read(reader))?,
            overscan_appropriate: cond(reader.read_bit()?, || reader.read_bit())?,
            video_signal_type: cond(reader.read_bit()?, || VideoSignalType::read(reader))?,
            chroma_loc_info: cond(reader.read_bit()?, || ChromaLocInfo::read(reader))?,
            timing_info: cond(reader.read_bit()?, || TimingInfo::read(reader))?,
            //nal_hrd_parameters: cond(reader.read_bit()?, || NalHrdParameters::read(reader))?,
            //vcl_hrd_parameters: cond(reader.read_bit()?, || VclHdrParameters::read(reader))?,
            //bitstream_restriction: cond(reader.read_bit()?, || BitstreamRestriction::read(reader))?,
        })
    }
}

/// Aspect Ration Info.
#[derive(Clone, Debug)]
pub struct AspectRatioInfo {
    pub aspect_ratio: u8,
    pub extended_sar: Option<(u16, u16)>,
}

impl AspectRatioInfo {
    fn read(reader: &mut BitReader) -> io::Result<AspectRatioInfo> {
        let aspect_ratio = reader.read_u8()?;
        let extended_sar = if aspect_ratio == 255 {
            let sar_width = reader.read_bits(16)? as u16;
            let sar_height = reader.read_bits(16)? as u16;
            Some((sar_width, sar_height))
        } else {
            None
        };
        Ok(AspectRatioInfo{ aspect_ratio, extended_sar })
    }
}

/// Video Signal Type.
#[derive(Clone, Debug)]
pub struct VideoSignalType {
    pub video_format: u8,
    pub video_full_range_flag: bool,
    pub colour_description: Option<ColourDescription>,
}

impl VideoSignalType {
    fn read(reader: &mut BitReader) -> io::Result<VideoSignalType> {
        Ok(VideoSignalType {
            video_format: reader.read_bits(3)? as u8,
            video_full_range_flag: reader.read_bit()?,
            colour_description: cond(reader.read_bit()?, || ColourDescription::read(reader))?,
        })
    }
}

/// Colour Description.
#[derive(Clone, Debug)]
pub struct ColourDescription {
    pub colour_primaries: u8,
    pub transfer_characteristics: u8,
    pub matrix_coefficients: u8,
}

impl ColourDescription {
    fn read(reader: &mut BitReader) -> io::Result<ColourDescription> {
        Ok(ColourDescription {
            colour_primaries: reader.read_u8()?,
            transfer_characteristics: reader.read_u8()?,
            matrix_coefficients: reader.read_u8()?,
        })
    }
}

/// Chroma Loc Information.
#[derive(Clone, Debug)]
pub struct ChromaLocInfo {
    pub chroma_sample_loc_type_top_field: u32,
    pub chroma_sample_loc_type_bottom_field: u32,
}

impl ChromaLocInfo {
    fn read(reader: &mut BitReader) -> io::Result<ChromaLocInfo> {
        Ok(ChromaLocInfo {
            chroma_sample_loc_type_top_field: reader.read_ue()?,
            chroma_sample_loc_type_bottom_field: reader.read_ue()?,
        })
    }
}

/// Timing Information.
#[derive(Clone, Debug)]
pub struct TimingInfo {
    pub num_units_in_tick: u32,
    pub time_scale: u32,
    pub fixed_frame_rate_flag: bool,
}

impl TimingInfo {
    fn read(reader: &mut BitReader) -> io::Result<TimingInfo> {
        Ok(TimingInfo {
            num_units_in_tick: reader.read_bits(32)?,
            time_scale: reader.read_bits(32)?,
            fixed_frame_rate_flag: reader.read_bit()?,
        })
    }

    /// Framerate in frames / sec.
    pub fn frame_rate(&self) -> f64 {
        log::trace!("avcc: TimingInfo: {:?}", self);
        (self.time_scale as f64 / (self.num_units_in_tick as f64)) / 2.0
    }
}
