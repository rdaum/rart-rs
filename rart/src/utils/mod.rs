pub mod bitarray;
pub mod bitset;
pub mod u8_keys;

/// Copy `src` into the front of `dst`, which must be at least as long.
///
/// Key and prefix fragments are almost always 16 bytes or less. For those lengths this uses
/// a pair of fixed-size, possibly overlapping moves that the compiler turns into plain loads
/// and stores, instead of the out-of-line `memcpy` call a variable-length slice copy becomes.
#[inline(always)]
pub(crate) fn copy_short(dst: &mut [u8], src: &[u8]) {
    let len = src.len();
    let dst = &mut dst[..len];
    match len {
        0 => {}
        1..=3 => {
            // First, middle and last byte cover lengths 1, 2 and 3.
            let (first, middle, last) = (src[0], src[len / 2], src[len - 1]);
            dst[0] = first;
            dst[len / 2] = middle;
            dst[len - 1] = last;
        }
        4..=7 => {
            let head: [u8; 4] = src[..4].try_into().unwrap();
            let tail: [u8; 4] = src[len - 4..].try_into().unwrap();
            dst[..4].copy_from_slice(&head);
            dst[len - 4..].copy_from_slice(&tail);
        }
        8..=16 => {
            let head: [u8; 8] = src[..8].try_into().unwrap();
            let tail: [u8; 8] = src[len - 8..].try_into().unwrap();
            dst[..8].copy_from_slice(&head);
            dst[len - 8..].copy_from_slice(&tail);
        }
        _ => dst.copy_from_slice(src),
    }
}

/// Pack up to 16 bytes into a `u128` whose little-endian bytes are `src` followed by zeros.
///
/// The value is assembled in registers from at most two fixed-size loads. Building a small
/// array this way, rather than zeroing it and writing fragments into it, avoids reading back
/// a wide value from memory that was just written in narrower pieces, which stalls
/// store-to-load forwarding on every call.
#[inline(always)]
pub(crate) fn pack_short_le(src: &[u8]) -> u128 {
    let len = src.len();
    debug_assert!(len <= 16);
    match len {
        0 => 0,
        1..=3 => {
            // First, middle and last byte cover lengths 1, 2 and 3; overlapping positions
            // hold the same byte, so OR-ing them is harmless.
            (src[0] as u128)
                | ((src[len / 2] as u128) << (8 * (len / 2)))
                | ((src[len - 1] as u128) << (8 * (len - 1)))
        }
        4..=7 => {
            let head = u32::from_le_bytes(src[..4].try_into().unwrap()) as u128;
            let tail = u32::from_le_bytes(src[len - 4..].try_into().unwrap()) as u128;
            head | (tail << (8 * (len - 4)))
        }
        _ => {
            let head = u64::from_le_bytes(src[..8].try_into().unwrap()) as u128;
            let tail = u64::from_le_bytes(src[len - 8..len].try_into().unwrap()) as u128;
            head | (tail << (8 * (len - 8)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{copy_short, pack_short_le};

    #[test]
    fn pack_short_le_matches_zero_padded_bytes_for_every_length() {
        let src: Vec<u8> = (1..=16).collect();
        for len in 0..=16 {
            let mut expected = [0u8; 16];
            expected[..len].copy_from_slice(&src[..len]);
            assert_eq!(
                pack_short_le(&src[..len]).to_le_bytes(),
                expected,
                "len {len}"
            );
        }
    }

    #[test]
    fn copy_short_matches_slice_copy_for_every_length() {
        let src: Vec<u8> = (1..=40).collect();
        for len in 0..=src.len() {
            let mut dst = [0xAAu8; 48];
            copy_short(&mut dst, &src[..len]);
            assert_eq!(&dst[..len], &src[..len], "len {len}");
            assert!(dst[len..].iter().all(|byte| *byte == 0xAA), "len {len}");
        }
    }
}
