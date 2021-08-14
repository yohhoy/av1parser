#![allow(dead_code)]
///
/// https://aomediacodec.github.io/av1-isobmff/
///
use byteorder::{BigEndian, ByteOrder};
use std::cmp;
use std::convert;
use std::fmt;
use std::io;
use std::io::{Read, SeekFrom};

pub const BOX_FILETYPE: [u8; 4] = *b"ftyp"; // FileType Box
const BOX_MEDIADATA: [u8; 4] = *b"mdat"; // Media Data Box
const BOX_MOVIE: [u8; 4] = *b"moov"; // Movie Box
const BOX_MOVIEHEADER: [u8; 4] = *b"mvhd"; // Movie Header Box
const BOX_TRACK: [u8; 4] = *b"trak"; // Track Box
const BOX_TRACKHEADER: [u8; 4] = *b"tkhd"; // Track Header Box
const BOX_MEDIA: [u8; 4] = *b"mdia"; // Media Box
const BOX_MEDIAINFORMATION: [u8; 4] = *b"minf"; // Media Information Box
const BOX_SAMPLETABLE: [u8; 4] = *b"stbl"; // Sample Table Box
const BOX_SAMPLEDESCRIPTION: [u8; 4] = *b"stsd"; // Sample Description Box
const BOX_SAMPLETOCHUNK: [u8; 4] = *b"stsc"; // Sample To Chunk Box
const BOX_SAMPLESIZE: [u8; 4] = *b"stsz"; // Sample Size Box
const BOX_CHUNKOFFSET: [u8; 4] = *b"stco"; // Chunk Offset Box/32bit
const BOX_CHUNKOFFSET64: [u8; 4] = *b"co64"; // Chunk Offset Box/64bit
const BOX_AV1SAMPLEENTRY: [u8; 4] = *b"av01"; // AV1 Sample Entry
const BOX_AV1CODECCONFIG: [u8; 4] = *b"av1C"; // AV1 Codec Configuration Box

pub const BRAND_AV01: [u8; 4] = *b"av01";

///
/// Four charactors code (u32)
///
#[derive(PartialEq)]
pub struct FCC {
    fcc: [u8; 4],
}

impl convert::From<[u8; 4]> for FCC {
    fn from(fcc: [u8; 4]) -> Self {
        Self { fcc }
    }
}

impl cmp::PartialEq<[u8; 4]> for FCC {
    #[must_use]
    fn eq(&self, other: &[u8; 4]) -> bool {
        self.fcc == *other
    }
}

impl fmt::Display for FCC {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            "{}{}{}{}",
            self.fcc[0] as char, self.fcc[1] as char, self.fcc[2] as char, self.fcc[3] as char
        ))
    }
}

impl fmt::Debug for FCC {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // same as fmt::Display
        f.write_fmt(format_args!("{}", self))
    }
}

fn read_fcc<R: io::Read>(mut reader: R) -> io::Result<FCC> {
    let mut fcc = [0; 4];
    reader.read_exact(&mut fcc)?;
    Ok(FCC { fcc })
}

fn read_u16<R: io::Read>(mut reader: R) -> io::Result<u16> {
    let mut value = [0; 2];
    reader.read_exact(&mut value)?;
    Ok(BigEndian::read_u16(&value))
}

fn read_u32<R: io::Read>(mut reader: R) -> io::Result<u32> {
    let mut value = [0; 4];
    reader.read_exact(&mut value)?;
    Ok(BigEndian::read_u32(&value))
}

fn read_u64<R: io::Read>(mut reader: R) -> io::Result<u64> {
    let mut value = [0; 8];
    reader.read_exact(&mut value)?;
    Ok(BigEndian::read_u64(&value))
}

/// read Box header, return (boxtype, payload_size)
fn read_box<R: io::Read>(mut reader: R) -> io::Result<(FCC, u64)> {
    let size = read_u32(&mut reader)? as u64;
    let boxtype = read_fcc(&mut reader)?;
    let payload_size = if size == 1 {
        let largesize = read_u64(&mut reader)?;
        if largesize < 16 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Too small Box(largesize={})", largesize),
            ));
        }
        largesize - 16
    } else if size == 0 {
        unimplemented!("box extends to end of file")
    } else {
        if size < 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Too small Box(size={})", size),
            ));
        }
        size as u64 - 8
    };
    Ok((boxtype, payload_size))
}

///
/// FileTypeBox
///
#[derive(Debug)]
pub struct FileTypeBox {
    pub major_brand: FCC,
    pub minor_version: u32,
    pub compatible_brands: Vec<FCC>,
}

/// read FileTypeBox
fn read_ftypbox<R: io::Read>(mut reader: R) -> io::Result<FileTypeBox> {
    let (boxtype, mut payload_size) = read_box(&mut reader)?;
    if boxtype != BOX_FILETYPE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid FileTypeBox boxtype={}", boxtype),
        ));
    }
    let major_brand = read_fcc(&mut reader)?;
    let minor_version = read_u32(&mut reader)?;
    payload_size -= 8;
    let mut compatible_brands = Vec::new();
    while 4 <= payload_size {
        compatible_brands.push(read_fcc(&mut reader)?);
        payload_size -= 4;
    }

    Ok(FileTypeBox {
        major_brand,
        minor_version,
        compatible_brands,
    })
}

///
/// AV1SampleEntry(VisualSampleEntry)
///
#[derive(Debug, Default)]
pub struct AV1SampleEntry {
    data_reference_index: u16, // ui(16)
    pub width: u16,            // ui(16)
    pub height: u16,           // ui(16)
    horizresolution: u32,      // ui(32)
    vertresolution: u32,       // ui(32)
    frame_count: u16,          // ui(16)
    compressorname: [u8; 32],  // string[32]
    depth: u16,                // ui(16)
}

fn read_av1sampleentry<R: io::Read>(mut reader: R) -> io::Result<AV1SampleEntry> {
    let mut av1se = AV1SampleEntry::default();

    // SampleEntry
    let mut _reserved0 = [0; 6];
    reader.read_exact(&mut _reserved0)?;
    av1se.data_reference_index = read_u16(&mut reader)?;
    // VisualSampleEntry
    let mut _reserved1 = [0; 16];
    reader.read_exact(&mut _reserved1)?;
    av1se.width = read_u16(&mut reader)?;
    av1se.height = read_u16(&mut reader)?;
    av1se.horizresolution = read_u32(&mut reader)?;
    av1se.vertresolution = read_u32(&mut reader)?;
    let _reserved2 = read_u32(&mut reader)?;
    av1se.frame_count = read_u16(&mut reader)?;
    reader.read_exact(&mut av1se.compressorname)?;
    av1se.depth = read_u16(&mut reader)?;
    let _pre_defined = read_u16(&mut reader)?;

    Ok(av1se)
}

///
/// AV1CodecConfigurationBox
///
#[derive(Debug, Default)]
pub struct AV1CodecConfigurationBox {
    pub seq_profile: u8,                          // ui(3)
    pub seq_level_idx_0: u8,                      // ui(5)
    pub seq_tier_0: u8,                           // ui(1)
    pub high_bitdepth: u8,                        // ui(1)
    pub twelve_bit: u8,                           // ui(1)
    pub monochrome: u8,                           // ui(1)
    pub chroma_subsampling_x: u8,                 // ui(1)
    pub chroma_subsampling_y: u8,                 // ui(1)
    pub chroma_sample_position: u8,               // ui(2)
    pub initial_presentation_delay_present: bool, // ui(1)
    pub initial_presentation_delay_minus_one: u8, // ui(4)
    pub config_obus: Vec<u8>,                     // ui(8)[]
}

fn read_av1codecconfig<R: io::Read>(
    mut reader: R,
    payload_size: u64,
) -> io::Result<AV1CodecConfigurationBox> {
    let mut av1cc = AV1CodecConfigurationBox::default();

    let mut bb = [0; 4];
    reader.read_exact(&mut bb)?;
    let (marker, version) = (bb[0] >> 7, bb[0] & 0x7f);
    if marker != 1 || version != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Invalid AV1CodecConfigurationBox(marker={}, version={})",
                marker, version
            ),
        ));
    }
    av1cc.seq_profile = bb[1] >> 5; // ui(3)
    av1cc.seq_level_idx_0 = bb[1] & 0x1f; // ui(5)
    av1cc.seq_tier_0 = bb[2] >> 7; // ui(1)
    av1cc.high_bitdepth = (bb[2] >> 6) & 1; // ui(1)
    av1cc.twelve_bit = (bb[2] >> 5) & 1; // ui(1)
    av1cc.monochrome = (bb[2] >> 4) & 1; // ui(1)
    av1cc.chroma_subsampling_x = (bb[2] >> 3) & 1; // ui(1)
    av1cc.chroma_subsampling_y = (bb[2] >> 2) & 1; // ui(1)
    av1cc.chroma_sample_position = bb[2] & 3; // ui(2)
    let _reserved = bb[3] >> 5; // ui(3)
    av1cc.initial_presentation_delay_present = ((bb[3] >> 4) & 1) != 0; // ui(1)
    av1cc.initial_presentation_delay_minus_one = bb[3] & 0xf; // ui(4)
    if 4 < payload_size {
        reader
            .take(payload_size - 4)
            .read_to_end(&mut av1cc.config_obus)?;
    }
    Ok(av1cc)
}

/// parse SampleDescriptionBox payload
fn parse_sampledescription<R: io::Read + io::Seek>(
    mut reader: R,
) -> io::Result<Option<(AV1SampleEntry, AV1CodecConfigurationBox)>> {
    let mut payload = None;
    let _version_flag = read_u32(&mut reader)?;
    let entry_count = read_u32(&mut reader)?;
    for _ in 0..entry_count {
        let (boxtype, size) = read_box(&mut reader)?;
        if boxtype == BOX_AV1SAMPLEENTRY {
            // read AV1SampleEntry
            let av1se = read_av1sampleentry(&mut reader)?;
            // read AV1CodecConfigurationBox
            let (boxtype, size) = read_box(&mut reader)?;
            if boxtype != BOX_AV1CODECCONFIG {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Invalid AV1CodecConfigurationBox(boxtype={}, size={})",
                        boxtype, size
                    ),
                ));
            }
            let av1cc = read_av1codecconfig(&mut reader, size)?;
            payload = Some((av1se, av1cc));
        } else {
            // ignore unknown SampleEntry
            reader.seek(SeekFrom::Current(size as i64))?;
        }
    }
    Ok(payload)
}

/// parse SampleToChunkBox payload
fn parse_sampletochunk<R: io::Read>(mut reader: R) -> io::Result<Vec<(u32, u32)>> {
    let mut stcs = Vec::new();
    let _version_flag = read_u32(&mut reader)?;
    let entry_count = read_u32(&mut reader)?;
    for _ in 1..=entry_count {
        let first_chunk = read_u32(&mut reader)?;
        let samples_per_chunk = read_u32(&mut reader)?;
        let _sample_description_index = read_u32(&mut reader)?;
        stcs.push((first_chunk, samples_per_chunk));
    }
    Ok(stcs)
}

/// parse SampleSizeBox payload
fn parse_samplesize<R: io::Read>(mut reader: R) -> io::Result<Vec<u32>> {
    let mut sizes = Vec::new();
    let _version_flag = read_u32(&mut reader)?;
    let sample_size = read_u32(&mut reader)?;
    let sample_count = read_u32(&mut reader)?;
    if sample_size == 0 {
        for _ in 1..=sample_count {
            let entry_size = read_u32(&mut reader)?;
            sizes.push(entry_size);
        }
    } else {
        for _ in 1..=sample_count {
            sizes.push(sample_size);
        }
    }
    Ok(sizes)
}

/// parse ChunkOffsetBox/ChunkLargeOffsetBox payload
fn parse_chunkoffset<R: io::Read>(mut reader: R, boxtype: FCC) -> io::Result<Vec<u64>> {
    assert!(boxtype == BOX_CHUNKOFFSET || boxtype == BOX_CHUNKOFFSET64);
    let mut offsets = Vec::new();
    let _version_flag = read_u32(&mut reader)?;
    let entry_count = read_u32(&mut reader)?;
    for _ in 0..entry_count {
        let chunk_offset = if boxtype == BOX_CHUNKOFFSET {
            read_u32(&mut reader)? as u64
        } else {
            read_u64(&mut reader)?
        };
        offsets.push(chunk_offset);
    }
    Ok(offsets)
}

/// parse TrackBox payload
fn parse_track<R: io::Read + io::Seek>(
    mut reader: R,
    size: u64,
    mp4: &mut IsoBmff,
) -> io::Result<bool> {
    let limit = reader.stream_position()? + size;
    let mut av1config = None;
    let (mut stcs, mut stsz, mut stco) = (Vec::new(), Vec::new(), Vec::new());
    loop {
        // read next Box
        let (boxtype, size) = match read_box(&mut reader) {
            Ok(result) => result,
            Err(err) => {
                if err.kind() == io::ErrorKind::UnexpectedEof {
                    break;
                } else {
                    return Err(err);
                }
            }
        };
        if boxtype == BOX_MEDIA || boxtype == BOX_MEDIAINFORMATION || boxtype == BOX_SAMPLETABLE {
            // parse nested Boxes
        } else if boxtype == BOX_SAMPLEDESCRIPTION {
            // parse SampleDescriptionBox
            av1config = parse_sampledescription(&mut reader)?;
        } else if boxtype == BOX_SAMPLETOCHUNK {
            // parse SampleToChunkBox
            stcs = parse_sampletochunk(&mut reader)?;
        } else if boxtype == BOX_SAMPLESIZE {
            // parse SampleSizeBox
            stsz = parse_samplesize(&mut reader)?;
        } else if boxtype == BOX_CHUNKOFFSET {
            // parse ChunkOffsetBox/ChunkLargeOffsetBox
            stco = parse_chunkoffset(&mut reader, boxtype)?;
        } else {
            reader.seek(SeekFrom::Current(size as i64))?;
        }
        if limit <= reader.stream_position()? {
            break;
        }
    }
    if av1config.is_none() {
        // This track is not 'av01' video
        return Ok(false);
    }
    mp4.av1config = av1config;

    // calculate Sample{pos,size} from stcs/stsz/stco
    let nsample = stsz.len();
    let mut samples = Vec::with_capacity(nsample);
    let (mut stcs_idx, mut stsz_idx, mut stco_idx) = (0, 0, 0);
    stcs.push((nsample as u32, 0)); // add sentinel
    let mut nsample_in_chunk = stcs[stcs_idx].1;
    while stsz_idx < nsample {
        let mut pos = stco[stco_idx];
        for _ in 0..nsample_in_chunk {
            let size = stsz[stsz_idx] as u64;
            samples.push(Sample { pos, size });
            pos += size;
            stsz_idx += 1;
        }
        stco_idx += 1;
        if stsz_idx + 1 >= stcs[stcs_idx + 1].0 as usize {
            stcs_idx += 1;
            nsample_in_chunk = stcs[stcs_idx].1;
        }
    }
    mp4.samples = samples;

    Ok(true)
}

///
/// Sample
///
#[derive(Debug)]
pub struct Sample {
    pub pos: u64,
    pub size: u64,
}

///
/// ISOBMFF/MP4 format
///
#[derive(Debug)]
pub struct IsoBmff {
    filetype: FileTypeBox,
    av1config: Option<(AV1SampleEntry, AV1CodecConfigurationBox)>,
    samples: Vec<Sample>,
}

impl IsoBmff {
    fn new(filetype: FileTypeBox) -> Self {
        IsoBmff {
            filetype,
            av1config: None,
            samples: Vec::new(),
        }
    }

    // get FileTypeBox
    pub fn get_filetype(&self) -> &FileTypeBox {
        &self.filetype
    }

    /// get (AV1SampleEntry, AV1CodecConfigurationBox)
    pub fn get_av1config(&self) -> Option<&(AV1SampleEntry, AV1CodecConfigurationBox)> {
        self.av1config.as_ref()
    }

    /// get 'av01' Samples
    pub fn get_samples(&self) -> &Vec<Sample> {
        &self.samples
    }
}

///
/// open ISOBMFF/MP4 file
///
pub fn open_mp4file<R: io::Read + io::Seek>(mut reader: R) -> io::Result<IsoBmff> {
    // read FileTypeBox
    let ftyp_box = read_ftypbox(&mut reader)?;
    let mut mp4 = IsoBmff::new(ftyp_box);
    loop {
        // read next Box
        let (boxtype, size) = match read_box(&mut reader) {
            Ok(result) => result,
            Err(err) => {
                if err.kind() == io::ErrorKind::UnexpectedEof {
                    break;
                } else {
                    return Err(err);
                }
            }
        };
        if boxtype == BOX_MOVIE {
            // parse nested Boxes
        } else if boxtype == BOX_TRACK {
            // parse TrackBox
            parse_track(&mut reader, size, &mut mp4)?;
        } else {
            reader.seek(SeekFrom::Current(size as i64))?;
        }
    }
    Ok(mp4)
}
