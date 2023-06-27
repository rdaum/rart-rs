use std::mem;

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
        Self {
            data: data.into_boxed_slice(),
        }
    }

    pub fn new_from_str(s: &str) -> Self {
        let mut data = Vec::with_capacity(s.len() + 1);
        data.extend_from_slice(s.as_bytes());
        data.push(0);
        Self {
            data: data.into_boxed_slice(),
        }
    }

    pub fn new_from_slice(data: &[u8]) -> Self {
        let data = Vec::from(data);
        Self {
            data: data.into_boxed_slice(),
        }
    }

    pub fn new_from_vec(data: Vec<u8>) -> Self {
        Self {
            data: data.into_boxed_slice(),
        }
    }
}

impl KeyTrait for VectorKey {
    type PartialType = VectorPartial;
    const MAXIMUM_SIZE: Option<usize> = None;

    fn at(&self, pos: usize) -> u8 {
        self.data[pos]
    }

    fn length_at(&self, at_depth: usize) -> usize {
        self.data.len() - at_depth
    }

    fn to_partial(&self, at_depth: usize) -> VectorPartial {
        VectorPartial::from_slice(&self.data[at_depth..])
    }

    fn matches_slice(&self, slice: &[u8]) -> bool {
        self.data.len() == slice.len() && &self.data[..] == slice
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
impl From<&str> for VectorKey {
    fn from(data: &str) -> Self {
        Self::new_from_str(data)
    }
}
macro_rules! impl_from_unsigned {
    ( $($t:ty),* ) => {
    $(
    impl From< $t > for VectorKey
    {
        fn from(data: $t) -> Self {
            VectorKey::new_from_slice(&data.to_be_bytes())
        }
    }
    impl From< &$t > for VectorKey
    {
        fn from(data: &$t) -> Self {
            (*data).into()
        }
    }
    ) *
    }
}
impl_from_unsigned!(u8, u16, u32, u64, usize, u128);

impl From<i8> for VectorKey {
    fn from(val: i8) -> Self {
        let v: u8 = unsafe { mem::transmute(val) };
        let i = (v ^ 0x80) & 0x80;
        let j = i | (v & 0x7F);
        let v = vec![j];
        VectorKey::new_from_vec(v)
    }
}

macro_rules! impl_from_signed {
    ( $t:ty, $tu:ty ) => {
        impl From<$t> for VectorKey {
            fn from(val: $t) -> Self {
                let v: $tu = unsafe { mem::transmute(val) };
                let xor = 1 << (std::mem::size_of::<$tu>() - 1);
                let i = (v ^ xor) & xor;
                let j = i | (v & (<$tu>::MAX >> 1));
                VectorKey::new_from_slice(&j.to_be_bytes())
            }
        }

        impl From<&$t> for VectorKey {
            fn from(val: &$t) -> Self {
                (*val).into()
            }
        }
    };
}

impl_from_signed!(i16, u16);
impl_from_signed!(i32, u32);
impl_from_signed!(i64, u64);
impl_from_signed!(i128, u128);
impl_from_signed!(isize, usize);
