use std::mem;

use crate::keys::KeyTrait;
use crate::partials::array_partial::ArrPartial;

#[derive(Clone, Copy)]
pub struct ArrayKey<const N: usize> {
    data: [u8; N],
    len: usize,
}

impl<const N: usize> ArrayKey<N> {
    pub fn new_from_slice(data: &[u8]) -> Self {
        assert!(data.len() <= N, "data length is greater than array length");
        let mut arr = [0; N];
        arr[0..data.len()].copy_from_slice(data);
        Self {
            data: arr,
            len: data.len(),
        }
    }

    pub fn new_from_str(s: &str) -> Self {
        assert!(s.len() + 1 < N, "data length is greater than array length");
        let mut arr = [0; N];
        arr[..s.len()].copy_from_slice(s.as_bytes());
        Self {
            data: arr,
            len: s.len() + 1,
        }
    }

    pub fn new_from_string(s: &String) -> Self {
        assert!(s.len() + 1 < N, "data length is greater than array length");
        let mut arr = [0; N];
        arr[..s.len()].copy_from_slice(s.as_bytes());
        Self {
            data: arr,
            len: s.len() + 1,
        }
    }

    pub fn new_from_array<const S: usize>(arr: [u8; S]) -> Self {
        Self::new_from_slice(&arr)
    }
}

impl<const N: usize> KeyTrait for ArrayKey<N> {
    type PartialType = ArrPartial<N>;
    const MAXIMUM_SIZE: Option<usize> = Some(N);

    #[inline(always)]
    fn at(&self, pos: usize) -> u8 {
        self.data[pos]
    }
    #[inline(always)]
    fn length_at(&self, at_depth: usize) -> usize {
        self.len - at_depth
    }
    fn to_partial(&self, at_depth: usize) -> ArrPartial<N> {
        ArrPartial::from_slice(&self.data[at_depth..self.len])
    }
    #[inline(always)]
    fn matches_slice(&self, slice: &[u8]) -> bool {
        &self.data[..self.len] == slice
    }
}

impl<const N: usize> From<String> for ArrayKey<N> {
    fn from(data: String) -> Self {
        Self::new_from_string(&data)
    }
}
impl<const N: usize> From<&String> for ArrayKey<N> {
    fn from(data: &String) -> Self {
        Self::new_from_string(data)
    }
}
impl<const N: usize> From<&str> for ArrayKey<N> {
    fn from(data: &str) -> Self {
        Self::new_from_str(data)
    }
}
macro_rules! impl_from_unsigned {
    ( $($t:ty),* ) => {
    $(
    impl<const N: usize> From< $t > for ArrayKey<N>
    {
        fn from(data: $t) -> Self {
            Self::new_from_slice(data.to_be_bytes().as_ref())
        }
    }
    impl<const N: usize> From< &$t > for ArrayKey<N>
    {
        fn from(data: &$t) -> Self {
            Self::new_from_slice(data.to_be_bytes().as_ref())
        }
    }
    ) *
    }
}
impl_from_unsigned!(u8, u16, u32, u64, usize, u128);

impl<const N: usize> From<i8> for ArrayKey<N> {
    fn from(val: i8) -> Self {
        let v: u8 = unsafe { mem::transmute(val) };
        let i = (v ^ 0x80) & 0x80;
        let j = i | (v & 0x7F);
        let mut data = [0; N];
        data[0] = j;
        Self { data, len: 1 }
    }
}

macro_rules! impl_from_signed {
    ( $t:ty, $tu:ty ) => {
        impl<const N: usize> From<$t> for ArrayKey<N> {
            fn from(val: $t) -> Self {
                let v: $tu = unsafe { mem::transmute(val) };
                let xor = 1 << (std::mem::size_of::<$tu>() - 1);
                let i = (v ^ xor) & xor;
                let j = i | (v & (<$tu>::MAX >> 1));
                ArrayKey::new_from_slice(j.to_be_bytes().as_ref())
            }
        }

        impl<const N: usize> From<&$t> for ArrayKey<N> {
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
