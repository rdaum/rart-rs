use crate::utils::bitset::BitsetTrait;

#[cfg(feature = "simd_keys")]
mod simd_keys {
    use simdeez::*;
    use simdeez::{prelude::*, simd_runtime_generate};

    simd_runtime_generate!(
        pub fn simdeez_find_insert_pos(key: u8, keys: &[u8], ff_mask_out: u32) -> Option<usize> {
            let key_cmp_vec = S::Vi8::set1(key as i8);
            let key_vec = SimdBaseIo::load_from_ptr_unaligned(keys.as_ptr() as *const i8);
            let results = key_cmp_vec.cmp_lt(key_vec);
            let bitfield = results.get_mask() & (ff_mask_out as u32);
            if bitfield != 0 {
                let idx = bitfield.trailing_zeros() as usize;
                return Some(idx);
            }
            None
        }
    );

    simd_runtime_generate!(
        pub fn simdeez_find_key(key: u8, keys: &[u8], ff_mask_out: u32) -> Option<usize> {
            let key_cmp_vec = S::Vi8::set1(key as i8);
            let key_vec = SimdBaseIo::load_from_ptr_unaligned(keys.as_ptr() as *const i8);
            let results = key_cmp_vec.cmp_eq(key_vec);
            let bitfield = results.get_mask() & (ff_mask_out as u32);
            if bitfield != 0 {
                let idx = bitfield.trailing_zeros() as usize;
                return Some(idx);
            }
            None
        }
    );
}

fn binary_find_key(key: u8, keys: &[u8], num_children: usize) -> Option<usize> {
    let mut left = 0;
    let mut right = num_children;
    while left < right {
        let mid = (left + right) / 2;
        match keys[mid].cmp(&key) {
            std::cmp::Ordering::Less => left = mid + 1,
            std::cmp::Ordering::Equal => return Some(mid),
            std::cmp::Ordering::Greater => right = mid,
        }
    }
    None
}

#[allow(unreachable_code)]
pub fn u8_keys_find_key_position_sorted<const WIDTH: usize>(
    key: u8,
    keys: &[u8],
    num_children: usize,
) -> Option<usize> {
    // Width 4 and under, just use linear search.
    if WIDTH <= 4 {
        return (0..num_children).find(|&i| keys[i] == key);
    }

    #[cfg(feature = "simd_keys")]
    if WIDTH >= 16 {
        return simd_keys::simdeez_find_key(key, keys.try_into().unwrap(), (1 << num_children) - 1);
    }

    // Fallback to binary search.
    binary_find_key(key, keys, num_children)
}

pub fn u8_keys_find_insert_position_sorted<const WIDTH: usize>(
    key: u8,
    keys: &[u8],
    num_children: usize,
) -> Option<usize> {
    #[cfg(feature = "simd_keys")]
    if WIDTH >= 16 {
        return simd_keys::simdeez_find_insert_pos(
            key,
            keys.try_into().unwrap(),
            (1 << num_children) - 1,
        );
    }

    // Fallback: use linear search to find the insertion point.
    (0..num_children)
        .rev()
        .find(|&i| key < keys[i])
        .or(Some(num_children))
}

#[allow(unreachable_code)]
pub fn u8_keys_find_key_position<const WIDTH: usize, Bitset: BitsetTrait>(
    key: u8,
    keys: &[u8],
    children_bitmask: &Bitset,
) -> Option<usize> {
    // // SIMD optimized forms of 16
    #[cfg(feature = "simd_keys")]
    if WIDTH >= 16 {
        // Special 0xff key is special
        let mut mask = (1 << WIDTH) - 1;
        if key == 255 {
            mask &= children_bitmask.as_bitmask() as u32;
        }
        return simd_keys::simdeez_find_key(key, keys.try_into().unwrap(), mask);
    }

    // Fallback to linear search for anything else (which is just WIDTH == 4, or if we have no
    // SIMD support).
    for (i, k) in keys.iter().enumerate() {
        if key == 255 && !children_bitmask.check(i) {
            continue;
        }
        if *k == key {
            return Some(i);
        }
    }
    None
}
