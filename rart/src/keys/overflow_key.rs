use std::cmp::Ordering;
use std::fmt;

use crate::keys::KeyTrait;
use crate::partials::Partial;
use crate::partials::overflow_partial::OverflowPartial;

/// A variable-size key type that stores short keys inline and spills long keys to boxed storage.
///
/// `OverflowKey` is intended for workloads where key lengths are not statically bounded, but most
/// keys are short enough that avoiding a heap allocation is valuable. Bytes are always exposed as a
/// contiguous slice: keys of length `<= KEY_INLINE` live in the inline array, while longer keys store
/// the full byte sequence in a `Box<[u8]>`.
///
/// The second const parameter controls the inline capacity of the associated
/// [`OverflowPartial`]. This is separate from key inline capacity because ART node prefixes have
/// different cache-density tradeoffs than full lookup keys. A useful starting point for mixed
/// dynamic-key workloads is `OverflowKey<32, 8>`.
///
/// ## Tradeoffs
///
/// - Compared with [`VectorKey`](super::vector_key::VectorKey), this can reduce allocation cost
///   for construction, insertion, and owned-key iteration when keys mostly fit inline.
/// - `VectorKey` can still be faster for lookup-heavy workloads, especially when most keys are long.
/// - [`ArrayKey`](super::array_key::ArrayKey) remains the simplest and fastest choice when key
///   length is statically bounded.
///
/// ## Examples
///
/// ```rust
/// use rart::{AdaptiveRadixTree, OverflowKey};
///
/// type Key = OverflowKey<32, 8>;
///
/// let mut tree = AdaptiveRadixTree::<Key, usize>::new();
/// tree.insert("alpha", 1);
/// tree.insert("alphabet", 2);
///
/// assert_eq!(tree.get("alpha"), Some(&1));
/// assert_eq!(tree.get("alphabet"), Some(&2));
/// ```
#[derive(Clone)]
pub struct OverflowKey<const N: usize, const P: usize = N> {
    inline: [u8; N],
    len: usize,
    overflow: Option<Box<[u8]>>,
}

impl<const N: usize, const P: usize> OverflowKey<N, P> {
    #[inline(always)]
    fn heap_data(&self) -> &[u8] {
        self.overflow
            .as_deref()
            .expect("overflow storage must exist when len exceeds inline capacity")
    }

    #[inline(always)]
    fn data(&self) -> &[u8] {
        if self.len <= N {
            &self.inline[..self.len]
        } else {
            self.heap_data()
        }
    }

    pub fn new_from_str(s: &str) -> Self {
        let mut data = Vec::with_capacity(s.len() + 1);
        data.extend_from_slice(s.as_bytes());
        data.push(0);
        Self::new_from_slice(&data)
    }

    pub fn new_from_string(s: &str) -> Self {
        Self::new_from_str(s)
    }

    pub fn new_from_array<const S: usize>(arr: [u8; S]) -> Self {
        Self::new_from_slice(&arr)
    }

    pub fn as_slice(&self) -> &[u8] {
        self.data()
    }

    pub fn is_inline(&self) -> bool {
        self.len <= N
    }

    pub fn to_be_u64(&self) -> u64 {
        debug_assert!(self.len <= 8, "data length is greater than 8 bytes");
        let mut arr = [0; 8];
        arr[8 - self.len..].copy_from_slice(self.data());
        u64::from_be_bytes(arr)
    }
}

impl<const N: usize, const P: usize> AsRef<[u8]> for OverflowKey<N, P> {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        self.data()
    }
}

impl<const N: usize, const P: usize> fmt::Debug for OverflowKey<N, P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OverflowKey")
            .field("data", &self.as_ref())
            .field("inline", &self.is_inline())
            .finish()
    }
}

impl<const N: usize, const P: usize> PartialEq for OverflowKey<N, P> {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl<const N: usize, const P: usize> Eq for OverflowKey<N, P> {}

impl<const N: usize, const P: usize> PartialOrd for OverflowKey<N, P> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<const N: usize, const P: usize> Ord for OverflowKey<N, P> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_ref().cmp(other.as_ref())
    }
}

impl<const N: usize, const P: usize> KeyTrait for OverflowKey<N, P> {
    type PartialType = OverflowPartial<P>;
    const MAXIMUM_SIZE: Option<usize> = None;

    fn new_from_slice(data: &[u8]) -> Self {
        if data.len() <= N {
            let mut inline = [0; N];
            inline[..data.len()].copy_from_slice(data);
            Self {
                inline,
                len: data.len(),
                overflow: None,
            }
        } else {
            Self {
                inline: [0; N],
                len: data.len(),
                overflow: Some(Box::from(data)),
            }
        }
    }

    fn new_from_partial(partial: &Self::PartialType) -> Self {
        Self::new_from_slice(partial.to_slice())
    }

    fn extend_from_partial(&self, partial: &Self::PartialType) -> Self {
        let mut data = Vec::with_capacity(self.len + partial.len());
        data.extend_from_slice(self.data());
        data.extend_from_slice(partial.to_slice());
        Self::new_from_slice(&data)
    }

    fn truncate(&self, at_depth: usize) -> Self {
        debug_assert!(at_depth <= self.len, "truncating beyond key length");
        Self::new_from_slice(&self.data()[..at_depth])
    }

    #[inline(always)]
    fn at(&self, pos: usize) -> u8 {
        if self.len <= N {
            self.inline[pos]
        } else {
            self.heap_data()[pos]
        }
    }

    #[inline(always)]
    fn length_at(&self, at_depth: usize) -> usize {
        self.len - at_depth
    }

    fn to_partial(&self, at_depth: usize) -> OverflowPartial<P> {
        OverflowPartial::from_slice(&self.data()[at_depth..])
    }

    #[inline(always)]
    fn matches_slice(&self, slice: &[u8]) -> bool {
        self.data() == slice
    }
}

impl<const N: usize, const P: usize> From<String> for OverflowKey<N, P> {
    fn from(data: String) -> Self {
        Self::new_from_string(data.as_str())
    }
}

impl<const N: usize, const P: usize> From<&String> for OverflowKey<N, P> {
    fn from(data: &String) -> Self {
        Self::new_from_string(data.as_str())
    }
}

impl<const N: usize, const P: usize> From<&str> for OverflowKey<N, P> {
    fn from(data: &str) -> Self {
        Self::new_from_str(data)
    }
}

macro_rules! impl_from_unsigned {
    ( $($t:ty),* ) => {
    $(
    impl<const N: usize, const P: usize> From< $t > for OverflowKey<N, P>
    {
        fn from(data: $t) -> Self {
            Self::new_from_slice(data.to_be_bytes().as_ref())
        }
    }
    impl<const N: usize, const P: usize> From< &$t > for OverflowKey<N, P>
    {
        fn from(data: &$t) -> Self {
            Self::new_from_slice(data.to_be_bytes().as_ref())
        }
    }
    ) *
    }
}
impl_from_unsigned!(u8, u16, u32, u64, usize, u128);

impl<const N: usize, const P: usize> From<i8> for OverflowKey<N, P> {
    fn from(val: i8) -> Self {
        let v: u8 = val as u8;
        let j = v ^ 0x80;
        Self::new_from_slice(&[j])
    }
}

macro_rules! impl_from_signed {
    ( $t:ty, $tu:ty ) => {
        impl<const N: usize, const P: usize> From<$t> for OverflowKey<N, P> {
            fn from(val: $t) -> Self {
                let v: $tu = val as $tu;
                let sign_bit = 1 << (std::mem::size_of::<$tu>() * 8 - 1);
                let j = v ^ sign_bit;
                OverflowKey::<N, P>::new_from_slice(j.to_be_bytes().as_ref())
            }
        }

        impl<const N: usize, const P: usize> From<&$t> for OverflowKey<N, P> {
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
mod tests {
    use crate::keys::KeyTrait;
    use crate::keys::overflow_key::OverflowKey;
    use crate::partials::overflow_partial::OverflowPartial;

    #[test]
    fn inline_and_overflow_match_slices() {
        let short = OverflowKey::<4>::new_from_slice(b"abc");
        assert!(short.is_inline());
        assert!(short.matches_slice(b"abc"));

        let long = OverflowKey::<4>::new_from_slice(b"abcdef");
        assert!(!long.is_inline());
        assert!(long.matches_slice(b"abcdef"));
    }

    #[test]
    fn ordering_ignores_unused_storage() {
        let a = OverflowKey::<4>::new_from_slice(b"abcd");
        let b = OverflowKey::<4>::new_from_slice(b"abcde");
        assert!(a < b);
    }

    #[test]
    fn make_extend_truncate_cross_inline_boundary() {
        let k = OverflowKey::<4>::new_from_slice(b"hel");
        let p = OverflowPartial::<4>::from_slice(b"lo!");
        let k2 = k.extend_from_partial(&p);
        assert!(!k2.is_inline());
        assert!(k2.matches_slice(b"hello!"));
        let k3 = k2.truncate(4);
        assert!(k3.is_inline());
        assert!(k3.matches_slice(b"hell"));
    }

    #[test]
    fn from_to_u64() {
        let k: OverflowKey<16> = 123u64.into();
        assert_eq!(k.to_be_u64(), 123u64);

        let k: OverflowKey<16> = 1u64.into();
        assert_eq!(k.to_be_u64(), 1u64);

        let k: OverflowKey<16> = 123213123123123u64.into();
        assert_eq!(k.to_be_u64(), 123213123123123u64);
    }
}
