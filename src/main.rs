extern crate byteorder;
extern crate hex;

use std::env;
use std::fs;
use std::io;
use std::io::{Seek, SeekFrom};

mod av1;
mod bitio;
mod ivf;
mod obu;

const WEBM_SIGNATURE: [u8; 4] = [0x1A, 0x45, 0xDF, 0xA3]; // EBML(Matroska/WebM)

const FCC_AV01: [u8; 4] = *b"AV01"; // AV1 codec

enum FileFormat {
    IVF,
    WebM,
    Bitstream,
}

/// probe file format
fn probe_fileformat<R: io::Read>(reader: &mut R) -> io::Result<FileFormat> {
    let mut b4 = [0; 4];
    reader.read_exact(&mut b4)?;
    let type_ = match b4 {
        ivf::IVF_SIGNATURE => FileFormat::IVF,
        WEBM_SIGNATURE => FileFormat::WebM,
        _ => FileFormat::Bitstream,
    };
    Ok(type_)
}

///
/// process OBU(Open Bitstream Unit)
///
fn process_obu<R: io::Read>(reader: &mut R, seq: &mut av1::Sequence, obu: &obu::Obu) {
    match obu.obu_type {
        obu::OBU_SEQUENCE_HEADER => {
            if let Some(sh) = obu::parse_sequence_header(reader, obu.obu_size) {
                println!("    {:?}", sh);
                seq.sh = Some(sh);
            }
        }
        obu::OBU_FRAME_HEADER | obu::OBU_FRAME => {
            if seq.sh.is_none() {
                return;
            }
            if let Some(fh) = obu::parse_frame_header(
                reader,
                obu.obu_size,
                seq.sh.as_ref().unwrap(),
                &mut seq.rfman,
            ) {
                println!("    {:?}", fh);
                if obu.obu_type == obu::OBU_FRAME {
                    println!("    {:?}", seq.rfman);
                    seq.rfman.update_process(&fh);
                }
            }
        }
        _ => {}
    }
}

/// parse IVF format
fn parse_ivf_format<R: io::Read + io::Seek>(mut reader: R, fname: &str) -> io::Result<()> {
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

    let mut seq = av1::Sequence::new();

    // parse IVF frames
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
            process_obu(&mut reader, &mut seq, &obu);
            reader.seek(SeekFrom::Start(pos + obu.obu_size as u64))?;
        }
        reader.seek(SeekFrom::Start(pos + frame.size as u64))?;
    }
    Ok(())
}

/// parse low overhead bitstream format
fn parse_obu_bitstream<R: io::Read + io::Seek>(mut reader: R, fname: &str) -> io::Result<()> {
    println!("{}: Raw stream", fname);

    let mut seq = av1::Sequence::new();
    let sz = u32::max_value();

    while let Ok(obu) = obu::parse_obu_header(&mut reader, sz) {
        println!("  {}", obu);
        let pos = reader.seek(SeekFrom::Current(0))?;
        process_obu(&mut reader, &mut seq, &obu);
        reader.seek(SeekFrom::Start(pos + obu.obu_size as u64))?;
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("usage: {} <input.ivf|obu>...", args[0]);
        return Ok(());
    }

    for fname in &args[1..] {
        // open input file as read-only mode
        let f = fs::OpenOptions::new().read(true).open(fname)?;
        let mut reader = io::BufReader::new(f);

        // probe media containter format
        let fmt = probe_fileformat(&mut reader)?;
        reader.seek(SeekFrom::Start(0))?;

        match fmt {
            FileFormat::IVF => parse_ivf_format(reader, fname)?,
            FileFormat::WebM => println!("{}: (WebM format unsupported)", fname),
            FileFormat::Bitstream => parse_obu_bitstream(reader, fname)?,
        }
    }
    Ok(())
}
