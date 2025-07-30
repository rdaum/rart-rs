use crate::keys::KeyTrait;
use crate::partials::Partial;
use crate::partials::array_partial::ArrPartial;

/// A fixed-size key type that stores up to N bytes on the stack.
///
/// `ArrayKey` is a stack-allocated key type that can store keys up to a compile-time
/// specified maximum size. This makes it very efficient for keys that have a known
/// maximum length, as it avoids heap allocations entirely.
///
/// ## Features
///
/// - **Stack allocated**: No heap allocations, very fast
/// - **Copy semantics**: Can be copied cheaply
/// - **Null termination**: Automatically adds null termination for string keys
/// - **Type safety**: Size is checked at compile time
///
/// ## Type Parameter
///
/// - `N`: Maximum number of bytes this key can store (including null terminator for strings)
///
/// ## Examples
///
/// ```rust
/// use rart::keys::array_key::ArrayKey;
///
/// // Create from string (adds null terminator)
/// let key1: ArrayKey<16> = "hello".into();
/// debug_assert_eq!(key1.as_ref(), b"hello\0");
///
/// // Create from numeric types
/// let key2: ArrayKey<8> = 42u32.into();
///
/// // Create from byte slices  
/// use rart::keys::KeyTrait;
/// let key3 = ArrayKey::<10>::new_from_slice(b"test");
/// ```
///
/// ## Size Considerations
///
/// Choose N based on your expected key sizes:
/// - For short strings (≤15 chars): `ArrayKey<16>`
/// - For medium strings (≤31 chars): `ArrayKey<32>`
/// - For numeric keys: `ArrayKey<8>` or `ArrayKey<16>`
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct ArrayKey<const N: usize> {
    data: [u8; N],
    len: usize,
}

impl<const N: usize> AsRef<[u8]> for ArrayKey<N> {
    fn as_ref(&self) -> &[u8] {
        &self.data[..self.len]
    }
}

impl<const N: usize> ArrayKey<N> {
    pub fn new_from_str(s: &str) -> Self {
        debug_assert!(s.len() + 1 < N, "data length is greater than array length");
        let mut arr = [0; N];
        arr[..s.len()].copy_from_slice(s.as_bytes());
        Self {
            data: arr,
            len: s.len() + 1,
        }
    }

    pub fn new_from_string(s: &String) -> Self {
        debug_assert!(s.len() + 1 < N, "data length is greater than array length");
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

    pub fn as_array(&self) -> &[u8; N] {
        &self.data
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data[..self.len]
    }

    /// (Convenience function. Not all keys can be assumed to be numeric.)
    pub fn to_be_u64(&self) -> u64 {
        // Copy from 0..min(len, 8) to a new array left-padding it, then convert to u64.
        let mut arr = [0; 8];
        arr[8 - self.len..].copy_from_slice(&self.data[..self.len]);
        u64::from_be_bytes(arr)
    }
}

impl<const N: usize> PartialOrd for ArrayKey<N> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<const N: usize> Ord for ArrayKey<N> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Compare only the used portion of the key data
        self.as_slice().cmp(other.as_slice())
    }
}

impl<const N: usize> KeyTrait for ArrayKey<N> {
    type PartialType = ArrPartial<N>;
    const MAXIMUM_SIZE: Option<usize> = Some(N);

    fn new_from_slice(data: &[u8]) -> Self {
        debug_assert!(data.len() <= N, "data length is greater than array length");
        let mut arr = [0; N];
        arr[0..data.len()].copy_from_slice(data);
        Self {
            data: arr,
            len: data.len(),
        }
    }

    fn new_from_partial(partial: &Self::PartialType) -> Self {
        let mut data = [0; N];
        let len = partial.len();
        data[..len].copy_from_slice(&partial.to_slice()[..len]);
        Self { data, len }
    }

    fn extend_from_partial(&self, partial: &Self::PartialType) -> Self {
        let cur_len = self.len;
        let partial_len = partial.len();
        debug_assert!(
            cur_len + partial_len <= N,
            "data length is greater than max key length"
        );
        let mut data = [0; N];
        data[..cur_len].copy_from_slice(&self.data[..cur_len]);
        let partial_slice = partial.to_slice();
        data[cur_len..cur_len + partial_len].copy_from_slice(&partial_slice[..partial_len]);
        Self {
            data,
            len: cur_len + partial_len,
        }
    }

    fn truncate(&self, at_depth: usize) -> Self {
        debug_assert!(at_depth <= self.len, "truncating beyond key length");
        Self {
            data: self.data,
            len: at_depth,
        }
    }

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
        // Convert signed to unsigned preserving sort order
        let v: u8 = val as u8;
        let j = v ^ 0x80; // Flip sign bit
        let mut data = [0; N];
        data[0] = j;
        Self { data, len: 1 }
    }
}

macro_rules! impl_from_signed {
    ( $t:ty, $tu:ty ) => {
        impl<const N: usize> From<$t> for ArrayKey<N> {
            fn from(val: $t) -> Self {
                // Convert signed to unsigned preserving sort order
                // Flip the sign bit to map negative numbers to 0..2^(n-1)-1
                // and positive numbers to 2^(n-1)..2^n-1
                let v: $tu = val as $tu;
                let sign_bit = 1 << (std::mem::size_of::<$tu>() * 8 - 1);
                let j = v ^ sign_bit;
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

#[cfg(test)]
mod test {
    use crate::keys::KeyTrait;
    use crate::keys::array_key::ArrayKey;
    use crate::partials::array_partial::ArrPartial;

    #[test]
    fn make_extend_truncate() {
        let k = ArrayKey::<8>::new_from_slice(b"hel");
        let p = ArrPartial::<8>::from_slice(b"lo");
        let k2 = k.extend_from_partial(&p);
        assert!(k2.matches_slice(b"hello"));
        let k3 = k2.truncate(3);
        assert!(k3.matches_slice(b"hel"));
    }

    #[test]
    fn from_to_u64() {
        let k: ArrayKey<16> = 123u64.into();
        assert_eq!(k.to_be_u64(), 123u64);

        let k: ArrayKey<16> = 1u64.into();
        assert_eq!(k.to_be_u64(), 1u64);

        let k: ArrayKey<16> = 123213123123123u64.into();
        assert_eq!(k.to_be_u64(), 123213123123123u64);
    }
}
