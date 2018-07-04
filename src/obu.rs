//
// https://aomedia.org/av1-bitstream-and-decoding-process-specification/
//
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
/// parse AV1 OBU
///
pub fn paese_av1_obu<R: io::Read>(bs: &mut R, sz: u32) -> io::Result<Obu> {
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
