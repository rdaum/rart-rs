use crate::keys::KeyTrait;
use crate::partials::vector_partial::VectorPartial;

// Owns variable sized key data. Used especially for strings where a null-termination is required.
#[derive(Clone, Eq, PartialEq, PartialOrd, Ord)]
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

    pub fn new_from_vec(data: Vec<u8>) -> Self {
        Self {
            data: data.into_boxed_slice(),
        }
    }

    pub fn to_be_u64(&self) -> u64 {
        // Value must be at least 8 bytes long.
        assert!(self.data.len() >= 8, "data length is less than 8 bytes");
        // Copy from 0..min(len, 8) to a new array left-padding it, then convert to u64.
        let mut arr = [0; 8];
        arr[8 - self.data.len()..].copy_from_slice(&self.data[..self.data.len()]);
        u64::from_be_bytes(arr)
    }
}

impl KeyTrait for VectorKey {
    type PartialType = VectorPartial;
    const MAXIMUM_SIZE: Option<usize> = None;

    fn extend_from_partial(&self, partial: &Self::PartialType) -> Self {
        let mut v = self.data.to_vec();
        v.extend_from_slice(partial.to_slice());
        Self {
            data: v.into_boxed_slice(),
        }
    }

    fn truncate(&self, at_depth: usize) -> Self {
        let mut v = self.data.to_vec();
        v.truncate(at_depth);
        Self {
            data: v.into_boxed_slice(),
        }
    }

    fn new_from_slice(data: &[u8]) -> Self {
        let data = Vec::from(data);
        Self {
            data: data.into_boxed_slice(),
        }
    }
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

    fn new_from_partial(partial: &Self::PartialType) -> Self {
        let data = Vec::from(partial.to_slice());
        Self {
            data: data.into_boxed_slice(),
        }
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
        let v: u8 = val.cast_unsigned();
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
                let v: $tu = val.cast_unsigned();
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

#[cfg(test)]
mod test {
    use crate::keys::KeyTrait;
    use crate::keys::vector_key::VectorKey;
    use crate::partials::vector_partial::VectorPartial;

    #[test]
    fn make_extend_truncate() {
        let k = VectorKey::new_from_slice(b"hel");
        let p = VectorPartial::from_slice(b"lo");
        let k2 = k.extend_from_partial(&p);
        assert!(k2.matches_slice(b"hello"));
        let k3 = k2.truncate(3);
        assert!(k3.matches_slice(b"hel"));
    }

    #[test]
    fn from_to_u64() {
        let k: VectorKey = 123u64.into();
        assert_eq!(k.to_be_u64(), 123u64);

        let k: VectorKey = 1u64.into();
        assert_eq!(k.to_be_u64(), 1u64);

        let k: VectorKey = 123213123123123u64.into();
        assert_eq!(k.to_be_u64(), 123213123123123u64);
    }
}
