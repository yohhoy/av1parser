extern crate byteorder;

use byteorder::{ByteOrder, LittleEndian};
use std::env;
use std::fs::OpenOptions;
use std::io::{BufReader, Read};
use std::io::{Seek, SeekFrom};

const IVF_HDR_SIZE: usize = 32;
const IVF_SIGNATURE: [u8; 4] = *b"DKIF";
const IVF_VERSION: u16 = 0;
const IVF_FCC_AV01: [u8; 4] = *b"AV01"; // AV1 codec

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
            println!("  #{} {}", frame.pts, frame.size);
            // TODO: parse AV1 OBU frame.
            reader.seek(SeekFrom::Current(frame.size as i64))?;
        }
    }
    Ok(())
}
