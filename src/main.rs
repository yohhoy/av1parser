extern crate byteorder;
#[macro_use]
extern crate clap;
extern crate hex;

use clap::{App, Arg};
use std::fs;
use std::io;
use std::io::{Seek, SeekFrom};

mod av1;
mod bitio;
mod ivf;
mod mkv;
mod obu;

const WEBM_SIGNATURE: [u8; 4] = [0x1A, 0x45, 0xDF, 0xA3]; // EBML(Matroska/WebM)

const FCC_AV01: [u8; 4] = *b"AV01"; // AV1 codec

/// application global config
struct AppConfig {
    verbose: u64,
}

enum FileFormat {
    IVF,       // IVF format
    WebM,      // Matroska/WebM format
    Bitstream, // Raw bitstream
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
fn process_obu<R: io::Read>(
    reader: &mut R,
    seq: &mut av1::Sequence,
    obu: &obu::Obu,
    config: &AppConfig,
) {
    match obu.obu_type {
        obu::OBU_SEQUENCE_HEADER => {
            if let Some(sh) = obu::parse_sequence_header(reader) {
                if config.verbose > 1 {
                    println!("  {:?}", sh);
                }
                seq.sh = Some(sh);
            }
        }
        obu::OBU_FRAME_HEADER | obu::OBU_FRAME => {
            if seq.sh.is_none() {
                if config.verbose > 1 {
                    println!("  no sequence header");
                }
                return;
            }
            if let Some(fh) =
                obu::parse_frame_header(reader, seq.sh.as_ref().unwrap(), &mut seq.rfman)
            {
                if !fh.show_existing_frame {
                    if fh.show_frame {
                        println!(
                            "  #{} {}, update({}), show",
                            seq.rfman.frame_counter,
                            av1::stringify::frame_type(fh.frame_type),
                            av1::stringify::ref_frame(fh.refresh_frame_flags)
                        );
                    } else {
                        println!(
                            "  #{} {}, update({}), {}",
                            seq.rfman.frame_counter,
                            av1::stringify::frame_type(fh.frame_type),
                            av1::stringify::ref_frame(fh.refresh_frame_flags),
                            if fh.showable_frame {
                                "showable"
                            } else {
                                "(refonly)"
                            }
                        );
                    }
                } else {
                    let show_idx = fh.frame_to_show_map_idx;
                    println!(
                        "  #{} show({})",
                        seq.rfman.frame_dts[show_idx as usize],
                        av1::stringify::ref_frame(1 << show_idx)
                    );
                }

                if config.verbose > 1 {
                    println!("  {:?}", fh);
                }
                if obu.obu_type == obu::OBU_FRAME {
                    if config.verbose > 2 {
                        println!("  {:?}", seq.rfman);
                    }
                    seq.rfman.update_process(&fh);
                }
            }
        }
        _ => {}
    }
}

/// parse IVF format
fn parse_ivf_format<R: io::Read + io::Seek>(
    mut reader: R,
    fname: &str,
    config: &AppConfig,
) -> io::Result<()> {
    // parse IVF header
    let mut ivf_header = [0; ivf::IVF_HEADER_SIZE];
    reader.read_exact(&mut ivf_header)?;
    match ivf::parse_ivf_header(&mut ivf_header) {
        Ok(hdr) => {
            let codec = String::from_utf8(hdr.codec.to_vec()).unwrap();
            println!(
                "{}: IVF codec={:?} size={}x{} fr={} scale={} n={}",
                fname, codec, hdr.width, hdr.height, hdr.framerate, hdr.timescale, hdr.nframes
            );
            if hdr.codec != FCC_AV01 {
                println!(
                    "{}: unsupport codec(0x{})",
                    fname,
                    hex::encode_upper(hdr.codec)
                );
                return Ok(());
            }
        }
        Err(msg) => {
            println!("{}: {}", fname, msg);
            return Ok(());
        }
    };

    let mut seq = av1::Sequence::new();

    // parse IVF frames
    while let Ok(frame) = ivf::parse_ivf_frame(&mut reader) {
        if config.verbose > 0 {
            println!("IVF F#{} size={}", frame.pts, frame.size);
        }
        let mut sz = frame.size;
        let pos = reader.seek(SeekFrom::Current(0))?;
        // parse OBU(open bitstream unit)s
        while sz > 0 {
            let obu = obu::parse_obu_header(&mut reader, sz)?;
            if config.verbose > 0 {
                println!("  {}", obu);
            }
            sz -= obu.header_len + obu.obu_size;
            let pos = reader.seek(SeekFrom::Current(0))?;
            process_obu(
                &mut io::Read::take(&mut reader, obu.obu_size as u64),
                &mut seq,
                &obu,
                config,
            );
            reader.seek(SeekFrom::Start(pos + obu.obu_size as u64))?;
        }
        reader.seek(SeekFrom::Start(pos + frame.size as u64))?;
    }
    Ok(())
}

/// parse WebM format
fn parse_webm_format<R: io::Read + io::Seek>(
    mut reader: R,
    fname: &str,
    config: &AppConfig,
) -> io::Result<()> {
    // open Matroska/WebM file
    let mut webm = mkv::open_mkvfile(&mut reader)?;
    println!("{}: Matroska/WebM", fname);

    let track_num = match webm.find_track(mkv::CODEC_V_AV1) {
        Some(num) => num,
        _ => {
            println!("{}: unsupported codec", fname);
            return Ok(());
        }
    };

    let mut seq = av1::Sequence::new();

    // parse WebM block
    while let Ok(block) = webm.next_block(&mut reader) {
        if block.size == 0 {
            break;
        }
        if block.track_num != track_num {
            continue;
        }

        if config.verbose > 0 {
            println!(
                "MKV F#{} flags=0x{:02x} size={}",
                block.timecode, block.flags, block.size
            );
        }
        let mut sz = block.size as u32;
        // parse OBU(open bitstream unit)s
        while sz > 0 {
            let obu = obu::parse_obu_header(&mut reader, sz)?;
            if config.verbose > 0 {
                println!("  {}", obu);
            }
            sz -= obu.header_len + obu.obu_size;
            let pos = reader.seek(SeekFrom::Current(0))?;
            process_obu(&mut reader, &mut seq, &obu, config);
            reader.seek(SeekFrom::Start(pos + obu.obu_size as u64))?;
        }

        reader.seek(SeekFrom::Start(block.offset + block.size))?;
    }
    Ok(())
}

/// parse low overhead bitstream format
fn parse_obu_bitstream<R: io::Read + io::Seek>(
    mut reader: R,
    fname: &str,
    config: &AppConfig,
) -> io::Result<()> {
    println!("{}: Raw stream", fname);

    let mut seq = av1::Sequence::new();
    let sz = u32::max_value();
    let mut fnum = 0;

    // parse OBU(open bitstream unit)s sequence
    while let Ok(obu) = obu::parse_obu_header(&mut reader, sz) {
        if config.verbose > 0 {
            if obu.obu_type == obu::OBU_TEMPORAL_DELIMITER {
                println!("Raw F#{}", fnum);
                fnum += 1;
            }
            println!("  {}", obu);
        }
        let pos = reader.seek(SeekFrom::Current(0))?;
        process_obu(&mut reader, &mut seq, &obu, config);
        reader.seek(SeekFrom::Start(pos + obu.obu_size as u64))?;
    }
    Ok(())
}

/// process input file
fn process_file(fname: &str, config: &AppConfig) -> io::Result<()> {
    // open input file as read-only mode
    let f = fs::OpenOptions::new().read(true).open(fname)?;
    let mut reader = io::BufReader::new(f);

    // probe media containter format
    let fmt = probe_fileformat(&mut reader)?;
    reader.seek(SeekFrom::Start(0))?;

    match fmt {
        FileFormat::IVF => parse_ivf_format(reader, fname, config)?,
        FileFormat::WebM => parse_webm_format(reader, fname, config)?,
        FileFormat::Bitstream => parse_obu_bitstream(reader, fname, config)?,
    };
    Ok(())
}

/// application entry point
fn main() -> std::io::Result<()> {
    let app = App::new(crate_name!())
        .version(crate_version!())
        .about(crate_description!())
        .arg(Arg::from_usage("<INPUT>... 'Input AV1 bitstream files'").index(1))
        .arg(Arg::from_usage("[v]... -v --verbose 'Show verbose log'"));

    // get commandline flags
    let mathces = app.get_matches();
    let config = AppConfig {
        verbose: mathces.occurrences_of("v"),
    };

    for fname in mathces.values_of("INPUT").unwrap() {
        process_file(fname, &config)?;
    }
    Ok(())
}
