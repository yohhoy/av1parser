//
// https://aomedia.org/av1-bitstream-and-decoding-process-specification/
//
use bitio::BitReader;
use std::fmt;
use std::io;

pub const OBU_SEQUENCE_HEADER: u8 = 1;
pub const OBU_TEMPORAL_DELIMITER: u8 = 2;
pub const OBU_FRAME_HEADER: u8 = 3;
pub const OBU_TILE_GROUP: u8 = 4;
pub const OBU_METADATA: u8 = 5;
pub const OBU_FRAME: u8 = 6;
pub const OBU_REDUNDANT_FRAME_HEADER: u8 = 7;
pub const OBU_TILE_LIST: u8 = 8;
pub const OBU_PADDING: u8 = 15;

const SELECT_SCREEN_CONTENT_TOOLS: u8 = 2;
const SELECT_INTEGER_MV: u8 = 2;

// Color primaries
const CP_BT_709: u8 = 1; // BT.709
const CP_UNSPECIFIED: u8 = 2; // Unspecified

// Transfer characteristics
const TC_UNSPECIFIED: u8 = 2; // Unspecified
const TC_SRGB: u8 = 13; // sRGB or sYCC

// Matrix coefacients
const MC_IDENTITY: u8 = 0; // Identity matrix
const MC_UNSPECIFIED: u8 = 2; // Unspecified

///
/// OBU(Open Bitstream Unit)
///
#[derive(Debug)]
pub struct Obu {
    // obu_header()
    pub obu_type: u8,             // f(4)
    pub obu_extension_flag: bool, // f(1)
    pub obu_has_size_field: bool, // f(1)
    // obu_extension_header()
    pub temporal_id: u8, // f(3)
    pub spatial_id: u8,  // f(2)
    // open_bitstream_unit()
    pub obu_size: u32, // leb128()
    pub header_len: u32,
}

impl fmt::Display for Obu {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let obu_type = match self.obu_type {
            OBU_SEQUENCE_HEADER => "SEQUENCE_HEADER".to_owned(),
            OBU_TEMPORAL_DELIMITER => "TEMPORAL_DELIMITER".to_owned(),
            OBU_FRAME_HEADER => "FRAME_HEADER".to_owned(),
            OBU_TILE_GROUP => "TILE_GROUP".to_owned(),
            OBU_FRAME => "FRAME".to_owned(),
            OBU_METADATA => "METADATA".to_owned(),
            OBU_REDUNDANT_FRAME_HEADER => "REDUNDANT_FRAME_HEADER".to_owned(),
            OBU_TILE_LIST => "TILE_LIST".to_owned(),
            OBU_PADDING => "PADDING".to_owned(),
            _ => format!("Reserved({})", self.obu_type), // Reserved
        };
        if self.obu_extension_flag {
            write!(
                f,
                "{} T{}S{} size={}+{}",
                obu_type, self.temporal_id, self.spatial_id, self.header_len, self.obu_size
            )
        } else {
            //  Base layer (temporal_id == 0 && spatial_id == 0)
            write!(f, "{} size={}+{}", obu_type, self.header_len, self.obu_size)
        }
    }
}

///
/// color_config()
///
#[derive(Debug, Default)]
pub struct ColorConfig {
    pub bit_depth: u8,
    pub mono_chrome: bool,                    // f(1)
    pub color_description_present_flag: bool, // f(1)
    pub color_primaries: u8,                  // f(8)
    pub transfer_characteristics: u8,         // f(8)
    pub matrix_coefficients: u8,              // f(8)
    pub color_range: bool,                    // f(1)
    pub chroma_sample_position: u8,           // f(2)
    pub separate_uv_delta_q: bool,            // f(1)
}

#[derive(Debug, Default)]
pub struct OperatingPoint {
    pub operating_point_idc: u16, // f(12)
    pub seq_level_idx: u8,        // f(5)
    pub seq_tier: u8,             // f(1)
}

///
/// Sequence header OBU
///
#[derive(Debug, Default)]
pub struct SequenceHeader {
    pub seq_profile: u8,                          // f(3)
    pub still_picture: bool,                      // f(1)
    pub reduced_still_picture_header: bool,       // f(1)
    pub timing_info_present_flag: bool,           // f(1)
    pub decoder_model_info_present_flag: bool,    // f(1)
    pub initial_display_delay_present_flag: bool, // f(1)
    pub operating_points_cnt_minus_1: u8,         // f(5)
    pub op: [OperatingPoint; 1],                  // OperatingPoint
    pub max_frame_width: u32,                     // f(n)
    pub max_frame_height: u32,                    // f(n)
    pub frame_id_numbers_present_flag: bool,      // f(1)
    pub delta_frame_id_length: u8,                // f(4)
    pub additional_frame_id_length: u8,           // f(3)
    pub use_128x128_superblock: bool,             // f(1)
    pub enable_filter_intra: bool,                // f(1)
    pub enable_intra_edge_filter: bool,           // f(1)
    pub enable_interintra_compound: bool,         // f(1)
    pub enable_masked_compound: bool,             // f(1)
    pub enable_warped_motion: bool,               // f(1)
    pub enable_dual_filter: bool,                 // f(1)
    pub enable_order_hint: bool,                  // f(1)
    pub enable_jnt_comp: bool,                    // f(1)
    pub enable_ref_frame_mvs: bool,               // f(1)
    pub seq_choose_screen_content_tools: bool,    // f(1)
    pub seq_force_screen_content_tools: u8,       // f(1)
    pub seq_choose_integer_mv: u8,                // f(1)
    pub seq_force_integer_mv: u8,                 // f(1)
    pub order_hint_bits: u8,                      // f(3)
    pub enable_superres: bool,                    // f(1)
    pub enable_cdef: bool,                        // f(1)
    pub enable_restoration: bool,                 // f(1)
    pub color_config: ColorConfig,                // color_config()
    pub film_grain_params_present: bool,          // f(1)
}

///
/// return (Leb128Bytes, leb128())
///
fn leb128<R: io::Read>(bs: &mut R) -> io::Result<(u32, u32)> {
    let mut value: u64 = 0;
    let mut leb128bytes = 0;
    for i in 0..8 {
        let mut leb128_byte = [0; 1];
        bs.read_exact(&mut leb128_byte)?; // f(8)
        let leb128_byte = leb128_byte[0];
        value |= ((leb128_byte & 0x7f) as u64) << (i * 7);
        leb128bytes += 1;
        if (leb128_byte & 0x80) != 0x80 {
            break;
        }
    }
    assert!(value <= (1u64 << 32) - 1);
    Ok((leb128bytes, value as u32))
}

///
/// parse color_config()
///
fn parse_color_config<R: io::Read>(
    br: &mut BitReader<R>,
    sh: &SequenceHeader,
) -> Option<ColorConfig> {
    let mut cc = ColorConfig::default();

    let high_bitdepth = br.f(1)? == 1; // f(1)
    if sh.seq_profile == 2 && high_bitdepth {
        let twelve_bit = br.f(1)? == 1; // f(1)
        cc.bit_depth = if twelve_bit { 12 } else { 10 }
    } else if sh.seq_profile <= 2 {
        cc.bit_depth = if high_bitdepth { 10 } else { 8 }
    }
    if sh.seq_profile == 1 {
        cc.mono_chrome = false;
    } else {
        cc.mono_chrome = br.f(1)? == 1; // f(1)
    }
    cc.color_description_present_flag = br.f(1)? == 1; // f(1)
    if cc.color_description_present_flag {
        cc.color_primaries = br.f(8)? as u8; // f(8)
        cc.transfer_characteristics = br.f(8)? as u8; // f(8)
        cc.matrix_coefficients = br.f(8)? as u8; // f(8)
    } else {
        cc.color_primaries = CP_UNSPECIFIED;
        cc.transfer_characteristics = TC_UNSPECIFIED;
        cc.matrix_coefficients = MC_UNSPECIFIED;
    }
    if cc.mono_chrome {
        cc.color_range = br.f(1)? == 1; // f(1)
        cc.separate_uv_delta_q = false;
        return Some(cc);
    } else if cc.color_primaries == CP_BT_709
        && cc.transfer_characteristics == TC_SRGB
        && cc.matrix_coefficients == MC_IDENTITY
    {
        cc.color_range = true;
        return Some(cc);
    } else {
        let (subsampling_x, subsampling_y);
        cc.color_range = br.f(1)? == 1; // f(1)
        if sh.seq_profile == 0 {
            subsampling_x = 1;
            subsampling_y = 1;
        } else if sh.seq_profile == 1 {
            subsampling_x = 0;
            subsampling_y = 0;
        } else {
            if cc.bit_depth == 12 {
                unimplemented!("BitDepth==12");
            } else {
                subsampling_x = 1;
                subsampling_y = 0;
            }
        }
        if subsampling_x != 0 && subsampling_y != 0 {
            cc.chroma_sample_position = br.f(2)? as u8; // f(2)
        }
    }
    cc.separate_uv_delta_q = br.f(1)? == 1; // f(1)

    Some(cc)
}

///
/// parse AV1 OBU header
///
pub fn parse_obu_header<R: io::Read>(bs: &mut R, sz: u32) -> io::Result<Obu> {
    // parse obu_header()
    let mut b1 = [0; 1];
    bs.read_exact(&mut b1)?;
    let obu_forbidden_bit = (b1[0] >> 7) & 1; // f(1)
    assert_eq!(obu_forbidden_bit, 0);
    let obu_type = (b1[0] >> 3) & 0b1111; // f(4)
    let obu_extension_flag = (b1[0] >> 2) & 1; // f(1)
    let obu_has_size_field = (b1[0] >> 1) & 1; // f(1)
    let (temporal_id, spatial_id) = if obu_extension_flag == 1 {
        // parse obu_extension_header()
        let mut b2 = [0; 1];
        bs.read_exact(&mut b2)?;
        ((b2[0] >> 5) & 0b111, (b2[0] >> 3) & 0b11) // f(3),f(2)
    } else {
        (0, 0)
    };
    // parse open_bitstream_unit()
    let (obu_size_len, obu_size) = if obu_has_size_field == 1 {
        leb128(bs)?
    } else {
        (0, sz - 1 - (obu_extension_flag as u32))
    };

    return Ok(Obu {
        obu_type: obu_type,
        obu_extension_flag: obu_extension_flag == 1,
        obu_has_size_field: obu_has_size_field == 1,
        temporal_id: temporal_id,
        spatial_id: spatial_id,
        obu_size: obu_size,
        header_len: 1 + (obu_extension_flag as u32) + obu_size_len,
    });
}

///
/// parse sequence_header_obu()
///
pub fn parse_sequence_header<R: io::Read>(bs: &mut R, sz: u32) -> Option<SequenceHeader> {
    let mut br = BitReader::new(bs, sz);
    let mut sh = SequenceHeader::default();

    sh.seq_profile = br.f(3)? as u8; // f(3)
    sh.still_picture = br.f(1)? == 1; // f(1)
    sh.reduced_still_picture_header = br.f(1)? == 1; // f(1)
    if sh.reduced_still_picture_header {
        unimplemented!("reduced_still_picture_header==1");
    } else {
        sh.timing_info_present_flag = br.f(1)? == 1; // f(1)
        if sh.timing_info_present_flag {
            unimplemented!("timing_info_present_flag==1");
        } else {
            sh.decoder_model_info_present_flag = false;
        }
        sh.initial_display_delay_present_flag = br.f(1)? == 1; // f(1)
        sh.operating_points_cnt_minus_1 = br.f(5)? as u8; // f(5)
        assert_eq!(sh.operating_points_cnt_minus_1, 0);
        for i in 0..=sh.operating_points_cnt_minus_1 as usize {
            sh.op[i].operating_point_idc = br.f(12)? as u16; // f(12)
            sh.op[i].seq_level_idx = br.f(5)? as u8; // f(5)
            if sh.op[i].seq_level_idx > 7 {
                sh.op[i].seq_tier = br.f(1)? as u8; // f(1)
            } else {
                sh.op[i].seq_tier = 0;
            }
            if sh.decoder_model_info_present_flag {
                unimplemented!("decoder_model_info_present_flag==1");
            }
            if sh.initial_display_delay_present_flag {
                unimplemented!("initial_display_delay_present_flag==1");
            }
        }
    }
    let frame_width_bits_minus_1 = br.f(4)? as usize; // f(4)
    let frame_height_bits_minus_1 = br.f(4)? as usize; // f(4)
    sh.max_frame_width = br.f(frame_width_bits_minus_1 + 1)? + 1; // f(n)
    sh.max_frame_height = br.f(frame_height_bits_minus_1 + 1)? + 1; // f(n)
    if sh.reduced_still_picture_header {
        sh.frame_id_numbers_present_flag = false;
    } else {
        sh.frame_id_numbers_present_flag = br.f(1)? == 1; // f(1)
    }
    if sh.frame_id_numbers_present_flag {
        sh.delta_frame_id_length = br.f(4)? as u8 + 2; // f(4)
        sh.additional_frame_id_length = br.f(3)? as u8 + 1; // f(3)
    }
    sh.use_128x128_superblock = br.f(1)? == 1; // f(1)
    sh.enable_filter_intra = br.f(1)? == 1; // f(1)
    sh.enable_intra_edge_filter = br.f(1)? == 1; // f(1)
    if sh.reduced_still_picture_header {
        unimplemented!("reduced_still_picture_header==1");
    } else {
        sh.enable_interintra_compound = br.f(1)? == 1; // f(1)
        sh.enable_masked_compound = br.f(1)? == 1; // f(1)
        sh.enable_warped_motion = br.f(1)? == 1; // f(1)
        sh.enable_dual_filter = br.f(1)? == 1; // f(1)
        sh.enable_order_hint = br.f(1)? == 1; // f(1)
        if sh.enable_order_hint {
            sh.enable_jnt_comp = br.f(1)? == 1; // f(1)
            sh.enable_ref_frame_mvs = br.f(1)? == 1; // f(1)
        } else {
            sh.enable_jnt_comp = false;
            sh.enable_ref_frame_mvs = false;
        }
        sh.seq_choose_screen_content_tools = br.f(1)? == 1; // f(1)
        if sh.seq_choose_screen_content_tools {
            sh.seq_force_screen_content_tools = SELECT_SCREEN_CONTENT_TOOLS;
        } else {
            sh.seq_force_screen_content_tools = br.f(1)? as u8; // f(1)
        }
        if sh.seq_force_screen_content_tools > 0 {
            sh.seq_choose_integer_mv = br.f(1)? as u8; // f(1)
            if sh.seq_choose_integer_mv > 0 {
                sh.seq_force_integer_mv = SELECT_INTEGER_MV;
            } else {
                sh.seq_force_integer_mv = br.f(1)? as u8; // f(1)
            }
        } else {
            sh.seq_force_integer_mv = SELECT_INTEGER_MV;
        }
        if sh.enable_order_hint {
            sh.order_hint_bits = br.f(3)? as u8 + 1; // f(3)
        } else {
            sh.order_hint_bits = 0;
        }
    }
    sh.enable_superres = br.f(1)? == 1; // f(1)
    sh.enable_cdef = br.f(1)? == 1; // f(1)
    sh.enable_restoration = br.f(1)? == 1; // f(1)
    sh.color_config = parse_color_config(&mut br, &sh)?; // color_config()
    sh.film_grain_params_present = br.f(1)? == 1; // f(1)

    Some(sh)
}
