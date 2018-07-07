//
// https://aomedia.org/av1-bitstream-and-decoding-process-specification/
//
use obu;

use obu::NUM_REF_FRAMES;

pub const INTRA_FRAME: usize = 0;
pub const LAST_FRAME: usize = 1;
pub const LAST2_FRAME: usize = 2;
pub const LAST3_FRAME: usize = 3;
pub const GOLDEN_FRAME: usize = 4;
pub const BWDREF_FRAME: usize = 5;
pub const ALTREF2_FRAME: usize = 6;
pub const ALTREF_FRAME: usize = 7;

///
/// Referenfe frame manager
///
#[derive(Debug)]
pub struct RefFrameManager {
    pub ref_valid: [bool; NUM_REF_FRAMES],    // RefValid[i]
    pub ref_frame_id: [u16; NUM_REF_FRAMES],  // RefFrameId[i]
    pub ref_frame_type: [u8; NUM_REF_FRAMES], // RefFrameType[i]
    pub ref_order_hint: [u8; NUM_REF_FRAMES], // RefOrderHint[i]
}

impl RefFrameManager {
    pub fn new() -> Self {
        RefFrameManager {
            ref_valid: [false; NUM_REF_FRAMES],
            ref_frame_id: [0; NUM_REF_FRAMES],
            ref_frame_type: [0; NUM_REF_FRAMES],
            ref_order_hint: [0; NUM_REF_FRAMES],
        }
    }

    /// Reference frame marking function
    pub fn mark_ref_frames(
        &mut self,
        id_len: usize,
        sh: &obu::SequenceHeader,
        fh: &obu::FrameHeader,
    ) {
        let diff_len = sh.delta_frame_id_length;
        for i in 0..NUM_REF_FRAMES {
            if fh.current_frame_id > (1 << diff_len) {
                if self.ref_frame_id[i] > fh.current_frame_id
                    || self.ref_frame_id[i] < fh.current_frame_id - (1 << diff_len)
                {
                    self.ref_valid[i] = false;
                }
            } else {
                if self.ref_frame_id[i] > fh.current_frame_id
                    && self.ref_frame_id[i]
                        < ((1 << id_len) + fh.current_frame_id - (1 << diff_len))
                {
                    self.ref_valid[i] = false;
                }
            }
        }
    }

    /// Reference frame update process
    pub fn update_process(&mut self, fh: &obu::FrameHeader) {
        for i in 0..NUM_REF_FRAMES {
            if (fh.refresh_frame_flags >> i) & 1 == 1 {
                self.ref_valid[i] = true;
                self.ref_frame_id[i] = fh.current_frame_id;
                self.ref_frame_type[i] = fh.frame_type;
                self.ref_order_hint[i] = fh.order_hint;
            }
        }
    }
}
