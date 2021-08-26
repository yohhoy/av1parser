use std::io;

/// numeric cast helper (u32 as T)
pub trait FromU32 {
    fn from_u32(v: u32) -> Self;
}

impl FromU32 for bool {
    #[inline]
    fn from_u32(v: u32) -> Self {
        v != 0
    }
}

macro_rules! impl_from_u32 {
    ($($ty:ty)*) => {
        $(
            impl FromU32 for $ty {
            #[inline]
                fn from_u32(v: u32) -> $ty {
                    v as $ty
                }
            }
        )*
    }
}

impl_from_u32!(u8 u16 u32 u64 usize);

///
/// Bitwise reader
///
pub struct BitReader<R> {
    inner: R,
    bbuf: u8,
    bpos: u8,
}

impl<R: io::Read> BitReader<R> {
    pub fn new(inner: R) -> BitReader<R> {
        BitReader {
            inner,
            bbuf: 0,
            bpos: 0,
        }
    }

    /// read_bit: read 1 bit
    pub fn read_bit(&mut self) -> Option<u8> {
        if self.bpos == 0 {
            let mut bbuf = [0; 1];
            match self.inner.read(&mut bbuf) {
                Ok(0) | Err(_) => return None, // EOF or IOErr
                Ok(n) => assert_eq!(n, 1),
            }
            self.bbuf = bbuf[0];
            self.bpos = 8;
        }
        self.bpos -= 1;
        Some((self.bbuf >> self.bpos) & 1)
    }

    /// f(n): read n-bits
    pub fn f<T: FromU32>(&mut self, nbit: usize) -> Option<T> {
        assert!(nbit <= 32);
        let mut x: u32 = 0;
        for _ in 0..nbit {
            x = (x << 1) | self.read_bit()? as u32;
        }
        Some(FromU32::from_u32(x))
    }

    /// su(n)
    pub fn su(&mut self, n: usize) -> Option<i32> {
        let mut value = self.f::<u32>(n)? as i32;
        let sign_mask = 1 << (n - 1);
        if value & sign_mask != 0 {
            value -= 2 * sign_mask
        }
        Some(value)
    }

    /// ns(n)
    pub fn ns(&mut self, n: u32) -> Option<u32> {
        let w = Self::floor_log2(n) + 1;
        let m = (1 << w) - n;
        let v = self.f::<u32>(w as usize - 1)?; // f(w - 1)
        if v < m {
            return Some(v);
        }
        let extra_bit = self.f::<u32>(1)?; // f(1)
        Some((v << 1) - m + extra_bit)
    }

    // FloorLog2(x)
    fn floor_log2(mut x: u32) -> u32 {
        let mut s = 0;
        while x != 0 {
            x >>= 1;
            s += 1;
        }
        s - 1
    }
}
