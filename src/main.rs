extern crate byteorder;
extern crate hex;

use std::env;
use std::fs;
use std::io::{BufReader, Read};
use std::io::{Seek, SeekFrom};

mod ivf;
mod obu;

const FCC_AV01: [u8; 4] = *b"AV01"; // AV1 codec

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

        // parse IVF header
        let mut ivf_header = [0; ivf::IVF_HEADER_SIZE];
        reader.read_exact(&mut ivf_header)?;
        match ivf::parse_ivf_header(&mut ivf_header) {
            Ok(hdr) => {
                if hdr.fcc != FCC_AV01 {
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
        while let Ok(frame) = ivf::parse_ivf_frame(&mut reader) {
            println!("  F#{} size={}", frame.pts, frame.size);
            let mut sz = frame.size;
            let pos = reader.seek(SeekFrom::Current(0))?;
            // parse AV1 OBU frame
            while sz > 0 {
                let obu = obu::paese_av1_obu(&mut reader, sz)?;
                println!("    {}", obu);
                sz -= obu.header_len + obu.obu_size;
                reader.seek(SeekFrom::Current(obu.obu_size as i64))?;
            }
            reader.seek(SeekFrom::Start(pos + frame.size as u64))?;
        }
    }
    Ok(())
}
