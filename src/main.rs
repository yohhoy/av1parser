extern crate byteorder;

use byteorder::{ByteOrder, LittleEndian};
use std::env;
use std::fs::OpenOptions;
use std::io::{BufReader, Read};
use std::io::{Seek, SeekFrom};
use std::fmt;

const IVF_HDR_SIZE: usize = 32;
const IVF_SIGNATURE: [u8; 4] = *b"DKIF";
const IVF_VERSION: u16 = 0;
const IVF_FCC_AV01: [u8; 4] = *b"AV01"; // AV1 codec

const OBU_SEQUENCE_HEADER: u8 = 1;
const OBU_TEMPORAL_DELIMITER: u8 = 2;
const OBU_FRAME: u8 = 6;

///
/// IVF file header
/// https://wiki.multimedia.cx/index.php/IVF
///
struct IvfHeader {
    fcc: [u8; 4], // FourCC
    width: u16,   // [pel]
    height: u16,  // [pel]
    framerate: u16,
    timescale: u16,
    nframes: u32,
}

///
/// IVF frame
///
struct IvfFrame {
    size: u32, // [byte]
    pts: u64,
}

///
/// OBU(Open Bitstream Unit)
/// https://aomedia.org/av1-bitstream-and-decoding-process-specification/
///
#[derive(Debug)]
struct Obu {
    // obu_header()
    obu_type: u8,             // f(4)
    obu_extension_flag: bool, // f(1)
    obu_has_size_field: bool, // f(1)
    // obu_extension_header()
    temporal_id: u8, // f(3)
    spatial_id: u8,  // f(2)
    // open_bitstream_unit()
    obu_size: u32, // leb128()
    header_len: u32,
}

impl fmt::Display for Obu {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let obu_type = match self.obu_type {
            OBU_SEQUENCE_HEADER => "SEQUENCE_HEADER".to_owned(),
            OBU_TEMPORAL_DELIMITER => "TEMPORAL_DELIMITER".to_owned(),
            OBU_FRAME => "FRAME".to_owned(),
            _ => format!("(obu_type={})", self.obu_type)
        };
        write!(f, "{} size={}+{} T#{} S#{}", obu_type, self.header_len, self.obu_size, self.temporal_id, self.spatial_id)
    }
}


///
/// parse IVF file header
///
fn parse_ivf_header(mut ivf: &[u8]) -> Result<IvfHeader, String> {
    assert_eq!(ivf.len(), IVF_HDR_SIZE);
    // signature (4b)
    let mut sig = [0; 4];
    ivf.read_exact(&mut sig).unwrap();
    if sig != IVF_SIGNATURE {
        return Err("Invalid IVF signature".to_owned());
    }
    // versoin (2b)
    let mut ver = [0; 2];
    ivf.read_exact(&mut ver).unwrap();
    let ver = LittleEndian::read_u16(&ver);
    if ver != IVF_VERSION {
        return Err("Invalid IVF version".to_owned());
    }
    // header length (2b)
    let mut hdrlen = [0; 2];
    ivf.read_exact(&mut hdrlen).unwrap();
    let hdrlen = LittleEndian::read_u16(&hdrlen);
    if hdrlen != IVF_HDR_SIZE as u16 {
        return Err("Invalid IVF header length".to_owned());
    }
    // FourCC (4b)
    let mut fcc = [0; 4];
    ivf.read_exact(&mut fcc).unwrap();
    // width (2b), height (2b)
    let mut width = [0; 2];
    let mut height = [0; 2];
    ivf.read_exact(&mut width).unwrap();
    ivf.read_exact(&mut height).unwrap();
    let width = LittleEndian::read_u16(&width);
    let height = LittleEndian::read_u16(&height);
    // framerate(2b), timescale(2b)
    let mut framerate = [0; 2];
    let mut timescale = [0; 2];
    ivf.read_exact(&mut framerate).unwrap();
    ivf.read_exact(&mut timescale).unwrap();
    let framerate = LittleEndian::read_u16(&framerate);
    let timescale = LittleEndian::read_u16(&timescale);
    // num of frames (4b)
    let mut nframes = [0; 4];
    ivf.read_exact(&mut nframes).unwrap();
    let nframes = LittleEndian::read_u32(&nframes);

    Ok(IvfHeader {
        fcc: fcc,
        width: width,
        height: height,
        framerate: framerate,
        timescale: timescale,
        nframes: nframes,
    })
}

///
/// parse IVF frame header
///
fn parse_ivf_frame<R: Read>(bs: &mut R) -> Option<IvfFrame> {
    // frame size (4b)
    let mut size = [0; 4];
    match bs.read_exact(&mut size) {
        Ok(_) => (),
        Err(_) => return None,
    }
    // presentation timestamp (8b)
    let mut pts = [0; 8];
    match bs.read_exact(&mut pts) {
        Ok(_) => (),
        Err(_) => return None,
    }

    Some(IvfFrame {
        size: LittleEndian::read_u32(&size),
        pts: LittleEndian::read_u64(&pts),
    })
}

///
/// return (Leb128Bytes, leb128())
///
fn leb128<R: Read>(bs: &mut R) -> (u32, u32) {
    let mut value: u64 = 0;
    let mut leb128bytes = 0;
    for i in 0..8 {
        let mut leb128_byte = [0; 1];
        bs.read_exact(&mut leb128_byte).unwrap(); // f(8)
        let leb128_byte = leb128_byte[0];
        value |= ((leb128_byte & 0x7f) as u64) << (i * 7);
        leb128bytes += 1;
        if (leb128_byte & 0x80) != 0x80 {
            break;
        }
    }
    assert!(value <= (1u64 << 32) - 1);
    (leb128bytes, value as u32)
}

///
/// parse AV1 OBU
///
fn paese_av1_obu<R: Read>(bs: &mut R, sz: u32) -> std::io::Result<Obu> {
    // parse obu_header()
    let mut b1 = [0; 1];
    bs.read_exact(&mut b1)?;
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
        leb128(bs)
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

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("usage: {} <inout.ivf>...", args[0]);
        return Ok(());
    }

    for fname in &args[1..] {
        // open IVF file as read-only mode
        let f = OpenOptions::new().read(true).open(fname)?;
        let mut reader = BufReader::new(f);

        // parse IVF header
        let mut ivf_header = [0; IVF_HDR_SIZE];
        reader.read_exact(&mut ivf_header)?;
        match parse_ivf_header(&mut ivf_header) {
            Ok(hdr) => {
                if hdr.fcc != IVF_FCC_AV01 {
                    println!("{}: unsupport codec", fname);
                    continue;
                }
                let fcc = String::from_utf8(hdr.fcc.to_vec()).unwrap();
                println!(
                    "{}: fcc={:?} size={}x{} fps={} scale={} f={}",
                    fname, fcc, hdr.width, hdr.height, hdr.framerate, hdr.timescale, hdr.nframes
                );
            }
            Err(msg) => {
                println!("{}: {}", fname, msg);
                continue;
            }
        };

        // parse all frames
        while let Some(frame) = parse_ivf_frame(&mut reader) {
            println!("  F#{} size={}", frame.pts, frame.size);
            let mut sz = frame.size;
            let pos = reader.seek(SeekFrom::Current(0))?;
            // parse AV1 OBU frame
            while sz > 0 {
                let obu = paese_av1_obu(&mut reader, sz)?;
                println!("    {}", obu);
                sz -= obu.header_len + obu.obu_size;
                reader.seek(SeekFrom::Current(obu.obu_size as i64))?;
            }
            reader.seek(SeekFrom::Start(pos + frame.size as u64))?;
        }
    }
    Ok(())
}
