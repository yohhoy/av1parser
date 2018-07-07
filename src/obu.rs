//
// https://aomedia.org/av1-bitstream-and-decoding-process-specification/
//
#![allow(dead_code)]
use av1;
use bitio::BitReader;
use std::cmp;
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

use av1::LAST_FRAME;

const REFS_PER_FRAME: usize = 3; // Number of reference frames that can be used for inter prediction
const MAX_TILE_WIDTH: u32 = 4096; // Maximum width of a tile in units of luma samples
const MAX_TILE_AREA: u32 = 4096 * 2304; // Maximum area of a tile in units of luma samples
const MAX_TILE_ROWS: u32 = 64; // Maximum number of tile rows
const MAX_TILE_COLS: u32 = 64; // Maximum number of tile columns
pub const NUM_REF_FRAMES: usize = 8; // Number of frames that can be stored for future reference
const SELECT_SCREEN_CONTENT_TOOLS: u8 = 2; // Value that indicates the allow_screen_content_tools syntax element is coded
const SELECT_INTEGER_MV: u8 = 2; // Value that indicates the force_integer_mv syntax element is coded
const PRIMARY_REF_NONE: u8 = 7; // Value of primary_ref_frame indicating that there is no primary reference frame
const SUPERRES_NUM: usize = 8; // Numerator for upscaling ratio
const SUPERRES_DENOM_MIN: usize = 9; // Smallest denominator for upscaling ratio
const SUPERRS_DENOM_BITS: usize = 3; // Number of bits sent to specify denominator of upscaling ratio

// Color primaries
const CP_BT_709: u8 = 1; // BT.709
const CP_UNSPECIFIED: u8 = 2; // Unspecified

// Transfer characteristics
const TC_UNSPECIFIED: u8 = 2; // Unspecified
const TC_SRGB: u8 = 13; // sRGB or sYCC

// Matrix coefacients
const MC_IDENTITY: u8 = 0; // Identity matrix
const MC_UNSPECIFIED: u8 = 2; // Unspecified

// Frame type
const KEY_FRAME: u8 = 0;
const INTER_FRAME: u8 = 1;
const INTRA_ONLY_FRAME: u8 = 2;
const SWITCH_FRAME: u8 = 3;

// interpolation_filter
const SWITCHABLE: u8 = 4;

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
            // Base layer (temporal_id == 0 && spatial_id == 0)
            write!(f, "{} size={}+{}", obu_type, self.header_len, self.obu_size)
        }
    }
}

// Color config
#[derive(Debug, Default)]
pub struct ColorConfig {
    pub bit_depth: u8, // BitDepth
    // color_config()
    pub mono_chrome: bool,                    // f(1)
    pub color_description_present_flag: bool, // f(1)
    pub color_primaries: u8,                  // f(8)
    pub transfer_characteristics: u8,         // f(8)
    pub matrix_coefficients: u8,              // f(8)
    pub color_range: bool,                    // f(1)
    pub chroma_sample_position: u8,           // f(2)
    pub separate_uv_delta_q: bool,            // f(1)
}

/// Timing info
#[derive(Debug, Default)]
pub struct TimingInfo {
    // timing_info()
    pub num_units_in_display_tick: u32, // f(32)
    pub time_scale: u32,                // f(32)
    pub equal_picture_interval: bool,   // f(1)
    pub num_ticks_per_picture: u32,     // uvlc()
}

///
/// operating point in Sequence Header OBU
///
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
    pub timing_info: TimingInfo,                  // timing_info()
    pub decoder_model_info_present_flag: bool,    // f(1)
    pub initial_display_delay_present_flag: bool, // f(1)
    pub operating_points_cnt_minus_1: u8,         // f(5)
    pub op: [OperatingPoint; 1],                  // OperatingPoint
    pub frame_width_bits: u8,                     // f(4)
    pub frame_height_bits: u8,                    // f(4)
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

/// Frame size
#[derive(Debug, Default)]
pub struct FrameSize {
    // frame_size()
    pub frame_width: u32,                // FrameWidth
    pub frame_height: u32,               // FrameHeight
    pub superres_params: SuperresParams, // superres_params()
}

/// Render size
#[derive(Debug, Default)]
pub struct RenderSize {
    // render_size()
    pub render_and_frame_size_different: bool, // f(1)
    pub render_width: u32,                     // RenderWidth
    pub render_height: u32,                    // RenderHeight
}

/// Superres params
#[derive(Debug, Default)]
pub struct SuperresParams {
    // superres_params()
    pub use_superres: bool,  // f(1)
    pub upscaled_width: u32, // UpscaledWidth
}

/// Interpolation filter
#[derive(Debug, Default)]
pub struct InterpolationFilter {
    // read_interpolation_filter()
    pub is_filter_switchable: bool, // f(1)
    pub interpolation_filter: u8,   // f(2)
}

/// Tile info
#[derive(Debug, Default)]
pub struct TileInfo {
    pub tile_cols: u16, // TileCols
    pub tile_rows: u16, // TileRows
    // tile_info()
    pub uniform_tile_spacing_flag: bool, // f(1)
    pub context_update_tile_id: u32,     // f(TileRowsLog2+TileColsLog2)
    pub tile_size_bytes: usize,          // TileSizeBytes
}

///
/// Frame header OBU
///
#[derive(Debug, Default)]
pub struct FrameHeader {
    // uncompressed_header()
    pub show_existing_frame: bool,                 // f(1)
    pub frame_to_show_map_idx: u8,                 // f(3)
    pub display_frame_id: u16,                     // f(idLen)
    pub frame_type: u8,                            // f(2)
    pub show_frame: bool,                          // f(1)
    pub showable_frame: bool,                      // f(1)
    pub error_resilient_mode: bool,                // f(1)
    pub disable_cdf_update: bool,                  // f(1)
    pub allow_screen_content_tools: bool,          // f(1)
    pub force_integer_mv: bool,                    // f(1)
    pub current_frame_id: u16,                     // f(idLen)
    pub frame_size_override_flag: bool,            // f(1)
    pub order_hint: u8,                            // f(OrderHintBits)
    pub primary_ref_frame: u8,                     // f(3)
    pub refresh_frame_flags: u8,                   // f(8)
    pub ref_order_hint: [u8; NUM_REF_FRAMES],      // f(OrderHintBits)
    pub frame_size: FrameSize,                     // frame_size()
    pub render_size: RenderSize,                   // render_size()
    pub allow_intrabc: bool,                       // f(1)
    pub frame_refs_short_signaling: bool,          // f(1)
    pub last_frame_idx: u8,                        // f(3)
    pub gold_frame_idx: u8,                        // f(3)
    pub ref_frame_idx: [u8; NUM_REF_FRAMES],       // f(3)
    pub allow_high_precision_mv: bool,             // f(1)
    pub interpolation_filter: InterpolationFilter, // interpolation_filter()
    pub is_motion_mode_switchable: bool,           // f(1)
    pub use_ref_frame_mvs: bool,                   // f(1)
    pub disable_frame_end_update_cdf: bool,        // f(1)
    pub order_hints: [u8; NUM_REF_FRAMES],         // OrderHints
    pub tile_info: TileInfo,                       // tile_info()
    pub allow_warped_motion: bool,                 // f(1)
    pub reduced_tx_set: bool,                      // f(1)
}

/// return (MiCols, MiRows)
fn compute_image_size(fs: &FrameSize) -> (u32, u32) {
    (
        2 * ((fs.frame_width + 7) >> 3),  // MiCol
        2 * ((fs.frame_height + 7) >> 3), // MiRows
    )
}

/// return (Leb128Bytes, leb128())
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
/// parse trailing_bits()
///
fn trailing_bits<R: io::Read>(br: &mut BitReader<R>) -> Option<()> {
    let trailing_one_bit = br.f::<u8>(1)?;
    if trailing_one_bit != 1 {
        return None;
    }
    while let Some(trailing_zero_bit) = br.f::<u8>(1) {
        if trailing_zero_bit != 0 {
            return None;
        }
    }
    Some(())
}

///
/// parse color_config()
///
fn parse_color_config<R: io::Read>(
    br: &mut BitReader<R>,
    sh: &SequenceHeader,
) -> Option<ColorConfig> {
    let mut cc = ColorConfig::default();

    let high_bitdepth = br.f::<bool>(1)?; // f(1)
    if sh.seq_profile == 2 && high_bitdepth {
        let twelve_bit = br.f::<bool>(1)?; // f(1)
        cc.bit_depth = if twelve_bit { 12 } else { 10 }
    } else if sh.seq_profile <= 2 {
        cc.bit_depth = if high_bitdepth { 10 } else { 8 }
    }
    if sh.seq_profile == 1 {
        cc.mono_chrome = false;
    } else {
        cc.mono_chrome = br.f::<bool>(1)?; // f(1)
    }
    cc.color_description_present_flag = br.f::<bool>(1)?; // f(1)
    if cc.color_description_present_flag {
        cc.color_primaries = br.f::<u8>(8)?; // f(8)
        cc.transfer_characteristics = br.f::<u8>(8)?; // f(8)
        cc.matrix_coefficients = br.f::<u8>(8)?; // f(8)
    } else {
        cc.color_primaries = CP_UNSPECIFIED;
        cc.transfer_characteristics = TC_UNSPECIFIED;
        cc.matrix_coefficients = MC_UNSPECIFIED;
    }
    if cc.mono_chrome {
        cc.color_range = br.f::<bool>(1)?; // f(1)
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
        cc.color_range = br.f::<bool>(1)?; // f(1)
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
            cc.chroma_sample_position = br.f::<u8>(2)?; // f(2)
        }
    }
    cc.separate_uv_delta_q = br.f::<bool>(1)?; // f(1)

    Some(cc)
}

///
/// parse frame_size()
///
fn parse_frame_size<R: io::Read>(
    br: &mut BitReader<R>,
    sh: &SequenceHeader,
    fh: &FrameHeader,
) -> Option<FrameSize> {
    let mut fs = FrameSize::default();

    if fh.frame_size_override_flag {
        fs.frame_width = br.f::<u32>(sh.frame_width_bits as usize)? + 1; // f(n)
        fs.frame_height = br.f::<u32>(sh.frame_height_bits as usize)? + 1; // f(n)
    } else {
        fs.frame_width = sh.max_frame_width;
        fs.frame_height = sh.max_frame_height;
    }
    fs.superres_params = parse_superres_params(br, &sh, &mut fs)?; // superres_params()
                                                                   // compute_image_size()

    Some(fs)
}

///
/// parse render_size()
///
fn parse_render_size<R: io::Read>(br: &mut BitReader<R>, fs: &FrameSize) -> Option<RenderSize> {
    let mut rs = RenderSize::default();

    rs.render_and_frame_size_different = br.f::<bool>(1)?; // f(1)
    if rs.render_and_frame_size_different {
        rs.render_width = br.f::<u32>(16)? + 1; // f(16)
        rs.render_height = br.f::<u32>(16)? + 1; // f(16)
    } else {
        rs.render_width = fs.superres_params.upscaled_width;
        rs.render_height = fs.frame_height;
    }

    Some(rs)
}

///
/// parse interpolation_filter()
///
fn parse_interpolation_filter<R: io::Read>(br: &mut BitReader<R>) -> Option<InterpolationFilter> {
    let mut ifp = InterpolationFilter::default();

    ifp.is_filter_switchable = br.f::<bool>(1)?; // f(1)
    if ifp.is_filter_switchable {
        ifp.interpolation_filter = SWITCHABLE;
    } else {
        ifp.interpolation_filter = br.f::<u8>(2)?; // f(2)
    }

    Some(ifp)
}

///
/// parse superres_params()
///
fn parse_superres_params<R: io::Read>(
    br: &mut BitReader<R>,
    sh: &SequenceHeader,
    fs: &mut FrameSize,
) -> Option<SuperresParams> {
    let mut sp = SuperresParams::default();

    if sh.enable_superres {
        sp.use_superres = br.f::<bool>(1)?; // f(1)
    } else {
        sp.use_superres = false;
    }
    let supreres_denom;
    if sp.use_superres {
        let coded_denom = br.f::<usize>(SUPERRS_DENOM_BITS)?; // f(SUPERRES_DENOM_BITS)
        supreres_denom = coded_denom + SUPERRES_DENOM_MIN;
    } else {
        supreres_denom = SUPERRES_NUM;
    }
    sp.upscaled_width = fs.frame_width;
    fs.frame_width = ((sp.upscaled_width as usize * SUPERRES_NUM + (supreres_denom / 2))
        / supreres_denom) as u32;

    Some(sp)
}

///
/// parse tile_info()
///
fn parse_tile_info<R: io::Read>(
    br: &mut BitReader<R>,
    sh: &SequenceHeader,
    fs: &FrameSize,
) -> Option<TileInfo> {
    let mut ti = TileInfo::default();

    // tile_log2: Tile size calculation function
    let tile_log2 = |blk_size, target| {
        let mut k = 0;
        while (blk_size << k) < target {
            k += 1;
        }
        k
    };

    let (mi_cols, mi_rows) = compute_image_size(fs);
    let sb_cols = if sh.use_128x128_superblock {
        (mi_cols + 31) >> 5
    } else {
        (mi_cols + 15) >> 4
    };
    let sb_rows = if sh.use_128x128_superblock {
        (mi_rows + 31) >> 5
    } else {
        (mi_rows + 15) >> 4
    };
    let sb_shift = if sh.use_128x128_superblock { 5 } else { 4 };
    let sb_size = sb_shift + 2;
    let max_tile_width_sb = MAX_TILE_WIDTH >> sb_size;
    let max_tile_area_sb = MAX_TILE_AREA >> (2 * sb_size);
    let min_log2_tile_cols = tile_log2(max_tile_width_sb, sb_cols);
    let max_log2_tile_cols = tile_log2(1, cmp::min(sb_cols, MAX_TILE_COLS));
    let max_log2_tile_rows = tile_log2(1, cmp::min(sb_rows, MAX_TILE_ROWS));
    let min_log2_tiles = cmp::max(
        min_log2_tile_cols,
        tile_log2(max_tile_area_sb, sb_rows * sb_cols),
    );

    ti.uniform_tile_spacing_flag = br.f::<bool>(1)?; // f(1)
    let (mut tile_cols_log2, mut tile_rows_log2): (usize, usize);
    if ti.uniform_tile_spacing_flag {
        tile_cols_log2 = min_log2_tile_cols;
        while tile_cols_log2 < max_log2_tile_cols {
            let increment_tile_cols_log2 = br.f::<bool>(1)?; // f(1)
            if increment_tile_cols_log2 {
                tile_cols_log2 += 1;
            } else {
                break;
            }
        }
        let tile_width_sb = (sb_cols + (1 << tile_cols_log2) - 1) >> tile_cols_log2;
        let (mut i, mut start_sb) = (0, 0);
        while start_sb < sb_cols {
            // MiColStarts[i] = startSb << sbShift
            i += 1;
            start_sb += tile_width_sb;
        }
        // MiColStarts[i] = MiCols
        ti.tile_cols = i;

        let min_log2_tile_rows = cmp::max(min_log2_tiles - tile_cols_log2, 0);
        tile_rows_log2 = min_log2_tile_rows;
        while tile_rows_log2 < max_log2_tile_rows {
            let increment_tile_rows_log2 = br.f::<bool>(1)?; // f(1)
            if increment_tile_rows_log2 {
                tile_rows_log2 += 1;
            } else {
                break;
            }
        }
        let tile_height_sb = (sb_rows + (1 << tile_rows_log2) - 1) >> tile_rows_log2;
        let (mut i, mut start_sb) = (0, 0);
        while start_sb < sb_rows {
            // MiRowStarts[i] = startSb << sbShift
            i += 1;
            start_sb += tile_height_sb;
        }
        // MiRowStarts[i] = MiRows
        ti.tile_rows = i;
    } else {
        let mut widest_tile_sb = 0;
        let (mut i, mut start_sb) = (0, 0);
        while start_sb < sb_cols {
            // MiColStarts[i] = startSb << sbShift
            let max_width = cmp::min(sb_cols - start_sb, max_tile_width_sb);
            let size_sb = br.ns(max_width)? + 1; // ns(maxWidth)
            widest_tile_sb = cmp::max(size_sb, widest_tile_sb);
            start_sb += size_sb;
            i += 1;
        }
        // MiColStarts[i] = MiCols
        ti.tile_cols = i;
        tile_cols_log2 = tile_log2(1, ti.tile_cols as u32);

        let max_tile_area_sb = if min_log2_tiles > 0 {
            (sb_rows * sb_cols) >> (min_log2_tiles + 1)
        } else {
            sb_rows * sb_cols
        };
        let max_tile_height_sb = cmp::max(max_tile_area_sb / widest_tile_sb, 1);
        let (mut start_sb, mut i) = (0, 0);
        while start_sb < sb_rows {
            // MiRowStarts[i] = startSb << sbShift
            let max_height = cmp::min(sb_rows - start_sb, max_tile_height_sb);
            let size_sb = br.ns(max_height)? + 1; // ns(maxHeight)
            start_sb += size_sb;
            i += 1;
        }
        // MiRowStarts[i] = MiRows
        ti.tile_rows = i;
        tile_rows_log2 = tile_log2(1, ti.tile_rows as u32);
    }
    if tile_cols_log2 > 0 && tile_rows_log2 > 0 {
        ti.context_update_tile_id = br.f::<u32>(tile_cols_log2 + tile_rows_log2)?; // f(TileRowsLog2+TileColsLog2)
        ti.tile_size_bytes = br.f::<usize>(2)? + 1; // f(2)
    } else {
        ti.context_update_tile_id = 0;
    }

    Some(ti)
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
    // parse 'obu_size' in open_bitstream_unit()
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

    sh.seq_profile = br.f::<u8>(3)?; // f(3)
    sh.still_picture = br.f::<bool>(1)?; // f(1)
    sh.reduced_still_picture_header = br.f::<bool>(1)?; // f(1)
    if sh.reduced_still_picture_header {
        unimplemented!("reduced_still_picture_header==1");
    } else {
        sh.timing_info_present_flag = br.f::<bool>(1)?; // f(1)
        if sh.timing_info_present_flag {
            unimplemented!("timing_info_present_flag==1");
        } else {
            sh.decoder_model_info_present_flag = false;
        }
        sh.initial_display_delay_present_flag = br.f::<bool>(1)?; // f(1)
        sh.operating_points_cnt_minus_1 = br.f::<u8>(5)?; // f(5)
        assert_eq!(sh.operating_points_cnt_minus_1, 0);
        for i in 0..=sh.operating_points_cnt_minus_1 as usize {
            sh.op[i].operating_point_idc = br.f::<u16>(12)?; // f(12)
            sh.op[i].seq_level_idx = br.f::<u8>(5)?; // f(5)
            if sh.op[i].seq_level_idx > 7 {
                sh.op[i].seq_tier = br.f::<u8>(1)?; // f(1)
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
    sh.frame_width_bits = br.f::<u8>(4)? + 1; // f(4)
    sh.frame_height_bits = br.f::<u8>(4)? + 1; // f(4)
    sh.max_frame_width = br.f::<u32>(sh.frame_width_bits as usize)? + 1; // f(n)
    sh.max_frame_height = br.f::<u32>(sh.frame_height_bits as usize)? + 1; // f(n)
    if sh.reduced_still_picture_header {
        sh.frame_id_numbers_present_flag = false;
    } else {
        sh.frame_id_numbers_present_flag = br.f::<bool>(1)?; // f(1)
    }
    if sh.frame_id_numbers_present_flag {
        sh.delta_frame_id_length = br.f::<u8>(4)? + 2; // f(4)
        sh.additional_frame_id_length = br.f::<u8>(3)? + 1; // f(3)
    }
    sh.use_128x128_superblock = br.f::<bool>(1)?; // f(1)
    sh.enable_filter_intra = br.f::<bool>(1)?; // f(1)
    sh.enable_intra_edge_filter = br.f::<bool>(1)?; // f(1)
    if sh.reduced_still_picture_header {
        unimplemented!("reduced_still_picture_header==1");
    } else {
        sh.enable_interintra_compound = br.f::<bool>(1)?; // f(1)
        sh.enable_masked_compound = br.f::<bool>(1)?; // f(1)
        sh.enable_warped_motion = br.f::<bool>(1)?; // f(1)
        sh.enable_dual_filter = br.f::<bool>(1)?; // f(1)
        sh.enable_order_hint = br.f::<bool>(1)?; // f(1)
        if sh.enable_order_hint {
            sh.enable_jnt_comp = br.f::<bool>(1)?; // f(1)
            sh.enable_ref_frame_mvs = br.f::<bool>(1)?; // f(1)
        } else {
            sh.enable_jnt_comp = false;
            sh.enable_ref_frame_mvs = false;
        }
        sh.seq_choose_screen_content_tools = br.f::<bool>(1)?; // f(1)
        if sh.seq_choose_screen_content_tools {
            sh.seq_force_screen_content_tools = SELECT_SCREEN_CONTENT_TOOLS;
        } else {
            sh.seq_force_screen_content_tools = br.f::<u8>(1)?; // f(1)
        }
        if sh.seq_force_screen_content_tools > 0 {
            sh.seq_choose_integer_mv = br.f::<u8>(1)?; // f(1)
            if sh.seq_choose_integer_mv > 0 {
                sh.seq_force_integer_mv = SELECT_INTEGER_MV;
            } else {
                sh.seq_force_integer_mv = br.f::<u8>(1)?; // f(1)
            }
        } else {
            sh.seq_force_integer_mv = SELECT_INTEGER_MV;
        }
        if sh.enable_order_hint {
            sh.order_hint_bits = br.f::<u8>(3)? + 1; // f(3)
        } else {
            sh.order_hint_bits = 0;
        }
    }
    sh.enable_superres = br.f::<bool>(1)?; // f(1)
    sh.enable_cdef = br.f::<bool>(1)?; // f(1)
    sh.enable_restoration = br.f::<bool>(1)?; // f(1)
    sh.color_config = parse_color_config(&mut br, &sh)?; // color_config()
    sh.film_grain_params_present = br.f::<bool>(1)?; // f(1)
    trailing_bits(&mut br)?;

    Some(sh)
}

///
/// parse frame_header
///
pub fn parse_frame_header<R: io::Read>(
    bs: &mut R,
    sz: u32,
    sh: &SequenceHeader,
    rfman: &mut av1::RefFrameManager,
) -> Option<FrameHeader> {
    let mut br = BitReader::new(bs, sz);
    let mut fh = FrameHeader::default();

    // uncompressed_header()
    let id_len = if sh.frame_id_numbers_present_flag {
        sh.additional_frame_id_length + sh.delta_frame_id_length
    } else {
        0
    } as usize;
    assert!(NUM_REF_FRAMES <= 8);
    let all_frames = ((1usize << NUM_REF_FRAMES) - 1) as u8; // 0xff
    let frame_is_intra: bool;
    if sh.reduced_still_picture_header {
        fh.show_existing_frame = false;
        fh.frame_type = KEY_FRAME;
        frame_is_intra = true;
        fh.show_frame = true;
        fh.showable_frame = false;
    } else {
        fh.show_existing_frame = br.f::<bool>(1)?; // f(1)
        if fh.show_existing_frame {
            fh.frame_to_show_map_idx = br.f::<u8>(3)?; // f(3)
            if sh.decoder_model_info_present_flag && !sh.timing_info.equal_picture_interval {
                unimplemented!("temporal_point_info()");
            }
            fh.refresh_frame_flags = 0;
            if sh.frame_id_numbers_present_flag {
                fh.display_frame_id = br.f::<u16>(id_len)?; // f(idLen)
            }
            fh.frame_type = rfman.ref_frame_type[fh.frame_to_show_map_idx as usize];
            if fh.frame_type == KEY_FRAME {
                fh.refresh_frame_flags = all_frames;
            }
            if sh.film_grain_params_present {
                unimplemented!("load_grain_params()");
            }
            return Some(fh);
        }
        fh.frame_type = br.f::<u8>(2)?; // f(2)
        frame_is_intra = fh.frame_type == INTRA_ONLY_FRAME || fh.frame_type == KEY_FRAME;
        fh.show_frame = br.f::<bool>(1)?; // f(1)
        if fh.show_frame
            && sh.decoder_model_info_present_flag
            && !sh.timing_info.equal_picture_interval
        {
            unimplemented!("temporal_point_info()");
        }
        if fh.show_frame {
            fh.showable_frame = fh.frame_type != KEY_FRAME;
        } else {
            fh.showable_frame = br.f::<bool>(1)?; // f(1)
        }
        if fh.frame_type == SWITCH_FRAME || (fh.frame_type == KEY_FRAME && fh.show_frame) {
            fh.error_resilient_mode = true;
        } else {
            fh.error_resilient_mode = br.f::<bool>(1)?; // f(1)
        }
    }
    if fh.frame_type == KEY_FRAME && fh.show_frame {
        for i in 0..NUM_REF_FRAMES {
            rfman.ref_valid[i] = false;
            rfman.ref_order_hint[i] = 0;
        }
        for i in 0..REFS_PER_FRAME {
            fh.order_hints[LAST_FRAME + i] = 0;
        }
    }
    fh.disable_cdf_update = br.f::<bool>(1)?; // f(1)
    if sh.seq_force_screen_content_tools == SELECT_SCREEN_CONTENT_TOOLS {
        fh.allow_screen_content_tools = br.f::<bool>(1)?; // f(1)
    } else {
        fh.allow_screen_content_tools = sh.seq_force_screen_content_tools != 0;
    }
    if fh.allow_screen_content_tools {
        if sh.seq_force_integer_mv == SELECT_INTEGER_MV {
            fh.force_integer_mv = br.f::<bool>(1)?; // f(1)
        } else {
            fh.force_integer_mv = sh.seq_force_integer_mv != 0;
        }
    } else {
        fh.force_integer_mv = false;
    }
    if frame_is_intra {
        fh.force_integer_mv = true;
    }
    if sh.frame_id_numbers_present_flag {
        let _prev_frame_id = fh.current_frame_id;
        fh.current_frame_id = br.f::<u16>(id_len)?; // f(idLen)
        rfman.mark_ref_frames(id_len, sh, &fh);
    } else {
        fh.current_frame_id = 0;
    }
    if fh.frame_type == SWITCH_FRAME {
        fh.frame_size_override_flag = true;
    } else if sh.reduced_still_picture_header {
        fh.frame_size_override_flag = false;
    } else {
        fh.frame_size_override_flag = br.f::<bool>(1)?; // f(1)
    }
    fh.order_hint = br.f::<u8>(sh.order_hint_bits as usize)?; // f(OrderHintBits)
    if frame_is_intra || fh.error_resilient_mode {
        fh.primary_ref_frame = PRIMARY_REF_NONE;
    } else {
        fh.primary_ref_frame = br.f::<u8>(3)?; // f(3)
    }
    if sh.decoder_model_info_present_flag {
        unimplemented!("decoder_model_info_present_flag==1");
    }
    fh.allow_high_precision_mv = false;
    fh.use_ref_frame_mvs = false;
    fh.allow_intrabc = false;
    if fh.frame_type == SWITCH_FRAME || (fh.frame_type == KEY_FRAME && fh.show_frame) {
        fh.refresh_frame_flags = all_frames;
    } else {
        fh.refresh_frame_flags = br.f::<u8>(8)?; // f(8)
    }
    if !frame_is_intra || fh.refresh_frame_flags != all_frames {
        if fh.error_resilient_mode && sh.enable_order_hint {
            for i in 0..NUM_REF_FRAMES {
                fh.ref_order_hint[i] = br.f::<u8>(sh.order_hint_bits as usize)?; // f(OrderHintBits)
                if fh.ref_order_hint[i] != rfman.ref_order_hint[i] {
                    rfman.ref_valid[i] = false;
                }
            }
        }
    }
    if fh.frame_type == KEY_FRAME {
        fh.frame_size = parse_frame_size(&mut br, sh, &fh)?; // frame_size()
        fh.render_size = parse_render_size(&mut br, &fh.frame_size)?; // render_size()
        if fh.allow_screen_content_tools
            && fh.frame_size.superres_params.upscaled_width == fh.frame_size.frame_width
        {
            fh.allow_intrabc = br.f::<bool>(1)?; // f(1)
        }
    } else {
        if fh.frame_type == INTRA_ONLY_FRAME {
            fh.frame_size = parse_frame_size(&mut br, sh, &fh)?; // frame_size()
            fh.render_size = parse_render_size(&mut br, &fh.frame_size)?; // render_size()
            if fh.allow_screen_content_tools
                && fh.frame_size.superres_params.upscaled_width == fh.frame_size.frame_width
            {
                fh.allow_intrabc = br.f::<bool>(1)?; // f(1)
            }
        } else {
            if !sh.enable_order_hint {
                fh.frame_refs_short_signaling = false;
            } else {
                fh.frame_refs_short_signaling = br.f::<bool>(1)?; // f(1)
                if fh.frame_refs_short_signaling {
                    fh.last_frame_idx = br.f::<u8>(3)?; // f(3)
                    fh.gold_frame_idx = br.f::<u8>(3)?; // f(3)
                                                        // set_frame_refs()
                }
            }
            for i in 0..REFS_PER_FRAME {
                if !fh.frame_refs_short_signaling {
                    fh.ref_frame_idx[i] = br.f::<u8>(3)?; // f(3)
                }
                if sh.frame_id_numbers_present_flag {
                    let delta_frame_id = br.f::<u16>(sh.delta_frame_id_length as usize)? + 1; // f(n)
                    let expected_frame_id =
                        (fh.current_frame_id + (1 << id_len) - delta_frame_id) % (1 << id_len);

                    // expectedFrameId[i] specifies the frame id for each frame used for reference.
                    // It is a requirement of bitstream conformance that whenever expectedFrameId[i] is calculated,
                    // the value matches RefFrameId[ref_frame_idx[i]] (this contains the value of current_frame_id
                    // at the time that the frame indexed by ref_frame_idx was stored).
                    assert_eq!(
                        expected_frame_id,
                        rfman.ref_frame_id[fh.ref_frame_idx[i] as usize]
                    );
                }
            }
            if fh.frame_size_override_flag && !fh.error_resilient_mode {
                unimplemented!("frame_size_with_refs()");
            } else {
                fh.frame_size = parse_frame_size(&mut br, sh, &fh)?; // frame_size()
                fh.render_size = parse_render_size(&mut br, &fh.frame_size)?; // render_size()
            }
            if fh.force_integer_mv {
                fh.allow_high_precision_mv = false;
            } else {
                fh.allow_high_precision_mv = br.f::<bool>(1)?; // f(1)
            }
            fh.interpolation_filter = parse_interpolation_filter(&mut br)?; // read_interpolation_filter()
            fh.is_motion_mode_switchable = br.f::<bool>(1)?; // f(1)
            if fh.error_resilient_mode || !sh.enable_ref_frame_mvs {
                fh.use_ref_frame_mvs = false;
            } else {
                fh.use_ref_frame_mvs = br.f::<bool>(1)?; // f(1)
            }
        }
    }
    if !frame_is_intra {
        for i in 0..REFS_PER_FRAME {
            let ref_frame = LAST_FRAME + i;
            let hint = rfman.ref_order_hint[fh.ref_frame_idx[i] as usize];
            fh.order_hints[ref_frame] = hint;
            if sh.enable_order_hint {
                // RefFrameSignBias[refFrame] = 0
            } else {
                // RefFrameSignBias[refFrame] = get_relative_dist(hint, OrderHint) > 0
            }
        }
    }
    if sh.reduced_still_picture_header || fh.disable_cdf_update {
        fh.disable_frame_end_update_cdf = true;
    } else {
        fh.disable_frame_end_update_cdf = br.f::<bool>(1)?; // f(1)
    }
    if fh.primary_ref_frame == PRIMARY_REF_NONE {
        // init_non_coeff_cdfs()
        // setup_past_independence()
    } else {
        // load_cdfs()
        // load_previous()
    }
    if fh.use_ref_frame_mvs {
        // motion_field_estimation()
    }
    fh.tile_info = parse_tile_info(&mut br, sh, &fh.frame_size)?; // tile_info()

    // quantization_params()
    // segmentation_params()
    // delta_q_params()
    // delta_lf_params()
    if fh.primary_ref_frame == PRIMARY_REF_NONE {
        // init_coeff_cdfs()
    } else {
        // load_previous_segment_ids()
    }
    // {
    //   SegQMLevel[][]
    // }
    // loop_filter_params()
    // cdef_params()
    // lr_params()
    // read_tx_mode()
    // frame_reference_mode()
    // skip_mode_params()
    if frame_is_intra || fh.error_resilient_mode || !sh.enable_warped_motion {
        fh.allow_warped_motion = false;
    } else {
        fh.allow_warped_motion = br.f::<bool>(1)?; // f(1)
    }
    fh.reduced_tx_set = br.f::<bool>(1)?; // f(1)

    // global_motion_params()
    // film_grain_params()

    Some(fh)
}
