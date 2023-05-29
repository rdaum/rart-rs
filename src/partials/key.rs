use std::mem;

use num_traits::{ToBytes, Unsigned};

pub trait Key: Clone {
    fn at(&self, pos: usize) -> u8;
    fn length(&self) -> usize;
    fn partial_after(&self, pos: usize) -> &[u8];
    fn as_slice(&self) -> &[u8] {
        self.partial_after(0)
    }
}

// Non-owning byte slice key.
#[derive(Clone)]
pub struct SliceKey<'a> {
    data: &'a [u8],
}

impl<'a> SliceKey<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }
}

impl<'a> Key for SliceKey<'a> {
    fn at(&self, pos: usize) -> u8 {
        self.data[pos]
    }

    fn length(&self) -> usize {
        self.data.len()
    }

    fn partial_after(&self, pos: usize) -> &'a [u8] {
        &self.data[pos..]
    }
}

// Owns variable sized key data. Used especially for strings where a null-termination is required.
#[derive(Clone)]
pub struct VectorKey {
    data: Vec<u8>,
}

impl VectorKey {
    pub fn from_string(s: &String) -> Self {
        let mut data = Vec::with_capacity(s.len() + 1);
        data.extend_from_slice(s.as_bytes());
        data.push(0);
        Self { data }
    }

    pub fn from_str(s: &str) -> Self {
        let mut data = Vec::with_capacity(s.len() + 1);
        data.extend_from_slice(s.as_bytes());
        data.push(0);
        Self { data }
    }

    pub fn from_slice(data: &[u8]) -> Self {
        let data = Vec::from(data);
        Self { data }
    }

    pub fn from(data: Vec<u8>) -> Self {
        Self { data }
    }

    pub fn from_unsigned<T: Unsigned + ToBytes>(un: T) -> Self {
        Self::from_slice(un.to_be_bytes().as_ref())
    }
}

impl Key for VectorKey {
    fn at(&self, pos: usize) -> u8 {
        self.data[pos]
    }

    fn length(&self) -> usize {
        self.data.len()
    }

    fn partial_after(&self, pos: usize) -> &[u8] {
        &self.data[pos..]
    }
}

impl From<u8> for VectorKey {
    fn from(data: u8) -> Self {
        Self::from_unsigned(data)
    }
}

impl From<u16> for VectorKey {
    fn from(data: u16) -> Self {
        Self::from_unsigned(data)
    }
}

impl From<u32> for VectorKey {
    fn from(data: u32) -> Self {
        Self::from_unsigned(data)
    }
}

impl From<u64> for VectorKey {
    fn from(data: u64) -> Self {
        Self::from_unsigned(data)
    }
}

impl From<u128> for VectorKey {
    fn from(data: u128) -> Self {
        Self::from_unsigned(data)
    }
}

impl From<usize> for VectorKey {
    fn from(data: usize) -> Self {
        Self::from_unsigned(data)
    }
}

impl From<&str> for VectorKey {
    fn from(data: &str) -> Self {
        Self::from_str(data)
    }
}

impl From<String> for VectorKey {
    fn from(data: String) -> Self {
        Self::from_string(&data)
    }
}
impl From<&String> for VectorKey {
    fn from(data: &String) -> Self {
        Self::from_string(data)
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
        VectorKey::from_unsigned(j)
    }
}

impl From<i16> for VectorKey {
    fn from(val: i16) -> Self {
        let v: u16 = unsafe { mem::transmute(val) };
        let xor = 1 << 15;
        let i = (v ^ xor) & xor;
        let j = i | (v & (u16::MAX >> 1));
        VectorKey::from_unsigned(j)
    }
}

impl From<i32> for VectorKey {
    fn from(val: i32) -> Self {
        let v: u32 = unsafe { mem::transmute(val) };
        let xor = 1 << 31;
        let i = (v ^ xor) & xor;
        let j = i | (v & (u32::MAX >> 1));
        VectorKey::from_unsigned(j)
    }
}
impl From<i64> for VectorKey {
    fn from(val: i64) -> Self {
        let v: u64 = unsafe { mem::transmute(val) };
        let xor = 1 << 63;
        let i = (v ^ xor) & xor;
        let j = i | (v & (u64::MAX >> 1));
        VectorKey::from_unsigned(j)
    }
}
impl From<i128> for VectorKey {
    fn from(val: i128) -> Self {
        let v: u128 = unsafe { mem::transmute(val) };
        let xor = 1 << 127;
        let i = (v ^ xor) & xor;
        let j = i | (v & (u128::MAX >> 1));
        VectorKey::from_unsigned(j)
    }
}

impl From<isize> for VectorKey {
    fn from(val: isize) -> Self {
        let v: usize = unsafe { mem::transmute(val) };
        let xor = 1 << 63;
        let i = (v ^ xor) & xor;
        let j = i | (v & (usize::MAX >> 1));
        VectorKey::from_unsigned(j)
    }
}

#[derive(Clone, Copy)]
pub struct ArrayKey<const N: usize> {
    data: [u8; N],
    len: usize,
}

impl<const N: usize> ArrayKey<N> {
    pub fn from_slice(data: &[u8]) -> Self {
        assert!(data.len() <= N, "data length is greater than array length");
        let mut arr = [0; N];
        arr[0..data.len()].copy_from_slice(data);
        Self {
            data: arr,
            len: data.len(),
        }
    }

    pub fn from_unsigned<T: Unsigned + ToBytes>(un: T) -> Self {
        Self::from_slice(un.to_be_bytes().as_ref())
    }

    pub fn from_str(s: &str) -> Self {
        assert!(s.len() < N, "data length is greater than array length");
        let mut arr = [0; N];
        arr[..s.len()].copy_from_slice(s.as_bytes());
        Self {
            data: arr,
            len: s.len() + 1,
        }
    }

    pub fn from_string(s: &String) -> Self {
        assert!(s.len() < N, "data length is greater than array length");
        let mut arr = [0; N];
        arr[..s.len()].copy_from_slice(s.as_bytes());
        Self {
            data: arr,
            len: s.len() + 1,
        }
    }

    pub fn from_array<const S: usize>(arr: [u8; S]) -> Self {
        Self::from_slice(&arr)
    }
}

impl<const N: usize> Key for ArrayKey<N> {
    fn at(&self, pos: usize) -> u8 {
        self.data[pos]
    }

    fn length(&self) -> usize {
        self.len
    }

    fn partial_after(&self, pos: usize) -> &[u8] {
        &self.data[pos..self.len]
    }

    fn as_slice(&self) -> &[u8] {
        &self.data[..self.len]
    }
}

impl<const N: usize> From<u8> for ArrayKey<N> {
    fn from(data: u8) -> Self {
        Self::from_unsigned(data)
    }
}

impl<const N: usize> From<u16> for ArrayKey<N> {
    fn from(data: u16) -> Self {
        Self::from_unsigned(data)
    }
}

impl<const N: usize> From<u32> for ArrayKey<N> {
    fn from(data: u32) -> Self {
        Self::from_unsigned(data)
    }
}

impl<const N: usize> From<u64> for ArrayKey<N> {
    fn from(data: u64) -> Self {
        Self::from_unsigned(data)
    }
}

impl<const N: usize> From<u128> for ArrayKey<N> {
    fn from(data: u128) -> Self {
        Self::from_unsigned(data)
    }
}

impl<const N: usize> From<usize> for ArrayKey<N> {
    fn from(data: usize) -> Self {
        Self::from_unsigned(data)
    }
}

impl<const N: usize> From<&str> for ArrayKey<N> {
    fn from(data: &str) -> Self {
        Self::from_str(data)
    }
}

impl<const N: usize> From<String> for ArrayKey<N> {
    fn from(data: String) -> Self {
        Self::from_string(&data)
    }
}
impl<const N: usize> From<&String> for ArrayKey<N> {
    fn from(data: &String) -> Self {
        Self::from_string(data)
    }
}

impl<const N: usize> From<i8> for ArrayKey<N> {
    fn from(val: i8) -> Self {
        let v: u8 = unsafe { mem::transmute(val) };
        let i = (v ^ 0x80) & 0x80;
        let j = i | (v & 0x7F);
        ArrayKey::from_unsigned(j)
    }
}

impl<const N: usize> From<i16> for ArrayKey<N> {
    fn from(val: i16) -> Self {
        let v: u16 = unsafe { mem::transmute(val) };
        let xor = 1 << 15;
        let i = (v ^ xor) & xor;
        let j = i | (v & (u16::MAX >> 1));
        ArrayKey::from_unsigned(j)
    }
}

impl<const N: usize> From<i32> for ArrayKey<N> {
    fn from(val: i32) -> Self {
        let v: u32 = unsafe { mem::transmute(val) };
        let xor = 1 << 31;
        let i = (v ^ xor) & xor;
        let j = i | (v & (u32::MAX >> 1));
        ArrayKey::from_unsigned(j)
    }
}
impl<const N: usize> From<i64> for ArrayKey<N> {
    fn from(val: i64) -> Self {
        let v: u64 = unsafe { mem::transmute(val) };
        let xor = 1 << 63;
        let i = (v ^ xor) & xor;
        let j = i | (v & (u64::MAX >> 1));
        ArrayKey::from_unsigned(j)
    }
}
impl<const N: usize> From<i128> for ArrayKey<N> {
    fn from(val: i128) -> Self {
        let v: u128 = unsafe { mem::transmute(val) };
        let xor = 1 << 127;
        let i = (v ^ xor) & xor;
        let j = i | (v & (u128::MAX >> 1));
        ArrayKey::from_unsigned(j)
    }
}

impl<const N: usize> From<isize> for ArrayKey<N> {
    fn from(val: isize) -> Self {
        let v: usize = unsafe { mem::transmute(val) };
        let xor = 1 << 63;
        let i = (v ^ xor) & xor;
        let j = i | (v & (usize::MAX >> 1));
        ArrayKey::from_unsigned(j)
    }
}
