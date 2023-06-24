use crate::utils::bitset::BitsetTrait;

#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
#[inline]
fn x86_64_sse_seek_insert_pos_16(key: u8, keys: [u8; 16], num_children: usize) -> Option<usize> {
    use std::arch::x86_64::{
        __m128i, _mm_cmplt_epi8, _mm_loadu_si128, _mm_movemask_epi8, _mm_set1_epi8,
    };

    let bitfield = unsafe {
        let cmp_vec = _mm_set1_epi8(key as i8);
        let cmp = _mm_cmplt_epi8(cmp_vec, _mm_loadu_si128(keys.as_ptr() as *const __m128i));
        let mask = (1 << num_children) - 1;
        _mm_movemask_epi8(cmp) & mask
    };

    if bitfield != 0 {
        let idx = bitfield.trailing_zeros() as usize;
        return Some(idx);
    }
    None
}

#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
#[inline]
fn x86_64_sse_find_key_16_up_to(key: u8, keys: [u8; 16], num_children: usize) -> Option<usize> {
    use std::arch::x86_64::{
        __m128i, _mm_cmpeq_epi8, _mm_loadu_si128, _mm_movemask_epi8, _mm_set1_epi8,
    };

    let bitfield = unsafe {
        let key_vec = _mm_set1_epi8(key as i8);
        let results = _mm_cmpeq_epi8(key_vec, _mm_loadu_si128(keys.as_ptr() as *const __m128i));
        // AVX512 has _mm_cmpeq_epi8_mask which can allow us to skip this step and go direct to a
        // bitmask from comparison results.
        // ... but that's stdsimd nightly only for now, and also not available on all processors.
        let mask = (1 << num_children) - 1;
        _mm_movemask_epi8(results) & mask
    };
    if bitfield != 0 {
        let idx = bitfield.trailing_zeros() as usize;
        return Some(idx);
    }
    None
}

#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
#[inline]
fn x86_64_sse_find_key_16(key: u8, keys: [u8; 16], bitmask: u16) -> Option<usize> {
    use std::arch::x86_64::{
        __m128i, _mm_cmpeq_epi8, _mm_loadu_si128, _mm_movemask_epi8, _mm_set1_epi8,
    };

    let bitfield = unsafe {
        let key_vec = _mm_set1_epi8(key as i8);
        let results = _mm_cmpeq_epi8(key_vec, _mm_loadu_si128(keys.as_ptr() as *const __m128i));
        // AVX512 has _mm_cmpeq_epi8_mask which can allow us to skip this step and go direct to a
        // bitmask from comparison results.
        // ... but that's stdsimd nightly only for now, and also not available on all processors.
        _mm_movemask_epi8(results) & bitmask as i32
    };
    if bitfield != 0 {
        let idx = bitfield.trailing_zeros() as usize;
        return Some(idx);
    }
    None
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn x86_64_sse_find_key_32_up_to(
    key: u8,
    keys: [u8; 32],
    num_children: usize,
) -> Option<usize> {
    use std::arch::x86_64::{
        __m256i, _mm256_cmpeq_epi8, _mm256_loadu_si256, _mm256_movemask_epi8, _mm256_set1_epi8,
    };

    let bitfield = unsafe {
        let key_vec = _mm256_set1_epi8(key as i8);
        let results =
            _mm256_cmpeq_epi8(key_vec, _mm256_loadu_si256(keys.as_ptr() as *const __m256i));
        let mask: i64 = (1 << num_children) - 1;
        _mm256_movemask_epi8(results) as i64 & mask
    };

    if bitfield != 0 {
        let idx = bitfield.trailing_zeros() as usize;

        return Some(idx);
    }
    None
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn x86_64_sse_find_key_32(key: u8, keys: [u8; 32], bitmask: u32) -> Option<usize> {
    use std::arch::x86_64::{
        __m256i, _mm256_cmpeq_epi8, _mm256_loadu_si256, _mm256_movemask_epi8, _mm256_set1_epi8,
    };

    let bitfield = unsafe {
        let key_vec = _mm256_set1_epi8(key as i8);
        let results =
            _mm256_cmpeq_epi8(key_vec, _mm256_loadu_si256(keys.as_ptr() as *const __m256i));
        _mm256_movemask_epi8(results) as i64 & bitmask as i64
    };

    if bitfield != 0 {
        let idx = bitfield.trailing_zeros() as usize;

        return Some(idx);
    }
    None
}

#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
#[inline]
fn aarch64_neon_seek_insert_pos(key: u8, keys: &[u8], num_children: usize) -> Option<usize> {
    use std::arch::aarch64::*;
    unsafe {
        // Fill a vector with the key.
        let key_vec = vdupq_n_u8(key);
        // Load the keys from the node.
        let node_keys_vec = vld1q_u8(keys.as_ptr());
        // Compare the key to the keys in the node.
        // Result is a vector of 0x00 for each value that is less than the key and 0xFF
        // for everything else.
        let cmp_vec = vcltq_u8(key_vec, node_keys_vec);

        // NEON does not have mm_movemask_epi8, so to get a bitfield out, we have to do
        // some extra work.

        // We use shrn to shift the 8-bit elements down to 4-bit elements, then we
        // reinterpret the vector as a 64-bit vector, and finally we extract the first
        // 64-bit lane.
        let eq_mask = vreinterpretq_u16_u8(cmp_vec);
        let res = vshrn_n_u16::<4>(eq_mask);

        // We now have a 64-bit wide # (instead of 8 as in x86) bitfield, where each
        // vector element is 4-bits wide instead of 1.
        let matches = vget_lane_u64::<0>(vreinterpret_u64_u8(res));

        if matches != 0 {
            // So we can count trailing zeros and divide by 4...
            let tlz = matches.trailing_zeros();

            let shifted = (tlz >> 2) as usize;
            if shifted < num_children {
                return Some(shifted);
            }
        }
        None
    }
}

#[cfg(target_arch = "aarch64")]
#[inline]
fn aarch64_neon_find_key(key: u8, keys: &[u8], num_children: usize) -> Option<usize> {
    use std::arch::aarch64::*;
    unsafe {
        if num_children == 0 {
            return None;
        }

        // Fill a vector with the key.
        let key_vec = vdupq_n_u8(key);
        // Load the keys from the node.
        let node_keys_vec = vld1q_u8(keys.as_ptr());
        // Compare the key to the keys in the node.
        // Result is a vector of 0x00 for each value that is equal to the key and 0xFF
        // for everything else.
        let cmp_vec = vceqq_u8(key_vec, node_keys_vec);

        // NEON does not have mm_movemask_epi8, so to get a bitfield out, we have to do
        // some extra work.

        // We use shrn to shift the 8-bit elements down to 4-bit elements, then we
        // reinterpret the vector as a 64-bit vector, and finally we extract the first
        // 64-bit lane.
        let eq_mask = vreinterpretq_u16_u8(cmp_vec);
        let res = vshrn_n_u16::<4>(eq_mask);

        // We now have a 64-bit wide # (instead of 8 as in x86) bitfield, where each
        // vector element is 4-bits wide instead of 1.
        let matches = vget_lane_u64::<0>(vreinterpret_u64_u8(res));

        if matches != 0 {
            // So we can count trailing zeros and divide by 4...
            let tlz = matches.trailing_zeros();

            // Div by 4 (r-shift 2) gives us the index of the matching key.
            let shifted = (tlz >> 2) as usize;
            if shifted < num_children {
                return Some(shifted);
            }
        }
        None
    }
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

    // SIMD optimized forms of 16
    if WIDTH == 16 {
        #[cfg(all(
            any(target_arch = "x86", target_arch = "x86_64"),
            target_feature = "sse2"
        ))]
        {
            return x86_64_sse_find_key_16_up_to(key, keys.try_into().unwrap(), num_children);
        }

        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        {
            return aarch64_neon_find_key(key, &keys, num_children as usize);
        }
    }

    // SIMD AVX only optimized form of 32
    if WIDTH == 32 {
        #[cfg(all(
            any(target_arch = "x86", target_arch = "x86_64"),
            target_feature = "sse2"
        ))]
        {
            return unsafe {
                x86_64_sse_find_key_32_up_to(key, keys.try_into().unwrap(), num_children)
            };
        }
    }

    // Fallback to binary search.
    binary_find_key(key, keys, num_children)
}

#[allow(unreachable_code)]
pub fn u8_keys_find_key_position<const WIDTH: usize, Bitset: BitsetTrait>(
    key: u8,
    keys: &[u8],
    children_bitmask: &Bitset,
) -> Option<usize> {
    // SIMD optimized forms of 16
    if WIDTH == 16 {
        #[cfg(all(
            any(target_arch = "x86", target_arch = "x86_64"),
            target_feature = "sse2"
        ))]
        {
            // Special 0xff key is special
            let mask = if key == 255 {
                children_bitmask.as_bitmask() as u16
            } else {
                0xffff
            };
            return x86_64_sse_find_key_16(key, keys.try_into().unwrap(), mask);
        }

        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        {
            return aarch64_neon_find_key(key, &keys, num_children as usize);
        }
    }

    // SIMD optimized forms of 32
    if WIDTH == 32 {
        #[cfg(all(
            any(target_arch = "x86", target_arch = "x86_64"),
            target_feature = "sse2"
        ))]
        {
            // Special 0xff key is special
            let mask = if key == 255 {
                children_bitmask.as_bitmask() as u32
            } else {
                0xffffffff
            };
            return unsafe { x86_64_sse_find_key_32(key, keys.try_into().unwrap(), mask) };
        }
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

pub fn u8_keys_find_insert_position<const WIDTH: usize>(
    key: u8,
    keys: &[u8],
    num_children: usize,
) -> Option<usize> {
    if WIDTH == 16 {
        #[cfg(all(
            any(target_arch = "x86", target_arch = "x86_64"),
            target_feature = "sse2"
        ))]
        {
            return x86_64_sse_seek_insert_pos_16(key, keys.try_into().unwrap(), num_children)
                .or(Some(num_children));
        }

        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        {
            return aarch64_neon_seek_insert_pos(key, &keys, num_children as usize)
                .or(Some(num_children));
        }
    }

    // Fallback: use linear search to find the insertion point.
    (0..num_children)
        .rev()
        .find(|&i| key < keys[i])
        .or(Some(num_children))
}
