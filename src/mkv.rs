#![allow(dead_code)]
use byteorder::{BigEndian, ByteOrder};
///
/// https://matroska.org/technical/specs/index.html
///
use std::io;
use std::io::{Read, SeekFrom};

// Element ID
const ELEMENT_EBML: u32 = 0x1A45DFA3; // EBML header
const ELEMENT_SEGMENT: u32 = 0x18538067; // Segment
const ELEMENT_SEEKHEAD: u32 = 0x114D9B74; // Meta Seek Information
const ELEMENT_INFO: u32 = 0x1549A966; // Segment Information
const ELEMENT_CLUSTER: u32 = 0x1F43B675; // Cluster
const ELEMENT_TIMECODE: u32 = 0xE7; // Cluster/Timecode
const ELEMENT_SIMPLEBLOCK: u32 = 0xA3; // Cluster/SimpleBlock
const ELEMENT_BLOCKGROUP: u32 = 0xA0; // Cluster/BlockGroup
const ELEMENT_TRACKS: u32 = 0x1654AE6B; // Track
const ELEMENT_TRACKENTRY: u32 = 0xAE; // Tracks/TrackEntry
const ELEMENT_TRACKNUMBER: u32 = 0xD7; // Tracks/TrackEntry/TrackNumber
const ELEMENT_TRACKTYPE: u32 = 0x83; // Tracks/TrackEntry/TrackType
const ELEMENT_CODECID: u32 = 0x86; // Tracks/TrackEntry/CodecID
const ELEMENT_VIDEO: u32 = 0xE0; // Tracks/TrackEntry/Video
const ELEMENT_PIXELWIDTH: u32 = 0xB0; // Tracks/TrackEntry/Video/PixelWidth
const ELEMENT_PIXELHEIGHT: u32 = 0xBA; // Tracks/TrackEntry/Video/PixelHeight
const ELEMENT_CUES: u32 = 0x1C53BB6B; // Cueing Data

// Codec ID
pub const CODEC_V_AV1: &str = "V_AV1"; // video/AV1

/// Element ID (1-4 bytes)
fn read_elementid<R: io::Read>(mut reader: R) -> io::Result<u32> {
    let mut b0 = [0; 1];
    reader.read_exact(&mut b0)?;
    let value: u32;
    if b0[0] & 0b1000_0000 == 0b1000_0000 {
        value = b0[0] as u32;
    } else if b0[0] & 0b1100_0000 == 0b0100_0000 {
        let mut b1 = [0; 1];
        reader.read_exact(&mut b1)?;
        value = (b0[0] as u32) << 8 | b1[0] as u32;
    } else if b0[0] & 0b1110_0000 == 0b0010_0000 {
        let mut b2 = [0; 2];
        reader.read_exact(&mut b2)?;
        value = (b0[0] as u32) << 16 | (b2[0] as u32) << 8 | b2[1] as u32;
    } else if b0[0] & 0b1111_0000 == 0b0001_0000 {
        let mut b3 = [0; 3];
        reader.read_exact(&mut b3)?;
        value = (b0[0] as u32) << 24 | (b3[0] as u32) << 16 | (b3[1] as u32) << 8 | b3[2] as u32;
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid ElementID",
        ));
    }
    Ok(value)
}

/// variable length codeded integer, return (value, length)
fn read_varint<R: io::Read>(mut reader: R) -> io::Result<(i64, usize)> {
    let mut b0 = [0; 1];
    reader.read_exact(&mut b0)?;
    let mut value = b0[0] as i64;
    let lzcnt = b0[0].leading_zeros() as usize;
    if lzcnt > 7 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid leading zero bits",
        ));
    }
    value &= (1 << (7 - lzcnt)) - 1;
    if lzcnt > 0 {
        let mut buf = [0; 7];
        reader.take(lzcnt as u64).read(&mut buf)?;
        for i in 0..lzcnt {
            value = (value << 8) | buf[i] as i64;
        }
    }
    Ok((value, 1 + lzcnt))
}

/// Data size (1-8 bytes)
#[inline]
fn read_datasize<R: io::Read>(reader: R) -> io::Result<i64> {
    let (value, _) = read_varint(reader)?;
    Ok(value)
}

/// Unsigned integer (1-8 bytes), return
fn read_uint<R: io::Read>(reader: R, len: i64) -> io::Result<u64> {
    assert!(0 < len && len <= 8);
    let mut buf = [0; 8];
    reader.take(len as u64).read(&mut buf)?;
    let mut value = buf[0] as u64;
    for i in 1..(len as usize) {
        value = value << 8 | buf[i] as u64;
    }
    Ok(value)
}

/// String (1-n bytes)
fn read_string<R: io::Read>(reader: R, len: i64) -> io::Result<String> {
    assert!(0 < len);
    let mut value = String::new();
    reader.take(len as u64).read_to_string(&mut value)?;
    Ok(value)
}

///
/// Matorska format
///
#[derive(Debug)]
pub struct Matroska {
    tracks: Vec<TrackEntey>,
    clusters: Vec<Cluster>,
    curr_cluster: usize,
    curr_offset: u64,
}

impl Matroska {
    fn new() -> Self {
        Matroska {
            tracks: Vec::new(),
            clusters: Vec::new(),
            curr_cluster: 0,
            curr_offset: 0,
        }
    }

    /// find track with CodecID
    pub fn find_track(&self, codec_id: &str) -> Option<u64> {
        self.tracks
            .iter()
            .find(|t| t.codec_id == codec_id)
            .map(|t| t.track_num)
    }

    /// get Video settings
    pub fn get_videosetting(&self, track_num: u64) -> Option<&VideoTrack> {
        self.tracks
            .iter()
            .find(|t| t.track_num == track_num)
            .and_then(|t| t.setting.as_ref())
    }

    /// read next block
    pub fn next_block<R: io::Read + io::Seek>(
        &mut self,
        mut reader: R,
    ) -> io::Result<Option<Block>> {
        if self.curr_offset == 0 {
            if self.clusters.len() <= self.curr_cluster {
                return Ok(None); // end of clusters
            }
            self.curr_offset = self.clusters[self.curr_cluster].pos_begin;
        }
        reader.seek(SeekFrom::Start(self.curr_offset))?;
        loop {
            // seek to SimpleBlock element
            let node = read_elementid(&mut reader)?;
            let node_size = read_datasize(&mut reader)?;
            if node != ELEMENT_SIMPLEBLOCK {
                reader.seek(SeekFrom::Current(node_size))?;
                continue;
            }

            // read SimpleBlock header (4- bytes)
            let (track_num, len) = read_varint(&mut reader)?;
            let mut buf = [0; 3];
            reader.read_exact(&mut buf)?;
            let tc_offset = BigEndian::read_i16(&buf);
            let node_size = (node_size - (len as i64) - 3) as u64;
            let flags = buf[2];

            self.curr_offset = reader.stream_position()? + node_size;
            return Ok(Some(Block {
                track_num: track_num as u64,
                timecode: self.clusters[self.curr_cluster].timecode + (tc_offset as i64),
                flags: flags,
                offset: self.curr_offset,
                size: node_size,
            }));
        }
    }

    // TrackEntry element
    fn read_trackentry<R: io::Read + io::Seek>(mut reader: R) -> io::Result<TrackEntey> {
        let mut entry = TrackEntey::new();
        while let Ok(node) = read_elementid(&mut reader) {
            let node_size = read_datasize(&mut reader)?;
            match node {
                ELEMENT_TRACKNUMBER => entry.track_num = read_uint(&mut reader, node_size)?,
                ELEMENT_TRACKTYPE => entry.track_type = read_uint(&mut reader, node_size)?,
                ELEMENT_CODECID => entry.codec_id = read_string(&mut reader, node_size)?,
                ELEMENT_VIDEO => {
                    let mut node_body = Vec::with_capacity(node_size as usize);
                    node_body.resize(node_size as usize, 0);
                    reader.read_exact(&mut node_body)?;
                    let node_body = io::Cursor::new(node_body);
                    let video = Self::read_videoentry(node_body)?;
                    entry.setting = Some(video);
                }
                _ => {
                    reader.seek(SeekFrom::Current(node_size))?;
                }
            };
        }
        Ok(entry)
    }

    // Video element
    fn read_videoentry<R: io::Read + io::Seek>(mut reader: R) -> io::Result<VideoTrack> {
        let mut video = VideoTrack::new();
        while let Ok(node) = read_elementid(&mut reader) {
            let node_size = read_datasize(&mut reader)?;
            match node {
                ELEMENT_PIXELWIDTH => video.pixel_width = read_uint(&mut reader, node_size)?,
                ELEMENT_PIXELHEIGHT => video.pixel_height = read_uint(&mut reader, node_size)?,
                _ => {
                    reader.seek(SeekFrom::Current(node_size))?;
                }
            }
        }
        Ok(video)
    }

    // Track element
    fn read_track<R: io::Read + io::Seek>(&mut self, mut reader: R) -> io::Result<()> {
        let mut pos = reader.stream_position()?;
        // TrackEntry nodes
        while let Ok(entry) = read_elementid(&mut reader) {
            if entry != ELEMENT_TRACKENTRY {
                reader.seek(SeekFrom::Start(pos))?;
                break;
            }
            let entry_size = read_datasize(&mut reader)? as usize;

            // add new track
            let mut entry_body = Vec::with_capacity(entry_size);
            entry_body.resize(entry_size, 0);
            reader.read_exact(&mut entry_body)?;
            let entry_body = io::Cursor::new(entry_body);
            self.tracks.push(Self::read_trackentry(entry_body)?);

            pos = reader.stream_position()?;
        }
        Ok(())
    }

    // Cluster element
    fn read_cluster<R: io::Read + io::Seek>(
        &mut self,
        mut reader: R,
        node_size: i64,
    ) -> io::Result<()> {
        let mut pos = reader.seek(SeekFrom::Current(0))?;
        let limit_pos = pos + node_size as u64;

        let mut cluster = Cluster::new();
        cluster.pos_end = limit_pos;
        let mut first_block = true;

        // Level2 elements
        while let Ok(node) = read_elementid(&mut reader) {
            let node_size = read_datasize(&mut reader)?;
            match node {
                ELEMENT_TIMECODE => cluster.timecode = read_uint(&mut reader, node_size)? as i64,
                ELEMENT_SIMPLEBLOCK => {
                    if first_block {
                        // store offset of first Block
                        cluster.pos_begin = pos;
                        first_block = false;
                    }
                    reader.seek(SeekFrom::Current(node_size))?;
                }
                ELEMENT_BLOCKGROUP => unimplemented!("BlockGroup"),
                _ => {
                    reader.seek(SeekFrom::Current(node_size))?;
                }
            }

            pos = reader.stream_position()?;
            if limit_pos <= pos {
                break;
            }
        }
        self.clusters.push(cluster);
        Ok(())
    }
}

///
/// Matroska/TrackEntry
///
#[derive(Debug)]
struct TrackEntey {
    track_num: u64,
    track_type: u64,
    codec_id: String,
    setting: Option<VideoTrack>,
}

impl TrackEntey {
    fn new() -> Self {
        TrackEntey {
            track_num: 0,
            track_type: 0,
            codec_id: "".into(),
            setting: None,
        }
    }
}

///
/// Matroska/TrackEntry/Video settings
///
#[derive(Debug)]
pub struct VideoTrack {
    pub pixel_width: u64,  // PixelWidth
    pub pixel_height: u64, // PixelHeight
}

impl VideoTrack {
    fn new() -> Self {
        VideoTrack {
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

///
/// Matroska/Cluster
///
#[derive(Debug)]
struct Cluster {
    timecode: i64,
    pos_begin: u64,
    pos_end: u64,
}

impl Cluster {
    fn new() -> Self {
        Cluster {
            timecode: 0,
            pos_begin: 0,
            pos_end: 0,
        }
    }
}

///
/// Matroska/(Simple)Block
///
#[derive(Debug)]
pub struct Block {
    pub track_num: u64,
    pub timecode: i64,
    pub flags: u8,
    pub offset: u64,
    pub size: u64,
}

///
/// open Matroska/WebM file
///
pub fn open_mkvfile<R: io::Read + io::Seek>(mut reader: R) -> io::Result<Matroska> {
    // EBML header
    let ebml_tag = read_elementid(&mut reader)?;
    if ebml_tag != ELEMENT_EBML {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid EBML header",
        ));
    }
    let ebml_size = read_datasize(&mut reader)?;
    reader.seek(SeekFrom::Current(ebml_size))?;

    // Segment
    let segment_tag = read_elementid(&mut reader)?;
    if segment_tag != ELEMENT_SEGMENT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid Segment element",
        ));
    }
    let _segment_size = read_datasize(&mut reader)?;

    // Level1 elements
    let mut mkv = Matroska::new();
    while let Ok(node) = read_elementid(&mut reader) {
        let node_size = read_datasize(&mut reader)?;
        match node {
            ELEMENT_TRACKS => mkv.read_track(&mut reader)?,
            ELEMENT_CLUSTER => mkv.read_cluster(&mut reader, node_size)?,
            _ => {
                reader.seek(SeekFrom::Current(node_size))?;
            }
        };
    }

    Ok(mkv)
}
