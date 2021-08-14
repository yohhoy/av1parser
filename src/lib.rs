extern crate byteorder;
extern crate hex;

pub mod av1;
mod bitio;
pub mod ivf;
pub mod mkv;
pub mod mp4;
pub mod obu;

use std::io;

pub const FCC_AV01: [u8; 4] = *b"AV01"; // AV1 codec
const WEBM_SIGNATURE: [u8; 4] = [0x1A, 0x45, 0xDF, 0xA3]; // EBML(Matroska/WebM)

pub enum FileFormat {
    IVF,       // IVF format
    WebM,      // Matroska/WebM format
    MP4,       // ISOBMFF/MP4 format
    Bitstream, // Raw bitstream
}

/// probe file format
pub fn probe_fileformat<R: io::Read>(reader: &mut R) -> io::Result<FileFormat> {
    let mut b4 = [0; 4];
    reader.read_exact(&mut b4)?;
    let fmt = match b4 {
        ivf::IVF_SIGNATURE => FileFormat::IVF,
        WEBM_SIGNATURE => FileFormat::WebM,
        _ => {
            reader.read_exact(&mut b4)?;
            match b4 {
                mp4::BOX_FILETYPE => FileFormat::MP4,
                _ => FileFormat::Bitstream,
            }
        }
    };
    Ok(fmt)
}
