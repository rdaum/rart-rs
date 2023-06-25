use std::mem;

use num_traits::{ToBytes, Unsigned};

use crate::keys::KeyTrait;
use crate::partials::vector_partial::VectorPartial;

// Owns variable sized key data. Used especially for strings where a null-termination is required.
#[derive(Clone)]
pub struct VectorKey {
    data: Box<[u8]>,
}

impl VectorKey {
    pub fn new_from_string(s: &String) -> Self {
        let mut data = Vec::with_capacity(s.len() + 1);
        data.extend_from_slice(s.as_bytes());
        data.push(0);
        Self { data: data.into_boxed_slice() }
    }

    pub fn new_from_str(s: &str) -> Self {
        let mut data = Vec::with_capacity(s.len() + 1);
        data.extend_from_slice(s.as_bytes());
        data.push(0);
        Self { data: data.into_boxed_slice() }
    }

    pub fn new_from_slice(data: &[u8]) -> Self {
        let data = Vec::from(data);
        Self { data: data.into_boxed_slice() }
    }

    pub fn new_from_vec(data: Vec<u8>) -> Self {
        Self { data: data.into_boxed_slice() }
    }

    pub fn new_from_unsigned<T: Unsigned + ToBytes>(un: T) -> Self {
        Self::new_from_slice(un.to_be_bytes().as_ref())
    }
}

impl KeyTrait<VectorPartial> for VectorKey {
    fn at(&self, pos: usize) -> u8 {
        self.data[pos]
    }

    fn length_at(&self, at_depth: usize) -> usize {
        self.data.len() - at_depth
    }

    fn to_prefix(&self, at_depth: usize) -> VectorPartial {
        VectorPartial::from_slice(&self.data[at_depth..])
    }

    fn matches_slice(&self, slice: &[u8]) -> bool {
        self.data.len() == slice.len() && &self.data[..] == slice
    }
}

impl From<u8> for VectorKey {
    fn from(data: u8) -> Self {
        Self::new_from_unsigned(data)
    }
}

impl From<u16> for VectorKey {
    fn from(data: u16) -> Self {
        Self::new_from_unsigned(data)
    }
}

impl From<u32> for VectorKey {
    fn from(data: u32) -> Self {
        Self::new_from_unsigned(data)
    }
}

impl From<u64> for VectorKey {
    fn from(data: u64) -> Self {
        Self::new_from_unsigned(data)
    }
}

impl From<u128> for VectorKey {
    fn from(data: u128) -> Self {
        Self::new_from_unsigned(data)
    }
}

impl From<usize> for VectorKey {
    fn from(data: usize) -> Self {
        Self::new_from_unsigned(data)
    }
}

impl From<&str> for VectorKey {
    fn from(data: &str) -> Self {
        Self::new_from_str(data)
    }
}

impl From<String> for VectorKey {
    fn from(data: String) -> Self {
        Self::new_from_string(&data)
    }
}
impl From<&String> for VectorKey {
    fn from(data: &String) -> Self {
        Self::new_from_string(data)
    }
}

impl From<i8> for VectorKey {
    fn from(val: i8) -> Self {
        // flip upper bit of signed value to get comparable byte sequence:
        // -128 => 0
        // -127 => 1
        // 0 => 128
        // 1 => 129
        // 127 => 255
        let v: u8 = unsafe { mem::transmute(val) };
        // flip upper bit and set to 0 other bits:
        // (0000_1100 ^ 1000_0000) & 1000_0000 = 1000_0000
        // (1000_1100 ^ 1000_0000) & 1000_0000 = 0000_0000
        let i = (v ^ 0x80) & 0x80;
        // repair bits(except upper bit) of value:
        // self = -127
        // i = 0 (0b0000_0000)
        // v = 129 (0b1000_0001)
        // j = 0b0000_0000 | (0b1000_0001 & 0b0111_1111) = 0b0000_0000 | 0b0000_0001 = 0b0000_0001 = 1
        let j = i | (v & 0x7F);
        VectorKey::new_from_unsigned(j)
    }
}

impl From<i16> for VectorKey {
    fn from(val: i16) -> Self {
        let v: u16 = unsafe { mem::transmute(val) };
        let xor = 1 << 15;
        let i = (v ^ xor) & xor;
        let j = i | (v & (u16::MAX >> 1));
        VectorKey::new_from_unsigned(j)
    }
}

impl From<i32> for VectorKey {
    fn from(val: i32) -> Self {
        let v: u32 = unsafe { mem::transmute(val) };
        let xor = 1 << 31;
        let i = (v ^ xor) & xor;
        let j = i | (v & (u32::MAX >> 1));
        VectorKey::new_from_unsigned(j)
    }
}
impl From<i64> for VectorKey {
    fn from(val: i64) -> Self {
        let v: u64 = unsafe { mem::transmute(val) };
        let xor = 1 << 63;
        let i = (v ^ xor) & xor;
        let j = i | (v & (u64::MAX >> 1));
        VectorKey::new_from_unsigned(j)
    }
}
impl From<i128> for VectorKey {
    fn from(val: i128) -> Self {
        let v: u128 = unsafe { mem::transmute(val) };
        let xor = 1 << 127;
        let i = (v ^ xor) & xor;
        let j = i | (v & (u128::MAX >> 1));
        VectorKey::new_from_unsigned(j)
    }
}

impl From<isize> for VectorKey {
    fn from(val: isize) -> Self {
        let v: usize = unsafe { mem::transmute(val) };
        let xor = 1 << 63;
        let i = (v ^ xor) & xor;
        let j = i | (v & (usize::MAX >> 1));
        VectorKey::new_from_unsigned(j)
    }
}
