//! Allocation-free hex formatting.
//!
//! Given that most, if not all, of the types that we hex-format are of constant byte size (Hash,
//! Address, Identity), this hex implementation lets you format to hex without needing to allocate
//! a `String` on the heap.

use core::{fmt, ops, str};

#[derive(Copy, Clone)]
pub struct HexString<const N: usize> {
    s: [HexByte; N],
}

pub fn encode<const N: usize>(bytes: &[u8; N]) -> HexString<N> {
    let s = bytes.map(HexByte::from_byte);
    HexString { s }
}

impl<const N: usize> HexString<N> {
    #[inline(always)]
    pub fn as_str(&self) -> &str {
        self
    }

    #[inline(always)]
    pub fn as_bytes(&self) -> &[u8] {
        str::as_bytes(self)
    }
}

impl<const N: usize> ops::Deref for HexString<N> {
    type Target = str;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        HexByte::as_str(&self.s)
    }
}

impl<const N: usize> AsRef<str> for HexString<N> {
    fn as_ref(&self) -> &str {
        self
    }
}

impl<const N: usize> AsRef<[u8]> for HexString<N> {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl<const N: usize> fmt::Display for HexString<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(self)
    }
}

impl<const N: usize> fmt::Debug for HexString<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(self)
    }
}

#[derive(Copy, Clone)]
#[repr(u8)]
enum HexNybble {
    _0 = b'0',
    _1 = b'1',
    _2 = b'2',
    _3 = b'3',
    _4 = b'4',
    _5 = b'5',
    _6 = b'6',
    _7 = b'7',
    _8 = b'8',
    _9 = b'9',
    _A = b'a',
    _B = b'b',
    _C = b'c',
    _D = b'd',
    _E = b'e',
    _F = b'f',
}

#[rustfmt::skip]
const NYBBLE_LOOKUP: [HexNybble; 16] = [
    HexNybble::_0, HexNybble::_1, HexNybble::_2, HexNybble::_3, HexNybble::_4, HexNybble::_5, HexNybble::_6, HexNybble::_7,
    HexNybble::_8, HexNybble::_9, HexNybble::_A, HexNybble::_B, HexNybble::_C, HexNybble::_D, HexNybble::_E, HexNybble::_F,
];

#[derive(Copy, Clone)]
#[repr(transparent)]
struct HexByte([HexNybble; 2]);

static BYTE_LOOKUP: [HexByte; 256] = {
    let mut arr = [HexByte::ZERO; 256];
    let mut i = 0;
    while i < arr.len() {
        let (hi, lo) = (i >> 4, i & 0x0F);
        let byte = HexByte([NYBBLE_LOOKUP[hi], NYBBLE_LOOKUP[lo]]);
        arr[i] = byte;
        i += 1;
    }
    arr
};

impl HexByte {
    const ZERO: HexByte = HexByte([HexNybble::_0, HexNybble::_0]);

    #[inline(always)]
    fn from_byte(b: u8) -> Self {
        BYTE_LOOKUP[b as usize]
    }

    #[inline(always)]
    fn as_nybbles(this: &[Self]) -> &[HexNybble] {
        // SAFETY: HexByte is repr(transparent) over [HexNybble; 2]
        let arrays = unsafe { &*(this as *const [HexByte] as *const [[HexNybble; 2]]) };
        // SAFETY: this is equivalent to the unstable [[T; N]].flatten() -> &[T] method
        // TODO: switch to <[[T; N]]>::flatten() once stabilized
        unsafe { arrays.align_to::<HexNybble>().1 }
    }
    #[inline(always)]
    fn as_str(this: &[Self]) -> &str {
        let nybbles = HexByte::as_nybbles(this);
        // SAFETY: a HexNybble can only be valid ascii, which is always valid utf8
        unsafe { str::from_utf8_unchecked(HexNybble::as_bytes(nybbles)) }
    }
}

impl HexNybble {
    #[inline(always)]
    fn as_bytes(this: &[Self]) -> &[u8] {
        // SAFETY: HexNybble is repr(u8)
        unsafe { &*(this as *const [HexNybble] as *const [u8]) }
    }
}
