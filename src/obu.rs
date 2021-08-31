//
// https://aomedia.org/av1-bitstream-and-decoding-process-specification/
//
use crate::av1;
use crate::bitio::BitReader;
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

use crate::av1::{
    ALTREF2_FRAME, ALTREF_FRAME, BWDREF_FRAME, GOLDEN_FRAME, INTRA_FRAME, LAST2_FRAME, LAST3_FRAME,
    LAST_FRAME,
};

const REFS_PER_FRAME: usize = 7; // Number of reference frames that can be used for inter prediction
const TOTAL_REFS_PER_FRAME: usize = 8; // Number of reference frame types (including intra type)
const MAX_TILE_WIDTH: u32 = 4096; // Maximum width of a tile in units of luma samples
const MAX_TILE_AREA: u32 = 4096 * 2304; // Maximum area of a tile in units of luma samples
const MAX_TILE_ROWS: u32 = 64; // Maximum number of tile rows
const MAX_TILE_COLS: u32 = 64; // Maximum number of tile columns
pub const NUM_REF_FRAMES: usize = 8; // Number of frames that can be stored for future reference
const MAX_SEGMENTS: usize = 8; // Number of segments allowed in segmentation map
const SEG_LVL_MAX: usize = 8; // Number of segment features
const SELECT_SCREEN_CONTENT_TOOLS: u8 = 2; // Value that indicates the allow_screen_content_tools syntax element is coded
const SELECT_INTEGER_MV: u8 = 2; // Value that indicates the force_integer_mv syntax element is coded
const RESTORATION_TILESIZE_MAX: usize = 256; // Maximum size of a loop restoration tile
const PRIMARY_REF_NONE: u8 = 7; // Value of primary_ref_frame indicating that there is no primary reference frame
const SUPERRES_NUM: usize = 8; // Numerator for upscaling ratio
const SUPERRES_DENOM_MIN: usize = 9; // Smallest denominator for upscaling ratio
const SUPERRS_DENOM_BITS: usize = 3; // Number of bits sent to specify denominator of upscaling ratio
const MAX_LOOP_FILTER: i32 = 63; // Maximum value used for loop filtering
const WARPEDMODEL_PREC_BITS: usize = 16; // Internal precision of warped motion models
const GM_ABS_TRANS_BITS: usize = 12; // Number of bits encoded for translational components of global motion models, if part of a ROTZOOM or AFFINE model
const GM_ABS_TRANS_ONLY_BITS: usize = 9; // Number of bits encoded for translational components of global motion models, if part of a TRANSLATION model
const GM_ABS_ALPHA_BITS: usize = 12; // Number of bits encoded for non-translational components of global motion models
const GM_ALPHA_PREC_BITS: usize = 15; // Number of fractional bits for sending non-translational warp model coefacients
const GM_TRANS_PREC_BITS: usize = 6; // Number of fractional bits for sending translational warp model coefacients
const GM_TRANS_ONLY_PREC_BITS: usize = 3; // Number of fractional bits used for pure translational warps

// Color primaries
const CP_BT_709: u8 = 1; // BT.709
const CP_UNSPECIFIED: u8 = 2; // Unspecified

// Transfer characteristics
const TC_UNSPECIFIED: u8 = 2; // Unspecified
const TC_SRGB: u8 = 13; // sRGB or sYCC

// Matrix coefacients
const MC_IDENTITY: u8 = 0; // Identity matrix
const MC_UNSPECIFIED: u8 = 2; // Unspecified

// Chroma sample position
const CSP_UNKNOWN: u8 = 0; // Unknown (in this case the source video transfer function must be signaled outside the AV1 bitstream)

// Frame type
pub const KEY_FRAME: u8 = 0;
pub const INTER_FRAME: u8 = 1;
pub const INTRA_ONLY_FRAME: u8 = 2;
pub const SWITCH_FRAME: u8 = 3;

// interpolation_filter
const SWITCHABLE: u8 = 4;

// Loop restoration type (FrameRestorationType, not lr_type)
const RESTORE_NONE: u8 = 0;
const RESTORE_SWITCHABLE: u8 = 3;
const RESTORE_WIENER: u8 = 1;
const RESTORE_SGRPROJ: u8 = 2;

// TxMode
const ONLY_4X4: u8 = 0;
const TX_MODE_LARGEST: u8 = 1;
const TX_MODE_SELECT: u8 = 2;

const IDENTITY: u8 = 0; // Warp model is just an identity transform
const TRANSLATION: u8 = 1; // Warp model is a pure translation
const ROTZOOM: u8 = 2; // Warp model is a rotation + symmetric zoom + translation
const AFFINE: u8 = 3; // Warp model is a general afane transform

// OBU Metadata Type
const METADATA_TYPE_HDR_CLL: u32 = 1;
const METADATA_TYPE_HDR_MDCV: u32 = 2;
const METADATA_TYPE_SCALABILITY: u32 = 3;
const METADATA_TYPE_ITUT_T35: u32 = 4;
const METADATA_TYPE_TIMECODE: u32 = 5;

// scalability_mode_idc
const SCALABILITY_SS: u8 = 14;

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
#[derive(Clone, Copy, Debug, Default)]
pub struct ColorConfig {
    pub bit_depth: u8,  // BitDepth
    pub num_planes: u8, // NumPlanes
    // color_config()
    pub mono_chrome: bool,            // f(1)
    pub color_primaries: u8,          // f(8)
    pub transfer_characteristics: u8, // f(8)
    pub matrix_coefficients: u8,      // f(8)
    pub color_range: bool,            // f(1)
    pub subsampling_x: u8,            // f(1)
    pub subsampling_y: u8,            // f(1)
    pub chroma_sample_position: u8,   // f(2)
    pub separate_uv_delta_q: bool,    // f(1)
}

/// Timing info
#[derive(Clone, Copy, Debug, Default)]
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
#[derive(Clone, Copy, Debug, Default)]
pub struct OperatingPoint {
    pub operating_point_idc: u16, // f(12)
    pub seq_level_idx: u8,        // f(5)
    pub seq_tier: u8,             // f(1)
}

///
/// Sequence header OBU
///
#[derive(Clone, Copy, Debug, Default)]
pub struct SequenceHeader {
    pub seq_profile: u8,                          // f(3)
    pub still_picture: bool,                      // f(1)
    pub reduced_still_picture_header: bool,       // f(1)
    pub timing_info_present_flag: bool,           // f(1)
    pub timing_info: TimingInfo,                  // timing_info()
    pub decoder_model_info_present_flag: bool,    // f(1)
    pub initial_display_delay_present_flag: bool, // f(1)
    pub operating_points_cnt: u8,                 // f(5)
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
    pub seq_force_screen_content_tools: u8,       // f(1)
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
    pub frame_width: u32,  // FrameWidth
    pub frame_height: u32, // FrameHeight
    // superres_params()
    pub use_superres: bool,  // f(1)
    pub upscaled_width: u32, // UpscaledWidth
}

/// Render size
#[derive(Debug, Default)]
pub struct RenderSize {
    // render_size()
    pub render_width: u32,  // RenderWidth
    pub render_height: u32, // RenderHeight
}

/// Loop filter params
#[derive(Debug, Default)]
pub struct LoopFilterParams {
    // loop_filter_params()
    pub loop_filter_level: [u8; 4],                          // f(6)
    pub loop_filter_sharpness: u8,                           // f(3)
    pub loop_filter_delta_enabled: bool,                     // f(1)
    pub loop_filter_ref_deltas: [i32; TOTAL_REFS_PER_FRAME], // su(1+6)
    pub loop_filter_mode_deltas: [i32; 2],                   // su(1+6)
}

/// Tile info
#[derive(Debug, Default, Clone, Copy)]
pub struct TileInfo {
    pub tile_cols: u16, // TileCols
    pub tile_rows: u16, // TileRows
    // tile_info()
    pub context_update_tile_id: u32, // f(TileRowsLog2+TileColsLog2)
    pub tile_size_bytes: usize,      // TileSizeBytes
}

/// Quantization params
#[derive(Debug, Default)]
pub struct QuantizationParams {
    pub deltaq_y_dc: i32, // DeltaQYDc
    pub deltaq_u_dc: i32, // DeltaQUDc
    pub deltaq_u_ac: i32, // DeltaQUAc
    pub deltaq_v_dc: i32, // DeltaQVDc
    pub deltaq_v_ac: i32, // DeltaQVAc
    // quantization_params()
    pub base_q_idx: u8,      // f(8)
    pub using_qmatrix: bool, // f(1)
    pub qm_y: u8,            // f(4)
    pub qm_u: u8,            // f(4)
    pub qm_v: u8,            // f(4)
}

/// Segmentation params
#[derive(Debug, Default)]
pub struct SegmentationParams {
    // segmentation_params()
    pub segmentation_enabled: bool,         // f(1)
    pub segmentation_update_map: bool,      // f(1)
    pub segmentation_temporal_update: bool, // f(1)
    pub segmentation_update_data: bool,     // f(1)
}

/// Quantizer index delta parameters
#[derive(Debug, Default)]
pub struct DeltaQParams {
    // delta_q_params()
    pub delta_q_present: bool, // f(1)
    pub delta_q_res: u8,       // f(2)
}

/// Loop filter delta parameters
#[derive(Debug, Default)]
pub struct DeltaLfParams {
    // delta_lf_params()
    pub delta_lf_present: bool, // f(1)
    pub delta_lf_res: u8,       // f(2)
    pub delta_lf_multi: bool,   // f(1)
}

/// CDEF params
#[derive(Debug, Default)]
pub struct CdefParams {
    // cdef_params()
    pub cdef_damping: u8,              // f(2)
    pub cdef_bits: u8,                 // f(2)
    pub cdef_y_pri_strength: [u8; 8],  // f(4)
    pub cdef_y_sec_strength: [u8; 8],  // f(2)
    pub cdef_uv_pri_strength: [u8; 8], // f(4)
    pub cdef_uv_sec_strength: [u8; 8], // f(2)
}

/// Loop restoration params
#[derive(Debug, Default)]
pub struct LrParams {
    pub uses_lr: bool,                   // UsesLr
    pub frame_restoration_type: [u8; 3], // FrameRestorationType[]
    pub loop_restoration_size: [u8; 3],  // LoopRestorationSize[]
}

/// Skip mode params
#[derive(Debug, Default)]
pub struct SkipModeParams {
    pub skip_mode_frame: [u8; 2], // SkipModeFrame[]
    // skip_mode_params()
    pub skip_mode_present: bool, // f(1)
}

/// Global motion params
#[derive(Debug, Default)]
pub struct GlobalMotionParams {
    pub gm_type: [u8; NUM_REF_FRAMES],              // GmType[]
    pub gm_params: [[i32; 6]; NUM_REF_FRAMES],      // gm_params[]
    pub prev_gm_params: [[i32; 6]; NUM_REF_FRAMES], // PrevGmParams[][]
}

///
/// Frame header OBU
///
#[derive(Debug, Default)]
pub struct FrameHeader {
    // uncompressed_header()
    pub show_existing_frame: bool,                // f(1)
    pub frame_to_show_map_idx: u8,                // f(3)
    pub display_frame_id: u16,                    // f(idLen)
    pub frame_type: u8,                           // f(2)
    pub frame_is_intra: bool,                     // FrameIsIntra
    pub show_frame: bool,                         // f(1)
    pub showable_frame: bool,                     // f(1)
    pub error_resilient_mode: bool,               // f(1)
    pub disable_cdf_update: bool,                 // f(1)
    pub allow_screen_content_tools: bool,         // f(1)
    pub force_integer_mv: bool,                   // f(1)
    pub current_frame_id: u16,                    // f(idLen)
    pub frame_size_override_flag: bool,           // f(1)
    pub order_hint: u8,                           // f(OrderHintBits)
    pub primary_ref_frame: u8,                    // f(3)
    pub refresh_frame_flags: u8,                  // f(8)
    pub ref_order_hint: [u8; NUM_REF_FRAMES],     // f(OrderHintBits)
    pub frame_size: FrameSize,                    // frame_size()
    pub render_size: RenderSize,                  // render_size()
    pub allow_intrabc: bool,                      // f(1)
    pub last_frame_idx: u8,                       // f(3)
    pub gold_frame_idx: u8,                       // f(3)
    pub ref_frame_idx: [u8; NUM_REF_FRAMES],      // f(3)
    pub allow_high_precision_mv: bool,            // f(1)
    pub interpolation_filter: u8,                 // f(2)
    pub is_motion_mode_switchable: bool,          // f(1)
    pub use_ref_frame_mvs: bool,                  // f(1)
    pub disable_frame_end_update_cdf: bool,       // f(1)
    pub order_hints: [u8; NUM_REF_FRAMES],        // OrderHints
    pub tile_info: TileInfo,                      // tile_info()
    pub quantization_params: QuantizationParams,  // quantization_params()
    pub segmentation_params: SegmentationParams,  // segmentation_params()
    pub delta_q_params: DeltaQParams,             // delta_q_params()
    pub delta_lf_params: DeltaLfParams,           // delta_lf_params()
    pub coded_lossless: bool,                     // CodedLossless
    pub all_lossless: bool,                       // AllLossless
    pub loop_filter_params: LoopFilterParams,     // loop_filter_params()
    pub cdef_params: CdefParams,                  // cdef_params()
    pub lr_params: LrParams,                      // lr_params()
    pub tx_mode: u8,                              // TxMode
    pub skip_mode_params: SkipModeParams,         // skip_mode_params()
    pub global_motion_params: GlobalMotionParams, // global_motion_params()
    pub film_grain_params: FilmGrainParams,       // film_grain_params()
    pub reference_select: bool,                   // f(1)
    pub allow_warped_motion: bool,                // f(1)
    pub reduced_tx_set: bool,                     // f(1)
}

///
/// Tile list OBU
///
#[derive(Debug, Default)]
pub struct TileList {
    pub output_frame_width_in_tiles_minus_1: u8,  // f(8)
    pub output_frame_height_in_tiles_minus_1: u8, // f(8)
    pub tile_count_minus_1: u16,                  // f(16)
    pub tile_list_entries: Vec<TileListEntry>,    // tile_list_entry()
}

/// Tile list entry parameters
#[derive(Debug, Default)]
pub struct TileListEntry {
    pub anchor_frame_idx: u8,        // f(8)
    pub anchor_tile_row: u8,         // f(8)
    pub anchor_tile_col: u8,         // f(8)
    pub tile_data_size_minus_1: u16, // f(16)
}

/// Film grain synthesis parameters
#[derive(Debug, Default)]
pub struct FilmGrainParams {
    pub apply_grain: bool,              // f(1)
    pub grain_seed: u16,                // f(16)
    pub update_grain: bool,             // f(1)
    pub film_grain_params_ref_idx: u8,  // f(3)
    pub num_y_points: u8,               // f(4)
    pub point_y_value: Vec<u8>,         // f(8)
    pub point_y_scaling: Vec<u8>,       // f(8)
    pub chroma_scaling_from_luma: bool, // f(1)
    pub num_cb_points: u8,              // f(4)
    pub point_cb_value: Vec<u8>,        // f(8)
    pub point_cb_scaling: Vec<u8>,      // f(8)
    pub num_cr_points: u8,              // f(4)
    pub point_cr_value: Vec<u8>,        // f(8)
    pub point_cr_scaling: Vec<u8>,      // f(8)
    pub grain_scaling_minus_8: u8,      // f(2)
    pub ar_coeff_lag: u8,               // f(2)
    pub ar_coeffs_y_plus_128: Vec<u8>,  // f(8)
    pub ar_coeffs_cb_plus_128: Vec<u8>, // f(8)
    pub ar_coeffs_cr_plus_128: Vec<u8>, // f(8)
    pub ar_coeff_shift_minus_6: u8,     // f(2)
    pub grain_scale_shift: u8,          // f(2)
    pub cb_mult: u8,                    // f(8)
    pub cb_luma_mult: u8,               // f(8)
    pub cb_offset: u16,                 // f(9)
    pub cr_mult: u8,                    // f(8)
    pub cr_luma_mult: u8,               // f(8)
    pub cr_offset: u16,                 // f(9)
    pub overlap_flag: bool,             // f(1)
    pub clip_to_restricted_range: bool, // f(1)
}

#[derive(Debug, Default)]
pub struct ScalabilityStructure {
    pub spatial_layers_cnt_minus_1: u8,                // f(2)
    pub spatial_layer_dimensions_present_flag: bool,   // f(1)
    pub spatial_layer_description_present_flag: bool,  // f(1)
    pub temporal_group_description_present_flag: bool, // f(1)
    pub scalability_structure_reserved_3bits: u8,      // f(3)
    pub spatial_layer_max_width: Vec<u16>,             // f(16)
    pub spatial_layer_max_height: Vec<u16>,            // f(16)
    pub spatial_layer_ref_id: Vec<u8>,                 // f(8)
    pub temporal_group_size: u8,                       // f(8)
    pub temporal_group_temporal_id: Vec<u8>,           // f(3)
    pub temporal_group_temporal_switching_up_point_flag: Vec<bool>, // f(1)
    pub temporal_group_spatial_switching_up_point_flag: Vec<bool>, // f(1)
    pub temporal_group_ref_cnt: Vec<u8>,               // f(3)
    pub temporal_group_ref_pic_diff: Vec<Vec<u8>>,     // f(8)
}

// Metadata OBU structs
#[derive(Debug)]
pub enum MetadataObu {
    HdrCll(HdrCllMetadata),
    HdrMdcv(HdrMdcvMetadata),
    Scalability(ScalabilityMetadata),
    ItutT35(ItutT35Metadata),
    Timecode(TimecodeMetadata),
}

#[derive(Debug, Default)]
pub struct HdrCllMetadata {
    pub max_cll: u16,  // f(16)
    pub max_fall: u16, // f(16)
}

#[derive(Debug, Default)]
pub struct HdrMdcvMetadata {
    pub primary_chromaticity_x: [u16; 3],
    pub primary_chromaticity_y: [u16; 3],
    pub white_point_chromaticity_x: u16, // f(16)
    pub white_point_chromaticity_y: u16, // f(16)
    pub luminance_max: u32,              // f(32)
    pub luminance_min: u32,              // f(32)
}

#[derive(Debug, Default)]
pub struct ScalabilityMetadata {
    pub scalability_mode_idc: u8,                            // f(8)
    pub scalability_structure: Option<ScalabilityStructure>, // scalability_structure()
}

#[derive(Debug, Default)]
pub struct ItutT35Metadata {
    pub itu_t_t35_country_code: u8,                        // f(8)
    pub itu_t_t35_country_code_extension_byte: Option<u8>, // f(8)
    pub itu_t_t35_payload_bytes: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct TimecodeMetadata {
    pub counting_type: u8,         // f(5)
    pub full_timestamp_flag: bool, // f(1)
    pub discontinuity_flag: bool,  // f(1)
    pub cnt_dropped_flag: bool,    // f(1)
    pub n_frames: u16,             // f(9)
    pub seconds_value: u8,         // f(6)
    pub minutes_value: u8,         // f(6)
    pub hours_value: u8,           // f(5)
    pub seconds_flag: bool,        // f(1)
    pub minutes_flag: bool,        // f(1)
    pub hours_flag: bool,          // f(1)
    pub time_offset_length: u8,    // f(5)
    pub time_offset_value: u32,    // f(time_offset_length), 5 bits <= 31
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
    assert!(value < (1u64 << 32));
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
    cc.num_planes = if cc.mono_chrome { 1 } else { 3 };
    let color_description_present_flag = br.f::<bool>(1)?; // f(1)
    if color_description_present_flag {
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
        cc.subsampling_x = 1;
        cc.subsampling_y = 1;
        cc.chroma_sample_position = CSP_UNKNOWN;
        cc.separate_uv_delta_q = false;
        return Some(cc);
    } else if cc.color_primaries == CP_BT_709
        && cc.transfer_characteristics == TC_SRGB
        && cc.matrix_coefficients == MC_IDENTITY
    {
        cc.color_range = true;
        cc.subsampling_x = 0;
        cc.subsampling_y = 0;
        return Some(cc);
    } else {
        cc.color_range = br.f::<bool>(1)?; // f(1)
        if sh.seq_profile == 0 {
            cc.subsampling_x = 1;
            cc.subsampling_y = 1;
        } else if sh.seq_profile == 1 {
            cc.subsampling_x = 0;
            cc.subsampling_y = 0;
        } else {
            if cc.bit_depth == 12 {
                cc.subsampling_x = br.f::<u8>(1)?; // f(1)
                if cc.subsampling_x != 0 {
                    cc.subsampling_y = br.f::<u8>(1)?; // f(1)
                } else {
                    cc.subsampling_y = 0;
                }
            } else {
                cc.subsampling_x = 1;
                cc.subsampling_y = 0;
            }
        }
        if cc.subsampling_x != 0 && cc.subsampling_y != 0 {
            cc.chroma_sample_position = br.f::<u8>(2)?; // f(2)
        }
    }
    cc.separate_uv_delta_q = br.f::<bool>(1)?; // f(1)

    Some(cc)
}

///
/// parse timing_info()
///
fn parse_timing_info<R: io::Read>(br: &mut BitReader<R>) -> Option<TimingInfo> {
    let mut ti = TimingInfo::default();

    ti.num_units_in_display_tick = br.f::<u32>(32)?; // f(32)
    ti.time_scale = br.f::<u32>(32)?; // f(32)
    ti.equal_picture_interval = br.f::<bool>(1)?; // f(1)
    if ti.equal_picture_interval {
        ti.num_ticks_per_picture = 0 + 1; // uvlc()
        unimplemented!("uvlc() for num_ticks_per_picture_minus_1");
    }

    Some(ti)
}

///
/// parse frame_size() (include superres_params())
///
fn parse_frame_size<R: io::Read>(
    br: &mut BitReader<R>,
    sh: &SequenceHeader,
    fh: &FrameHeader,
) -> Option<FrameSize> {
    let mut fs = FrameSize::default();

    // frame_size()
    if fh.frame_size_override_flag {
        fs.frame_width = br.f::<u32>(sh.frame_width_bits as usize)? + 1; // f(n)
        fs.frame_height = br.f::<u32>(sh.frame_height_bits as usize)? + 1; // f(n)
    } else {
        fs.frame_width = sh.max_frame_width;
        fs.frame_height = sh.max_frame_height;
    }
    // superres_params()
    if sh.enable_superres {
        fs.use_superres = br.f::<bool>(1)?; // f(1)
    } else {
        fs.use_superres = false;
    }
    let supreres_denom;
    if fs.use_superres {
        let coded_denom = br.f::<usize>(SUPERRS_DENOM_BITS)?; // f(SUPERRES_DENOM_BITS)
        supreres_denom = coded_denom + SUPERRES_DENOM_MIN;
    } else {
        supreres_denom = SUPERRES_NUM;
    }
    fs.upscaled_width = fs.frame_width;
    fs.frame_width = ((fs.upscaled_width as usize * SUPERRES_NUM + (supreres_denom / 2))
        / supreres_denom) as u32;
    // compute_image_size()

    Some(fs)
}

///
/// parse render_size()
///
fn parse_render_size<R: io::Read>(br: &mut BitReader<R>, fs: &FrameSize) -> Option<RenderSize> {
    let mut rs = RenderSize::default();

    let render_and_frame_size_different = br.f::<bool>(1)?; // f(1)
    if render_and_frame_size_different {
        rs.render_width = br.f::<u32>(16)? + 1; // f(16)
        rs.render_height = br.f::<u32>(16)? + 1; // f(16)
    } else {
        rs.render_width = fs.upscaled_width;
        rs.render_height = fs.frame_height;
    }

    Some(rs)
}

/// read_interpolation_filter()
fn read_interpolation_filter<R: io::Read>(br: &mut BitReader<R>) -> Option<u8> {
    let is_filter_switchable = br.f::<bool>(1)?; // f(1)
    let interpolation_filter;
    if is_filter_switchable {
        interpolation_filter = SWITCHABLE;
    } else {
        interpolation_filter = br.f::<u8>(2)?; // f(2)
    }

    Some(interpolation_filter)
}

///
/// parse loop_filter_params()
///
fn parse_loop_filter_params<R: io::Read>(
    br: &mut BitReader<R>,
    cc: &ColorConfig,
    fh: &FrameHeader,
) -> Option<LoopFilterParams> {
    let mut lfp = LoopFilterParams::default();

    if fh.coded_lossless || fh.allow_intrabc {
        lfp.loop_filter_level[0] = 0;
        lfp.loop_filter_level[1] = 0;
        lfp.loop_filter_ref_deltas[INTRA_FRAME] = 1;
        lfp.loop_filter_ref_deltas[LAST_FRAME] = 0;
        lfp.loop_filter_ref_deltas[LAST2_FRAME] = 0;
        lfp.loop_filter_ref_deltas[LAST3_FRAME] = 0;
        lfp.loop_filter_ref_deltas[BWDREF_FRAME] = 0;
        lfp.loop_filter_ref_deltas[GOLDEN_FRAME] = -1;
        lfp.loop_filter_ref_deltas[ALTREF_FRAME] = -1;
        lfp.loop_filter_ref_deltas[ALTREF2_FRAME] = -1;
        for i in 0..2 {
            lfp.loop_filter_mode_deltas[i] = 0;
        }
        return Some(lfp);
    }
    lfp.loop_filter_level[0] = br.f::<u8>(6)?; // f(6)
    lfp.loop_filter_level[1] = br.f::<u8>(6)?; // f(6)
    if cc.num_planes > 1 {
        if lfp.loop_filter_level[0] != 0 || lfp.loop_filter_level[1] != 0 {
            lfp.loop_filter_level[2] = br.f::<u8>(6)?; // f(6)
            lfp.loop_filter_level[3] = br.f::<u8>(6)?; // f(6)
        }
    }
    lfp.loop_filter_sharpness = br.f::<u8>(3)?; // f(3)
    lfp.loop_filter_delta_enabled = br.f::<bool>(1)?; // f(1)
    if lfp.loop_filter_delta_enabled {
        let loop_filter_delta_update = br.f::<bool>(1)?; // f(1)
        if loop_filter_delta_update {
            for i in 0..TOTAL_REFS_PER_FRAME {
                let update_ref_delta = br.f::<bool>(1)?; // f(1)
                if update_ref_delta {
                    lfp.loop_filter_ref_deltas[i] = br.su(1 + 6)?; // su(1+6)
                }
            }
            for i in 0..2 {
                let update_mode_delta = br.f::<bool>(1)?; // f(1)
                if update_mode_delta {
                    lfp.loop_filter_mode_deltas[i] = br.su(1 + 6)?; // su(1+6)
                }
            }
        }
    }

    Some(lfp)
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

    let uniform_tile_spacing_flag = br.f::<bool>(1)?; // f(1)
    let (mut tile_cols_log2, mut tile_rows_log2): (usize, usize);
    if uniform_tile_spacing_flag {
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

        let min_log2_tile_rows =
            cmp::max(min_log2_tiles as isize - tile_cols_log2 as isize, 0) as usize;
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
            let width_in_sbs = br.ns(max_width)? + 1; // ns(maxWidth)
            let size_sb = width_in_sbs;
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
            let height_in_sbs = br.ns(max_height)? + 1; // ns(maxHeight)
            let size_sb = height_in_sbs;
            start_sb += size_sb;
            i += 1;
        }
        // MiRowStarts[i] = MiRows
        ti.tile_rows = i;
        tile_rows_log2 = tile_log2(1, ti.tile_rows as u32);
    }
    if tile_cols_log2 > 0 || tile_rows_log2 > 0 {
        ti.context_update_tile_id = br.f::<u32>(tile_cols_log2 + tile_rows_log2)?; // f(TileRowsLog2+TileColsLog2)
        ti.tile_size_bytes = br.f::<usize>(2)? + 1; // f(2)
    } else {
        ti.context_update_tile_id = 0;
    }

    Some(ti)
}

///
/// parse quantization_params()
///
fn parse_quantization_params<R: io::Read>(
    br: &mut BitReader<R>,
    cc: &ColorConfig,
) -> Option<QuantizationParams> {
    let mut qp = QuantizationParams::default();

    qp.base_q_idx = br.f::<u8>(8)?; // f(8)
    qp.deltaq_y_dc = read_delta_q(br)?; // read_delta_q()
    if cc.num_planes > 1 {
        let diff_uv_delta;
        if cc.separate_uv_delta_q {
            diff_uv_delta = br.f::<bool>(1)?; // f(1)
        } else {
            diff_uv_delta = false;
        }
        qp.deltaq_u_dc = read_delta_q(br)?; // read_delta_q()
        qp.deltaq_u_ac = read_delta_q(br)?; // read_delta_q()
        if diff_uv_delta {
            qp.deltaq_v_dc = read_delta_q(br)?; // read_delta_q()
            qp.deltaq_v_ac = read_delta_q(br)?; // read_delta_q()
        } else {
            qp.deltaq_v_dc = qp.deltaq_u_dc;
            qp.deltaq_v_ac = qp.deltaq_u_ac;
        }
    } else {
        qp.deltaq_u_dc = 0;
        qp.deltaq_u_ac = 0;
        qp.deltaq_v_dc = 0;
        qp.deltaq_v_ac = 0;
    }
    qp.using_qmatrix = br.f::<bool>(1)?; // f(1)
    if qp.using_qmatrix {
        qp.qm_y = br.f::<u8>(4)?; // f(4)
        qp.qm_u = br.f::<u8>(4)?; // f(4)
        if !cc.separate_uv_delta_q {
            qp.qm_v = qp.qm_u;
        } else {
            qp.qm_v = br.f::<u8>(4)?; // f(4)
        }
    }

    Some(qp)
}

/// Delta quantizer
fn read_delta_q<R: io::Read>(br: &mut BitReader<R>) -> Option<i32> {
    let delta_coded = br.f::<bool>(1)?; // f(1)
    let delta_q;
    if delta_coded {
        delta_q = br.su(1 + 6)?; // su(1+6)
    } else {
        delta_q = 0;
    }

    Some(delta_q as i32)
}

///
/// parse segmentation_params()
///
fn parse_segmentation_params<R: io::Read>(
    br: &mut BitReader<R>,
    fh: &FrameHeader,
) -> Option<SegmentationParams> {
    let mut sp = SegmentationParams::default();

    #[allow(non_upper_case_globals)]
    const Segmentation_Feature_Bits: [usize; SEG_LVL_MAX] = [8, 6, 6, 6, 6, 3, 0, 0];
    #[allow(non_upper_case_globals)]
    const Segmentation_Feature_Signed: [usize; SEG_LVL_MAX] = [1, 1, 1, 1, 1, 0, 0, 0];
    #[allow(non_upper_case_globals)]
    const Segmentation_Feature_Max: [i32; SEG_LVL_MAX] = [
        255,
        MAX_LOOP_FILTER,
        MAX_LOOP_FILTER,
        MAX_LOOP_FILTER,
        MAX_LOOP_FILTER,
        7,
        0,
        0,
    ];

    sp.segmentation_enabled = br.f::<bool>(1)?; // f(1)
    if sp.segmentation_enabled {
        if fh.primary_ref_frame == PRIMARY_REF_NONE {
            sp.segmentation_update_map = true;
            sp.segmentation_temporal_update = false;
            sp.segmentation_update_data = true;
        } else {
            sp.segmentation_update_map = br.f::<bool>(1)?; // f(1)
            if sp.segmentation_update_map {
                sp.segmentation_temporal_update = br.f::<bool>(1)?; // f(1)
            }
            sp.segmentation_update_data = br.f::<bool>(1)?; // f(1)
        }
        if sp.segmentation_update_data {
            for _ in 0..MAX_SEGMENTS {
                for j in 0..SEG_LVL_MAX {
                    let feature_value;
                    let feature_enabled = br.f::<bool>(1)?; // f(1)

                    // FeatureEnabled[i][j] = feature_enabled
                    let mut clipped_value = 0;
                    if feature_enabled {
                        let bits_to_read = Segmentation_Feature_Bits[j];
                        let limit = Segmentation_Feature_Max[j];
                        if Segmentation_Feature_Signed[j] == 1 {
                            feature_value = br.su(1 + bits_to_read)?; // su(1+bitsToRead)
                            clipped_value = cmp::max(-limit, cmp::min(limit, feature_value));
                        } else {
                            feature_value = br.f::<u32>(bits_to_read)? as i32; // f(bitsToRead)
                            clipped_value = cmp::max(0, cmp::min(limit, feature_value));
                        }
                    }
                    let _ = clipped_value; // FeatureData[i][j] = clippedValue
                }
            }
        }
    } else {
        // FeatureEnabled[i][j] = 0
        // FeatureData[i][j] = 0
    }
    // SegIdPreSkip
    // LastActiveSegId

    Some(sp)
}

///
/// parse delta_q_params()
///
fn parse_delta_q_params<R: io::Read>(
    br: &mut BitReader<R>,
    qp: &QuantizationParams,
) -> Option<DeltaQParams> {
    let mut dqp = DeltaQParams::default();

    dqp.delta_q_res = 0;
    dqp.delta_q_present = false;
    if qp.base_q_idx > 0 {
        dqp.delta_q_present = br.f::<bool>(1)?; // f(1)
    }
    if dqp.delta_q_present {
        dqp.delta_q_res = br.f::<u8>(2)?; // f(2)
    }

    Some(dqp)
}

///
/// parse delta_lf_params()
///
fn parse_delta_lf_params<R: io::Read>(
    br: &mut BitReader<R>,
    fh: &FrameHeader,
) -> Option<DeltaLfParams> {
    let mut dlfp = DeltaLfParams::default();

    dlfp.delta_lf_present = false;
    dlfp.delta_lf_res = 0;
    dlfp.delta_lf_multi = false;
    if fh.delta_q_params.delta_q_present {
        if !fh.allow_intrabc {
            dlfp.delta_lf_present = br.f::<bool>(1)?; // f(1)
        }
        if dlfp.delta_lf_present {
            dlfp.delta_lf_res = br.f::<u8>(2)?; // f(2)
            dlfp.delta_lf_multi = br.f::<bool>(1)?; // f(1)
        }
    }

    Some(dlfp)
}

///
/// parse cdef_params()
///
fn parse_cdef_params<R: io::Read>(
    br: &mut BitReader<R>,
    sh: &SequenceHeader,
    fh: &FrameHeader,
) -> Option<CdefParams> {
    let mut cdefp = CdefParams::default();

    if fh.coded_lossless || fh.allow_intrabc || !sh.enable_cdef {
        cdefp.cdef_bits = 0;
        cdefp.cdef_y_pri_strength[0] = 0;
        cdefp.cdef_y_sec_strength[0] = 0;
        cdefp.cdef_uv_pri_strength[0] = 0;
        cdefp.cdef_uv_sec_strength[0] = 0;
        cdefp.cdef_damping = 3;
        return Some(cdefp);
    }
    cdefp.cdef_damping = br.f::<u8>(2)? + 3; // f(2)
    cdefp.cdef_bits = br.f::<u8>(2)?; // f(2)
    for i in 0..(1 << cdefp.cdef_bits) {
        cdefp.cdef_y_pri_strength[i] = br.f::<u8>(4)?; // f(4)
        cdefp.cdef_y_sec_strength[i] = br.f::<u8>(2)?; // f(2)
        if cdefp.cdef_y_sec_strength[i] == 3 {
            cdefp.cdef_y_sec_strength[i] += 1;
        }
        if sh.color_config.num_planes > 1 {
            cdefp.cdef_uv_pri_strength[i] = br.f::<u8>(4)?; // f(4)
            cdefp.cdef_uv_sec_strength[i] = br.f::<u8>(2)?; // f(2)
            if cdefp.cdef_uv_sec_strength[i] == 3 {
                cdefp.cdef_uv_sec_strength[i] += 1;
            }
        }
    }

    Some(cdefp)
}

///
/// parse lr_params()
///
fn parse_lr_params<R: io::Read>(
    br: &mut BitReader<R>,
    sh: &SequenceHeader,
    fh: &FrameHeader,
) -> Option<LrParams> {
    let mut lrp = LrParams::default();

    #[allow(non_upper_case_globals)]
    const Remap_Lr_Type: [u8; 4] = [
        RESTORE_NONE,
        RESTORE_SWITCHABLE,
        RESTORE_WIENER,
        RESTORE_SGRPROJ,
    ];

    if fh.all_lossless || fh.allow_intrabc || !sh.enable_restoration {
        lrp.frame_restoration_type[0] = RESTORE_NONE;
        lrp.frame_restoration_type[1] = RESTORE_NONE;
        lrp.frame_restoration_type[2] = RESTORE_NONE;
        lrp.uses_lr = false;
        return Some(lrp);
    }
    lrp.uses_lr = false;
    let mut use_chroma_lr = false;
    for i in 0..sh.color_config.num_planes as usize {
        let lr_type = br.f::<usize>(2)?; // f(2)
        lrp.frame_restoration_type[i] = Remap_Lr_Type[lr_type];
        if lrp.frame_restoration_type[i] != RESTORE_NONE {
            lrp.uses_lr = true;
            if i > 0 {
                use_chroma_lr = true;
            }
        }
    }
    if lrp.uses_lr {
        let mut lr_unit_shift;
        if sh.use_128x128_superblock {
            lr_unit_shift = br.f::<u8>(1)?; // f(1)
            lr_unit_shift += 1;
        } else {
            lr_unit_shift = br.f::<u8>(1)?; // f(1)
            if lr_unit_shift != 0 {
                let lr_unit_extra_shift = br.f::<u8>(1)?; // f(1)
                lr_unit_shift += lr_unit_extra_shift;
            }
        }
        lrp.loop_restoration_size[0] = (RESTORATION_TILESIZE_MAX >> (2 - lr_unit_shift)) as u8;
        let lr_uv_shift;
        if sh.color_config.subsampling_x != 0 && sh.color_config.subsampling_y != 0 && use_chroma_lr
        {
            lr_uv_shift = br.f::<u8>(1)?; // f(1)
        } else {
            lr_uv_shift = 0;
        }
        lrp.loop_restoration_size[1] = lrp.loop_restoration_size[0] >> lr_uv_shift;
        lrp.loop_restoration_size[2] = lrp.loop_restoration_size[0] >> lr_uv_shift;
    }

    Some(lrp)
}

/// read_tx_mode()
fn read_tx_mode<R: io::Read>(br: &mut BitReader<R>, fh: &FrameHeader) -> Option<u8> {
    let tx_mode: u8;
    if fh.coded_lossless {
        tx_mode = ONLY_4X4;
    } else {
        let tx_mode_select = br.f::<bool>(1)?; // f(1)
        if tx_mode_select {
            tx_mode = TX_MODE_SELECT;
        } else {
            tx_mode = TX_MODE_LARGEST;
        }
    }

    Some(tx_mode)
}

///
/// parse skip_mode_params()
///
fn parse_skip_mode_params<R: io::Read>(
    br: &mut BitReader<R>,
    sh: &SequenceHeader,
    fh: &FrameHeader,
    rfman: &av1::RefFrameManager,
) -> Option<SkipModeParams> {
    let mut smp = SkipModeParams::default();

    let skip_mode_allowed;
    if fh.frame_is_intra || !fh.reference_select || !sh.enable_order_hint {
        skip_mode_allowed = false;
    } else {
        let mut forward_idx = -1;
        let mut backward_idx = -1;
        let (mut forward_hint, mut backward_hint) = (0, 0);
        for i in 0..REFS_PER_FRAME {
            let ref_hint = rfman.ref_order_hint[fh.ref_frame_idx[i] as usize] as i32;
            if av1::get_relative_dist(ref_hint, fh.order_hint as i32, sh) < 0 {
                if forward_idx < 0 || av1::get_relative_dist(ref_hint, forward_hint, sh) > 0 {
                    forward_idx = i as i32;
                    forward_hint = ref_hint;
                }
            } else if av1::get_relative_dist(ref_hint, fh.order_hint as i32, sh) > 0 {
                if backward_idx < 0 || av1::get_relative_dist(ref_hint, backward_hint, sh) < 0 {
                    backward_idx = i as i32;
                    backward_hint = ref_hint;
                }
            }
        }
        if forward_idx < 0 {
            skip_mode_allowed = false;
        } else if backward_idx >= 0 {
            skip_mode_allowed = true;
            smp.skip_mode_frame[0] =
                (LAST_FRAME as i32 + cmp::min(forward_idx, backward_idx)) as u8;
            smp.skip_mode_frame[1] =
                (LAST_FRAME as i32 + cmp::max(forward_idx, backward_idx)) as u8;
        } else {
            let mut second_forward_id = -1;
            let mut second_forward_hint = 0;
            for i in 0..REFS_PER_FRAME {
                let ref_hint = rfman.ref_order_hint[fh.ref_frame_idx[i] as usize] as i32;
                if av1::get_relative_dist(ref_hint, forward_hint, sh) < 0 {
                    if second_forward_id < 0
                        || av1::get_relative_dist(ref_hint, second_forward_hint, sh) > 0
                    {
                        second_forward_id = i as i32;
                        second_forward_hint = ref_hint;
                    }
                }
            }
            if second_forward_id < 0 {
                skip_mode_allowed = false;
            } else {
                skip_mode_allowed = true;
                smp.skip_mode_frame[0] =
                    (LAST_FRAME as i32 + cmp::min(forward_idx, second_forward_id)) as u8;
                smp.skip_mode_frame[1] =
                    (LAST_FRAME as i32 + cmp::max(forward_idx, second_forward_id)) as u8;
            }
        }
    }
    if skip_mode_allowed {
        smp.skip_mode_present = br.f::<bool>(1)?; // f(1)
    } else {
        smp.skip_mode_present = false;
    }

    Some(smp)
}

///
/// parse global_motion_params()
///
fn parse_global_motion_params<R: io::Read>(
    br: &mut BitReader<R>,
    fh: &FrameHeader,
) -> Option<GlobalMotionParams> {
    let mut gmp = GlobalMotionParams::default();

    for ref_ in LAST_FRAME..=ALTREF_FRAME {
        gmp.gm_type[ref_] = IDENTITY;
        for i in 0..6 {
            gmp.gm_params[ref_][i] = if i % 3 == 2 {
                1 << WARPEDMODEL_PREC_BITS
            } else {
                0
            };
        }
    }
    if fh.frame_is_intra {
        return Some(gmp);
    }
    for ref_ in LAST_FRAME..=ALTREF_FRAME {
        let is_global = br.f::<bool>(1)?; // f(1)
        let type_;
        if is_global {
            let is_rot_zoom = br.f::<bool>(1)?; // f(1)
            if is_rot_zoom {
                type_ = ROTZOOM;
            } else {
                let is_translation = br.f::<bool>(1)?; // f(1)
                type_ = if is_translation { TRANSLATION } else { AFFINE };
            }
        } else {
            type_ = IDENTITY;
        }
        gmp.gm_type[ref_] = type_;

        if type_ >= ROTZOOM {
            gmp.gm_params[ref_][2] = read_global_param(br, type_, ref_, 2, fh)?;
            gmp.gm_params[ref_][3] = read_global_param(br, type_, ref_, 3, fh)?;
            if type_ == AFFINE {
                gmp.gm_params[ref_][4] = read_global_param(br, type_, ref_, 4, fh)?;
                gmp.gm_params[ref_][5] = read_global_param(br, type_, ref_, 5, fh)?;
            } else {
                gmp.gm_params[ref_][4] = -gmp.gm_params[ref_][3];
                gmp.gm_params[ref_][5] = gmp.gm_params[ref_][2];
            }
        }
        if type_ > TRANSLATION {
            gmp.gm_params[ref_][0] = read_global_param(br, type_, ref_, 1, fh)?;
            gmp.gm_params[ref_][1] = read_global_param(br, type_, ref_, 0, fh)?;
        }
    }

    Some(gmp)
}

/// read_global_param() return gm_params[ref][idx]
fn read_global_param<R: io::Read>(
    br: &mut BitReader<R>,
    type_: u8,
    ref_: usize,
    idx: usize,
    fh: &FrameHeader,
) -> Option<i32> {
    let mut abs_bits = GM_ABS_ALPHA_BITS;
    let mut prec_bits = GM_ALPHA_PREC_BITS;
    if idx < 2 {
        if type_ == TRANSLATION {
            abs_bits = GM_ABS_TRANS_ONLY_BITS - if fh.allow_high_precision_mv { 0 } else { 1 };
            prec_bits = GM_TRANS_ONLY_PREC_BITS - if fh.allow_high_precision_mv { 0 } else { 1 };
        } else {
            abs_bits = GM_ABS_TRANS_BITS;
            prec_bits = GM_TRANS_PREC_BITS;
        }
    }
    let prec_diff = WARPEDMODEL_PREC_BITS - prec_bits;
    let round = if (idx % 3) == 2 {
        1 << WARPEDMODEL_PREC_BITS
    } else {
        0
    };
    let sub = if (idx % 3) == 2 { 1 << prec_bits } else { 0 };
    let mx = 1 << abs_bits;
    let r = (fh.global_motion_params.prev_gm_params[ref_][idx] >> prec_diff) - sub;
    let gm_params = (decode_signed_subexp_with_ref(br, -mx, mx + 1, r)? << prec_diff) + round;

    Some(gm_params)
}

/// decode_signed_subexp_with_ref()
fn decode_signed_subexp_with_ref<R: io::Read>(
    br: &mut BitReader<R>,
    low: i32,
    high: i32,
    r: i32,
) -> Option<i32> {
    let x = decode_unsigned_subexp_with_ref(br, high - low, r - low)?;
    Some(x + low)
}

/// decode_unsigned_subexp_with_ref()
fn decode_unsigned_subexp_with_ref<R: io::Read>(
    br: &mut BitReader<R>,
    mx: i32,
    r: i32,
) -> Option<i32> {
    let v = decode_subexp(br, mx)?;
    if (r << 1) <= mx {
        Some(inverse_recenter(r, v))
    } else {
        Some(mx - 1 - inverse_recenter(mx - 1 - r, v))
    }
}

/// decode_subexp()
fn decode_subexp<R: io::Read>(br: &mut BitReader<R>, num_syms: i32) -> Option<i32> {
    let mut i = 0;
    let mut mk = 0;
    let k = 3;
    loop {
        let b2 = if i != 0 { k + i - 1 } else { k };
        let a = 1 << b2;
        if num_syms <= mk + 3 * a {
            let subexp_final_bits = br.ns((num_syms - mk) as u32)? as i32; // ns(numSyms-mk)
            return Some(subexp_final_bits + mk);
        } else {
            let subexp_more_bits = br.f::<bool>(1)?; // f(1)
            if subexp_more_bits {
                i += 1;
                mk += a;
            } else {
                let subexp_bits = br.ns(b2)? as i32; // ns(b2)
                return Some(subexp_bits + mk);
            }
        }
    }
}

/// inverse_recenter()
#[inline]
fn inverse_recenter(r: i32, v: i32) -> i32 {
    if v > 2 * r {
        v
    } else if (v & 1) != 0 {
        r - ((v + 1) >> 1)
    } else {
        r + (v >> 1)
    }
}

///
/// parse film_grain_params()
///
fn parse_film_grain_params<R: io::Read>(
    br: &mut BitReader<R>,
    sh: &SequenceHeader,
    fh: &FrameHeader,
) -> Option<FilmGrainParams> {
    let mut fgp = FilmGrainParams::default();

    if !sh.film_grain_params_present || (!fh.show_frame && fh.showable_frame) {
        // reset_grain_params()
        return Some(fgp);
    }

    fgp.apply_grain = br.f::<bool>(1)?; // f(1)
    if !fgp.apply_grain {
        // reset_grain_params()
        return Some(fgp);
    }

    fgp.grain_seed = br.f::<u16>(16)?; // f(16)

    fgp.update_grain = if fh.frame_type == INTER_FRAME {
        br.f::<bool>(1)? // f(1)
    } else {
        true // 1
    };

    if !fgp.update_grain {
        fgp.film_grain_params_ref_idx = br.f::<u8>(3)?;

        assert!(fgp.film_grain_params_ref_idx <= (REFS_PER_FRAME - 1) as u8);
    }

    fgp.num_y_points = br.f::<u8>(4)?;

    assert!(fgp.num_y_points <= 14);

    for _ in 0..fgp.num_y_points {
        fgp.point_y_value.push(br.f::<u8>(8)?); // f(8)
        fgp.point_y_scaling.push(br.f::<u8>(8)?); // f(8)
    }

    let cc = sh.color_config;
    fgp.chroma_scaling_from_luma = if cc.mono_chrome {
        false // 0
    } else {
        br.f::<bool>(1)? // f(1)
    };

    if sh.color_config.mono_chrome
        || fgp.chroma_scaling_from_luma
        || (cc.subsampling_x == 1 && cc.subsampling_y == 1 && fgp.num_y_points == 0)
    {
        fgp.num_cb_points = 0;
        fgp.num_cr_points = 0;
    } else {
        fgp.num_cb_points = br.f::<u8>(4)?; // f(4)

        for _ in 0..fgp.num_cb_points {
            fgp.point_cb_value.push(br.f::<u8>(8)?); // f(8)
            fgp.point_cb_scaling.push(br.f::<u8>(8)?); // f(8)
        }

        fgp.num_cr_points = br.f::<u8>(4)?; // f(4)

        for _ in 0..fgp.num_cr_points {
            fgp.point_cr_value.push(br.f::<u8>(8)?); // f(8)
            fgp.point_cr_scaling.push(br.f::<u8>(8)?); // f(8)
        }
    }

    assert!(fgp.num_cb_points <= 10);
    assert!(fgp.num_cr_points <= 10);

    fgp.grain_scaling_minus_8 = br.f::<u8>(2)?; // f(2)
    fgp.ar_coeff_lag = br.f::<u8>(2)?; // f(2)
    let num_pos_luma = 2 * fgp.ar_coeff_lag * (fgp.ar_coeff_lag + 1);
    let num_pos_chroma;

    if fgp.num_y_points != 0 {
        num_pos_chroma = num_pos_luma + 1;

        for _ in 0..num_pos_luma {
            fgp.ar_coeffs_y_plus_128.push(br.f::<u8>(8)?); // f(8)
        }
    } else {
        num_pos_chroma = num_pos_luma;
    }

    if fgp.chroma_scaling_from_luma || fgp.num_cb_points != 0 {
        for _ in 0..num_pos_chroma {
            fgp.ar_coeffs_cb_plus_128.push(br.f::<u8>(8)?); // f(8)
        }
    }

    if fgp.chroma_scaling_from_luma || fgp.num_cr_points != 0 {
        for _ in 0..num_pos_chroma {
            fgp.ar_coeffs_cr_plus_128.push(br.f::<u8>(8)?); // f(8)
        }
    }

    fgp.ar_coeff_shift_minus_6 = br.f::<u8>(2)?; // f(2)
    fgp.grain_scale_shift = br.f::<u8>(2)?; // f(2)

    if fgp.num_cb_points != 0 {
        fgp.cb_mult = br.f::<u8>(8)?; // f(8)
        fgp.cb_luma_mult = br.f::<u8>(8)?; // f(8)
        fgp.cb_offset = br.f::<u16>(9)?; // f(9)
    }

    if fgp.num_cr_points != 0 {
        fgp.cr_mult = br.f::<u8>(8)?; // f(8)
        fgp.cr_luma_mult = br.f::<u8>(8)?; // f(8)
        fgp.cr_offset = br.f::<u16>(9)?; // f(9)
    }

    fgp.overlap_flag = br.f::<bool>(1)?; // f(1)
    fgp.clip_to_restricted_range = br.f::<bool>(1)?; // f(1)

    Some(fgp)
}

/// setup_past_independence()
fn setup_past_independence(fh: &mut FrameHeader) {
    // FeatureData[i][j]
    // PrevSegmentIds[row][col]
    for ref_ in LAST_FRAME..=ALTREF_FRAME {
        fh.global_motion_params.gm_type[ref_] = IDENTITY;
        for i in 0..=5 {
            fh.global_motion_params.prev_gm_params[ref_][i] = if i % 3 == 2 {
                1 << WARPEDMODEL_PREC_BITS
            } else {
                0
            };
        }
    }
    fh.loop_filter_params.loop_filter_delta_enabled = true;
    fh.loop_filter_params.loop_filter_ref_deltas[INTRA_FRAME] = 1;
    fh.loop_filter_params.loop_filter_ref_deltas[LAST_FRAME] = 0;
    fh.loop_filter_params.loop_filter_ref_deltas[LAST2_FRAME] = 0;
    fh.loop_filter_params.loop_filter_ref_deltas[LAST3_FRAME] = 0;
    fh.loop_filter_params.loop_filter_ref_deltas[BWDREF_FRAME] = 0;
    fh.loop_filter_params.loop_filter_ref_deltas[GOLDEN_FRAME] = -1;
    fh.loop_filter_params.loop_filter_ref_deltas[ALTREF_FRAME] = -1;
    fh.loop_filter_params.loop_filter_ref_deltas[ALTREF2_FRAME] = -1;
    fh.loop_filter_params.loop_filter_mode_deltas[0] = 0;
    fh.loop_filter_params.loop_filter_mode_deltas[1] = 0;
}

/// load_previous()
fn load_previous(fh: &mut FrameHeader, rfman: &av1::RefFrameManager) {
    let prev_frame = fh.ref_frame_idx[fh.primary_ref_frame as usize] as usize;
    fh.global_motion_params.prev_gm_params = rfman.saved_gm_params[prev_frame];
}

///
/// parse AV1 OBU header
///
pub fn parse_obu_header<R: io::Read>(bs: &mut R, sz: u32) -> io::Result<Obu> {
    // parse obu_header()
    let mut b1 = [0; 1];
    bs.read_exact(&mut b1)?;
    let obu_forbidden_bit = (b1[0] >> 7) & 1; // f(1)
    if obu_forbidden_bit != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "obu_forbidden_bit!=0",
        ));
    }
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
    let obu_header_len = 1 + (obu_extension_flag as u32);
    let (obu_size_len, obu_size) = if obu_has_size_field == 1 {
        leb128(bs)?
    } else {
        if sz < obu_header_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid sz in open_bitstream_unit()",
            ));
        }
        (0, sz - obu_header_len)
    };

    if sz < obu_header_len + obu_size_len + obu_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid OBU size",
        ));
    }

    Ok(Obu {
        obu_type,
        obu_extension_flag: obu_extension_flag == 1,
        obu_has_size_field: obu_has_size_field == 1,
        temporal_id,
        spatial_id,
        obu_size,
        header_len: obu_header_len + obu_size_len,
    })
}

///
/// parse sequence_header_obu()
///
pub fn parse_sequence_header<R: io::Read>(bs: &mut R) -> Option<SequenceHeader> {
    let mut br = BitReader::new(bs);
    let mut sh = SequenceHeader::default();

    sh.seq_profile = br.f::<u8>(3)?; // f(3)
    sh.still_picture = br.f::<bool>(1)?; // f(1)
    sh.reduced_still_picture_header = br.f::<bool>(1)?; // f(1)
    if sh.reduced_still_picture_header {
        sh.timing_info_present_flag = false;
        sh.decoder_model_info_present_flag = false;
        sh.initial_display_delay_present_flag = false;
        sh.operating_points_cnt = 1;
        sh.op[0].operating_point_idc = 0;
        sh.op[0].seq_level_idx = br.f::<u8>(5)?; // f(5)
        sh.op[0].seq_tier = 0;
        // decoder_model_present_for_this_op[0] = 0
        // initial_display_delay_present_for_this_op[0] = 0
        assert!(true);
    } else {
        sh.timing_info_present_flag = br.f::<bool>(1)?; // f(1)
        if sh.timing_info_present_flag {
            sh.timing_info = parse_timing_info(&mut br)?; // timing_info()
            sh.decoder_model_info_present_flag = br.f::<bool>(1)?; // f(1)
            if sh.decoder_model_info_present_flag {
                unimplemented!("decoder_model_info()");
            }
        } else {
            sh.decoder_model_info_present_flag = false;
        }
        sh.initial_display_delay_present_flag = br.f::<bool>(1)?; // f(1)
        sh.operating_points_cnt = br.f::<u8>(5)? + 1; // f(5)
        assert_eq!(sh.operating_points_cnt, 1); // FIXME: support single operating point
        for i in 0..(sh.operating_points_cnt) as usize {
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
    // operatingPoint = choose_operating_point()
    // OperatingPointIdc = operating_point_idc[operatingPoint]
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
        sh.enable_interintra_compound = false;
        sh.enable_masked_compound = false;
        sh.enable_warped_motion = false;
        sh.enable_dual_filter = false;
        sh.enable_order_hint = false;
        sh.enable_jnt_comp = false;
        sh.enable_ref_frame_mvs = false;
        sh.seq_force_screen_content_tools = SELECT_SCREEN_CONTENT_TOOLS;
        sh.seq_force_integer_mv = SELECT_INTEGER_MV;
        sh.order_hint_bits = 0;
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
        let seq_choose_screen_content_tools = br.f::<bool>(1)?; // f(1)
        if seq_choose_screen_content_tools {
            sh.seq_force_screen_content_tools = SELECT_SCREEN_CONTENT_TOOLS;
        } else {
            sh.seq_force_screen_content_tools = br.f::<u8>(1)?; // f(1)
        }
        if sh.seq_force_screen_content_tools > 0 {
            let seq_choose_integer_mv = br.f::<u8>(1)?; // f(1)
            if seq_choose_integer_mv > 0 {
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
    sh: &SequenceHeader,
    rfman: &mut av1::RefFrameManager,
) -> Option<FrameHeader> {
    let mut br = BitReader::new(bs);
    let mut fh = FrameHeader::default();

    // uncompressed_header()
    let id_len = if sh.frame_id_numbers_present_flag {
        sh.additional_frame_id_length + sh.delta_frame_id_length
    } else {
        0
    } as usize;
    assert!(id_len <= 16);
    assert!(NUM_REF_FRAMES <= 8);
    let all_frames = ((1usize << NUM_REF_FRAMES) - 1) as u8; // 0xff
    if sh.reduced_still_picture_header {
        fh.show_existing_frame = false;
        fh.frame_type = KEY_FRAME;
        fh.frame_is_intra = true;
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
        fh.frame_is_intra = fh.frame_type == INTRA_ONLY_FRAME || fh.frame_type == KEY_FRAME;
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
    if fh.frame_is_intra {
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
    if fh.frame_is_intra || fh.error_resilient_mode {
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
    if !fh.frame_is_intra || fh.refresh_frame_flags != all_frames {
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
            && fh.frame_size.upscaled_width == fh.frame_size.frame_width
        {
            fh.allow_intrabc = br.f::<bool>(1)?; // f(1)
        }
    } else {
        if fh.frame_type == INTRA_ONLY_FRAME {
            fh.frame_size = parse_frame_size(&mut br, sh, &fh)?; // frame_size()
            fh.render_size = parse_render_size(&mut br, &fh.frame_size)?; // render_size()
            if fh.allow_screen_content_tools
                && fh.frame_size.upscaled_width == fh.frame_size.frame_width
            {
                fh.allow_intrabc = br.f::<bool>(1)?; // f(1)
            }
        } else {
            let frame_refs_short_signaling;
            if !sh.enable_order_hint {
                frame_refs_short_signaling = false;
            } else {
                frame_refs_short_signaling = br.f::<bool>(1)?; // f(1)
                if frame_refs_short_signaling {
                    fh.last_frame_idx = br.f::<u8>(3)?; // f(3)
                    fh.gold_frame_idx = br.f::<u8>(3)?; // f(3)
                    unimplemented!("set_frame_refs()");
                }
            }
            for i in 0..REFS_PER_FRAME {
                if !frame_refs_short_signaling {
                    fh.ref_frame_idx[i] = br.f::<u8>(3)?; // f(3)

                    // ref_frame_idx[i] specifies which reference frames are used by inter frames.
                    // It is a requirement of bitstream conformance that RefValid[ref_frame_idx[i]] is equal to 1,
                    // and that the selected reference frames match the current frame in bit depth, profile,
                    // chroma subsampling, and color space.
                    assert!(rfman.ref_valid[fh.ref_frame_idx[i] as usize]);
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
            fh.interpolation_filter = read_interpolation_filter(&mut br)?; // read_interpolation_filter()
            fh.is_motion_mode_switchable = br.f::<bool>(1)?; // f(1)
            if fh.error_resilient_mode || !sh.enable_ref_frame_mvs {
                fh.use_ref_frame_mvs = false;
            } else {
                fh.use_ref_frame_mvs = br.f::<bool>(1)?; // f(1)
            }
        }
    }
    if !fh.frame_is_intra {
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
        setup_past_independence(&mut fh);
    } else {
        // load_cdfs(ref_frame_idx[primary_ref_frame])
        load_previous(&mut fh, rfman);
    }
    if fh.use_ref_frame_mvs {
        // motion_field_estimation()
    }
    fh.tile_info = parse_tile_info(&mut br, sh, &fh.frame_size)?; // tile_info()
    fh.quantization_params = parse_quantization_params(&mut br, &sh.color_config)?; // quantization_params()
    fh.segmentation_params = parse_segmentation_params(&mut br, &fh)?; // segmentation_params()
    fh.delta_q_params = parse_delta_q_params(&mut br, &fh.quantization_params)?; // delta_q_params()
    fh.delta_lf_params = parse_delta_lf_params(&mut br, &fh)?; // delta_lf_params()
    if fh.primary_ref_frame == PRIMARY_REF_NONE {
        // init_coeff_cdfs()
    } else {
        // load_previous_segment_ids()
    }
    fh.coded_lossless = false; // FIXME: assume lossy coding
    for _segment_id in 0..MAX_SEGMENTS {
        // CodedLossless
        // SegQMLevel[][segmentId]
    }
    fh.all_lossless =
        fh.coded_lossless && (fh.frame_size.frame_width == fh.frame_size.upscaled_width);
    fh.loop_filter_params = parse_loop_filter_params(&mut br, &sh.color_config, &fh)?; // loop_filter_params()
    fh.cdef_params = parse_cdef_params(&mut br, sh, &fh)?; // cdef_params()
    fh.lr_params = parse_lr_params(&mut br, sh, &fh)?; // lr_params()
    fh.tx_mode = read_tx_mode(&mut br, &fh)?; // read_tx_mode()
    {
        // frame_reference_mode()
        if fh.frame_is_intra {
            fh.reference_select = false;
        } else {
            fh.reference_select = br.f::<bool>(1)?; // f(1)
        }
    }
    fh.skip_mode_params = parse_skip_mode_params(&mut br, sh, &fh, rfman)?; // skip_mode_params()
    if fh.frame_is_intra || fh.error_resilient_mode || !sh.enable_warped_motion {
        fh.allow_warped_motion = false;
    } else {
        fh.allow_warped_motion = br.f::<bool>(1)?; // f(1)
    }
    fh.reduced_tx_set = br.f::<bool>(1)?; // f(1)
    fh.global_motion_params = parse_global_motion_params(&mut br, &fh)?; // global_motion_params()
    fh.film_grain_params = parse_film_grain_params(&mut br, sh, &fh)?; // film_grain_params()

    Some(fh)
}

///
/// parse tile_list_obu()
///
pub fn parse_tile_list<R: io::Read>(bs: &mut R) -> Option<TileList> {
    let mut br = BitReader::new(bs);
    let mut tl = TileList::default();

    tl.output_frame_width_in_tiles_minus_1 = br.f::<u8>(8)?;
    tl.output_frame_height_in_tiles_minus_1 = br.f::<u8>(8)?;
    tl.tile_count_minus_1 = br.f::<u16>(16)?;

    for _ in 0..=tl.tile_count_minus_1 {
        tl.tile_list_entries.push(parse_tile_list_entry(&mut br)?);
    }

    Some(tl)
}

///
/// parse tile_list_entry()
///
fn parse_tile_list_entry<R: io::Read>(br: &mut BitReader<R>) -> Option<TileListEntry> {
    let mut tle = TileListEntry::default();

    tle.anchor_frame_idx = br.f::<u8>(8)?;
    tle.anchor_tile_row = br.f::<u8>(8)?;
    tle.anchor_tile_col = br.f::<u8>(8)?;
    tle.tile_data_size_minus_1 = br.f::<u16>(16)?;

    Some(tle)
}

///
/// parse metadata_obu()
///
pub fn parse_metadata_obu<R: io::Read>(bs: &mut R) -> io::Result<MetadataObu> {
    let (_metadata_type_len, metadata_type) = leb128(bs)?;
    let mut br = BitReader::new(bs);

    let metadata = match metadata_type {
        METADATA_TYPE_HDR_CLL => parse_hdr_cll_metadata(&mut br),
        METADATA_TYPE_HDR_MDCV => parse_hdr_mdcv_metadata(&mut br),
        METADATA_TYPE_SCALABILITY => parse_scalability_metadata(&mut br),
        METADATA_TYPE_ITUT_T35 => parse_itu_t_t35_metadata(&mut br),
        METADATA_TYPE_TIMECODE => parse_timecode_metadata(&mut br),
        _ => None,
    };

    if let Some(metadata_obu) = metadata {
        Ok(metadata_obu)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Failed parsing metadata OBU or invalid metadata_type",
        ))
    }
}

///
/// parse metadata_hdr_cll()
///
fn parse_hdr_cll_metadata<R: io::Read>(br: &mut BitReader<R>) -> Option<MetadataObu> {
    let mut meta = HdrCllMetadata::default();

    meta.max_cll = br.f::<u16>(16)?; // f(16)
    meta.max_fall = br.f::<u16>(16)?; // f(16)

    Some(MetadataObu::HdrCll(meta))
}

///
/// parse metadata_hdr_mdcv()
///
fn parse_hdr_mdcv_metadata<R: io::Read>(br: &mut BitReader<R>) -> Option<MetadataObu> {
    let mut meta = HdrMdcvMetadata::default();

    for i in 0..3 {
        meta.primary_chromaticity_x[i] = br.f::<u16>(16)?; // f(16)
        meta.primary_chromaticity_y[i] = br.f::<u16>(16)?; // f(16)
    }

    meta.white_point_chromaticity_x = br.f::<u16>(16)?; // f(16)
    meta.white_point_chromaticity_y = br.f::<u16>(16)?; // f(16)
    meta.luminance_max = br.f::<u32>(32)?; // f(32)
    meta.luminance_min = br.f::<u32>(32)?; // f(32)

    Some(MetadataObu::HdrMdcv(meta))
}

///
/// parse metadata_scalability()
///
fn parse_scalability_metadata<R: io::Read>(br: &mut BitReader<R>) -> Option<MetadataObu> {
    let mut meta = ScalabilityMetadata::default();

    meta.scalability_mode_idc = br.f::<u8>(8)?; // f(8)
    if meta.scalability_mode_idc == SCALABILITY_SS {
        meta.scalability_structure = parse_scalability_structure(br);
    }

    Some(MetadataObu::Scalability(meta))
}

///
/// parse scalability_structure()
///
fn parse_scalability_structure<R: io::Read>(br: &mut BitReader<R>) -> Option<ScalabilityStructure> {
    let mut ss = ScalabilityStructure::default();

    ss.spatial_layers_cnt_minus_1 = br.f::<u8>(2)?; // f(2)
    ss.spatial_layer_dimensions_present_flag = br.f::<bool>(1)?; // f(1)
    ss.spatial_layer_description_present_flag = br.f::<bool>(1)?; // f(1)
    ss.temporal_group_description_present_flag = br.f::<bool>(1)?; // f(1)
    ss.scalability_structure_reserved_3bits = br.f::<u8>(3)?; // f(3)

    if ss.spatial_layer_dimensions_present_flag {
        for _ in 0..=ss.spatial_layers_cnt_minus_1 {
            ss.spatial_layer_max_width.push(br.f::<u16>(16)?); // f(16)
            ss.spatial_layer_max_height.push(br.f::<u16>(16)?); // f(16)
        }
    }

    if ss.spatial_layer_description_present_flag {
        for _ in 0..=ss.spatial_layers_cnt_minus_1 {
            ss.spatial_layer_ref_id.push(br.f::<u8>(8)?); // f(8)
        }
    }

    if ss.temporal_group_description_present_flag {
        ss.temporal_group_size = br.f::<u8>(8)?; // f(8)

        for i in 0..ss.temporal_group_size as usize {
            ss.temporal_group_temporal_id.push(br.f::<u8>(3)?); // f(3)
            ss.temporal_group_temporal_switching_up_point_flag
                .push(br.f::<bool>(1)?); // f(1)
            ss.temporal_group_spatial_switching_up_point_flag
                .push(br.f::<bool>(1)?); // f(1)
            ss.temporal_group_ref_cnt.push(br.f::<u8>(3)?); // f(3)

            for _ in 0..ss.temporal_group_ref_cnt[i] {
                ss.temporal_group_ref_pic_diff[i].push(br.f::<u8>(8)?); // f(8)
            }
        }
    }

    Some(ss)
}

///
/// parse metadata_itut_t35()
///
fn parse_itu_t_t35_metadata<R: io::Read>(br: &mut BitReader<R>) -> Option<MetadataObu> {
    let mut meta = ItutT35Metadata::default();

    meta.itu_t_t35_country_code = br.f::<u8>(8)?; // f(8)

    meta.itu_t_t35_country_code_extension_byte = if meta.itu_t_t35_country_code == 0xFF {
        br.f::<u8>(8) // f(8)
    } else {
        None
    };

    while let Some(byte) = br.f::<u8>(8) {
        meta.itu_t_t35_payload_bytes.push(byte);
    }

    Some(MetadataObu::ItutT35(meta))
}

///
/// parse metadata_timecode()
///
fn parse_timecode_metadata<R: io::Read>(br: &mut BitReader<R>) -> Option<MetadataObu> {
    let mut meta = TimecodeMetadata::default();

    meta.counting_type = br.f::<u8>(5)?; // f(5)
    meta.full_timestamp_flag = br.f::<bool>(1)?; // f(1)
    meta.discontinuity_flag = br.f::<bool>(1)?; // f(1)
    meta.cnt_dropped_flag = br.f::<bool>(1)?; // f(1)
    meta.n_frames = br.f::<u16>(9)?; // f(9)

    if meta.full_timestamp_flag {
        meta.seconds_value = br.f::<u8>(6)?; // f(6)
        meta.minutes_value = br.f::<u8>(6)?; // f(6)
        meta.hours_value = br.f::<u8>(5)?; // f(5)
    } else {
        meta.seconds_flag = br.f::<bool>(1)?; // f(1)

        if meta.seconds_flag {
            meta.seconds_value = br.f::<u8>(6)?; // f(6)
            meta.minutes_flag = br.f::<bool>(1)?; // f(1)

            if meta.minutes_flag {
                meta.minutes_value = br.f::<u8>(6)?; // f(6)
                meta.hours_flag = br.f::<bool>(1)?; // f(1)

                if meta.hours_flag {
                    meta.hours_value = br.f::<u8>(5)?; // f(5)
                }
            }
        }
    }

    meta.time_offset_length = br.f::<u8>(5)?; // f(5)

    if meta.time_offset_length > 0 {
        meta.time_offset_value = br.f::<u32>(meta.time_offset_length as usize)?;
        // f(time_offset_length)
    }

    Some(MetadataObu::Timecode(meta))
}
