extern crate av1parser;
extern crate byteorder;
#[macro_use]
extern crate clap;
extern crate hex;

#[cfg(feature = "metadata_hdr10plus")]
extern crate hdr10plus;

use av1parser::*;
use clap::{App, Arg};
use std::cmp;
use std::fs;
use std::io;
use std::io::{Seek, SeekFrom};

mod av1;
mod bitio;
mod ivf;
mod mkv;
mod mp4;
mod obu;

/// application global config
struct AppConfig {
    verbose: u64,
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
    let reader = &mut io::Read::take(reader, obu.obu_size as u64);
    match obu.obu_type {
        obu::OBU_SEQUENCE_HEADER => {
            if let Some(sh) = obu::parse_sequence_header(reader) {
                if config.verbose > 1 {
                    println!("  {:?}", sh);
                }
                seq.sh = Some(sh);
            } else {
                println!("  invalid SequenceHeader");
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
                    let error_resilient = if fh.error_resilient_mode { "*" } else { "" };
                    if fh.show_frame {
                        println!(
                            "  #{} {}{}, update({}), show@{}",
                            seq.rfman.decode_order,
                            av1::stringify::frame_type(fh.frame_type),
                            error_resilient,
                            av1::stringify::ref_frame(fh.refresh_frame_flags),
                            seq.rfman.present_order
                        );
                    } else {
                        println!(
                            "  #{} {}{}, update({}), {}",
                            seq.rfman.decode_order,
                            av1::stringify::frame_type(fh.frame_type),
                            error_resilient,
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
                        "    #{} ({}) show@{}",
                        seq.rfman.frame_buf[show_idx as usize],
                        av1::stringify::ref_frame(1 << show_idx),
                        seq.rfman.present_order,
                    );
                }
                if config.verbose > 1 {
                    println!("  {:?}", fh);
                }

                // decode_frame_wrapup(): Decode frame wrapup process
                if fh.show_frame || fh.show_existing_frame {
                    seq.rfman.output_process(&fh);
                }
                if !fh.show_existing_frame {
                    if config.verbose > 2 {
                        println!("  {:?}", seq.rfman);
                    }
                    seq.rfman.update_process(&fh);
                }
            }
        }
        obu::OBU_TILE_LIST => {
            if let Some(tl) = obu::parse_tile_list(reader) {
                if config.verbose > 2 {
                    println!("  {:?}", tl);
                }
            } else {
                println!("  invalid TileList")
            }
        }
        obu::OBU_METADATA => {
            if let Ok(metadata) = obu::parse_metadata_obu(reader) {
                if config.verbose > 1 {
                    println!("    {:?}", metadata);

                    if let obu::MetadataObu::ItutT35(m) = metadata {
                        match &m.itu_t_t35_payload_bytes[..7] {
                            [0xB5, 0x00, 0x3C, 0x00, 0x01, 0x04, 0x01] => {
                                println!("    ST2094-40 metadata");

                                // ST2094-40
                                // https://aomediacodec.github.io/av1-hdr10plus/#use-of-hdr10-with-av1-t35-obus
                                #[cfg(feature = "metadata_hdr10plus")] {
                                    let parsed_meta = hdr10plus::metadata::Hdr10PlusMetadata::parse(m.itu_t_t35_payload_bytes);
                                    println!("        {:?}", parsed_meta);
                                }
                            },
                            _ => (),
                        }
                    }
                }
            } else {
                println!("    invalid MetadataObu");
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
    match ivf::parse_ivf_header(&ivf_header) {
        Ok(hdr) => {
            let codec = String::from_utf8(hdr.codec.to_vec()).unwrap();
            println!(
                "{}: IVF codec={:?} size={}x{} timescale={}/{} length={}",
                fname,
                codec,
                hdr.width,
                hdr.height,
                hdr.timescale_num,
                hdr.timescale_den,
                hdr.length
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
        let pos = reader.stream_position()?;
        // parse OBU(open bitstream unit)s
        while sz > 0 {
            let obu = obu::parse_obu_header(&mut reader, sz)?;
            if config.verbose > 0 {
                println!("  {}", obu);
            }
            sz -= obu.header_len + obu.obu_size;
            let pos = reader.stream_position()?;
            process_obu(&mut reader, &mut seq, &obu, config);
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

    let codec_id = mkv::CODEC_V_AV1;
    let track_num = match webm.find_track(codec_id) {
        Some(num) => num,
        _ => {
            println!("{}: Matroska/WebM \"{}\" codec not found", fname, codec_id);
            return Ok(());
        }
    };
    match webm.get_videosetting(track_num) {
        Some(video) => println!(
            "{}: Matroska/WebM codec=\"{}\" size={}x{}",
            fname, codec_id, video.pixel_width, video.pixel_height
        ),
        None => println!(
            "{}: Matroska/WebM codec=\"{}\" size=(unknown)",
            fname, codec_id
        ),
    }

    let mut seq = av1::Sequence::new();

    // parse WebM block
    while let Ok(Some(block)) = webm.next_block(&mut reader) {
        if block.track_num != track_num {
            // skip non AV1 track data
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
            let pos = reader.stream_position()?;
            process_obu(&mut reader, &mut seq, &obu, config);
            reader.seek(SeekFrom::Start(pos + obu.obu_size as u64))?;
        }

        reader.seek(SeekFrom::Start(block.offset + block.size))?;
    }
    Ok(())
}

/// parse MP4(ISOBMFF) format
fn parse_mp4_format<R: io::Read + io::Seek>(
    mut reader: R,
    fname: &str,
    config: &AppConfig,
) -> io::Result<()> {
    // open MP4(ISOBMFF) file
    let mp4 = mp4::open_mp4file(&mut reader)?;
    if config.verbose > 1 {
        println!("  {:?}", mp4.get_filetype());
    }

    let brand_av01 = mp4::FCC::from(mp4::BRAND_AV01);
    let brands = &mp4.get_filetype().compatible_brands;
    if !brands.iter().any(|b| *b == brand_av01) {
        println!("{}: ISOBMFF/MP4 {} brand not found", fname, brand_av01);
        return Ok(());
    }
    let (av1se, av1cc) = match mp4.get_av1config() {
        Some(config) => config,
        None => {
            println!("{}: ISOBMFF/MP4 {} track not found", fname, brand_av01);
            return Ok(());
        }
    };
    println!(
        "{}: ISOBMFF/MP4 codec={} size={}x{}",
        fname, brand_av01, av1se.width, av1se.height
    );
    if config.verbose > 1 {
        println!("  {:?}", av1se);
        println!("  {:?}", av1cc);
    }

    let mut seq = av1::Sequence::new();

    // process AV1CodecConfigurationBox::configOBUs
    let mut cur = io::Cursor::new(av1cc.config_obus.clone());
    let mut config_sz = av1cc.config_obus.len() as u32;
    while config_sz > 0 {
        let obu = obu::parse_obu_header(&mut cur, config_sz)?;
        if config.verbose > 0 {
            println!("  {}", obu);
        }
        config_sz -= obu.header_len + obu.obu_size;
        process_obu(&mut cur, &mut seq, &obu, config);
    }

    // parse AV1 Samples
    for sample in mp4.get_samples() {
        reader.seek(SeekFrom::Start(sample.pos))?;
        let mut sz = sample.size;
        // parse OBU(open bitstream unit)s
        while sz > 0 {
            let obu_size = cmp::min(sz, u32::MAX as u64) as u32;
            let obu = obu::parse_obu_header(&mut reader, obu_size)?;
            if config.verbose > 0 {
                println!("  {}", obu);
            }
            sz -= (obu.header_len + obu.obu_size) as u64;
            let pos = reader.stream_position()?;
            process_obu(&mut reader, &mut seq, &obu, config);
            reader.seek(SeekFrom::Start(pos + obu.obu_size as u64))?;
        }
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
    let sz = u32::MAX;
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
        let pos = reader.stream_position()?;
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

    // probe media container format
    let fmt = probe_fileformat(&mut reader)?;
    reader.seek(SeekFrom::Start(0))?;

    match fmt {
        FileFormat::IVF => parse_ivf_format(reader, fname, config)?,
        FileFormat::WebM => parse_webm_format(reader, fname, config)?,
        FileFormat::MP4 => parse_mp4_format(reader, fname, config)?,
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
    let matches = app.get_matches();
    let config = AppConfig {
        verbose: matches.occurrences_of("v"),
    };

    for fname in matches.values_of("INPUT").unwrap() {
        process_file(fname, &config)?;
    }
    Ok(())
}
