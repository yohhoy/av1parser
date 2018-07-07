extern crate byteorder;
extern crate hex;

use std::env;
use std::fs;
use std::io::{BufReader, Read};
use std::io::{Seek, SeekFrom};

mod av1;
mod bitio;
mod ivf;
mod obu;

const FCC_AV01: [u8; 4] = *b"AV01"; // AV1 codec

///
/// parse IVF file
///
fn parse_ivf_file<R: Read + Seek>(mut reader: R, fname: &str) -> std::io::Result<()> {
    // parse IVF header
    let mut ivf_header = [0; ivf::IVF_HEADER_SIZE];
    reader.read_exact(&mut ivf_header)?;
    match ivf::parse_ivf_header(&mut ivf_header) {
        Ok(hdr) => {
            if hdr.codec != FCC_AV01 {
                println!(
                    "{}: unsupport codec(0x{})",
                    fname,
                    hex::encode_upper(hdr.codec)
                );
                return Ok(());
            }
            let codec = String::from_utf8(hdr.codec.to_vec()).unwrap();
            println!(
                "{}: IVF codec={:?} size={}x{} fps={} scale={} f={}",
                fname, codec, hdr.width, hdr.height, hdr.framerate, hdr.timescale, hdr.nframes
            );
        }
        Err(msg) => {
            println!("{}: {}", fname, msg);
            return Ok(());
        }
    };

    //
    let mut sequence: Option<obu::SequenceHeader> = None;
    let mut rfman = av1::RefFrameManager::new();

    // parse all frames
    while let Ok(frame) = ivf::parse_ivf_frame(&mut reader) {
        println!("F#{} size={}", frame.pts, frame.size);
        let mut sz = frame.size;
        let pos = reader.seek(SeekFrom::Current(0))?;
        // parse OBUs(open bitstream unit)
        while sz > 0 {
            let obu = obu::parse_obu_header(&mut reader, sz)?;
            println!("  {}", obu);
            sz -= obu.header_len + obu.obu_size;
            let pos = reader.seek(SeekFrom::Current(0))?;
            match obu.obu_type {
                obu::OBU_SEQUENCE_HEADER => {
                    if let Some(sh) = obu::parse_sequence_header(&mut reader, obu.obu_size) {
                        println!("    {:?}", sh);
                        sequence = Some(sh);
                    }
                }
                obu::OBU_FRAME_HEADER | obu::OBU_FRAME => {
                    if let Some(fh) = obu::parse_frame_header(
                        &mut reader,
                        obu.obu_size,
                        sequence.as_ref().unwrap(),
                        &mut rfman,
                    ) {
                        println!("    {:?}", fh);
                        if obu.obu_type == obu::OBU_FRAME {
                            println!("    {:?}", rfman);
                            rfman.update_process(&fh);
                        }
                    }
                }
                _ => {}
            }
            reader.seek(SeekFrom::Start(pos + obu.obu_size as u64))?;
        }
        reader.seek(SeekFrom::Start(pos + frame.size as u64))?;
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("usage: {} <input.ivf>...", args[0]);
        return Ok(());
    }

    for fname in &args[1..] {
        // open IVF file as read-only mode
        let f = fs::OpenOptions::new().read(true).open(fname)?;

        let mut reader = BufReader::new(f);
        parse_ivf_file(reader, fname)?;
    }
    Ok(())
}
