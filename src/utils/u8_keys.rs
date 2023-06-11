#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
#[inline]
fn x86_64_sse_seek_insert_pos(key: u8, keys: &[u8], num_children: usize) -> Option<usize> {
    use std::arch::x86_64::{
        __m128i, _mm_cmplt_epi8, _mm_loadu_si128, _mm_movemask_epi8, _mm_set1_epi8,
    };

    let bitfield = unsafe {
        let key_vec = _mm_set1_epi8(key as i8);
        let cmp = _mm_cmplt_epi8(key_vec, _mm_loadu_si128(keys.as_ptr() as *const __m128i));
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
fn x86_64_sse_find_key(key: u8, keys: &[u8], num_children: usize) -> Option<usize> {
    use std::arch::x86_64::{
        __m128i, _mm_cmpeq_epi8, _mm_loadu_si128, _mm_movemask_epi8, _mm_set1_epi8,
    };

    let bitfield = unsafe {
        let key_vec = _mm_set1_epi8(key as i8);
        let results = _mm_cmpeq_epi8(key_vec, _mm_loadu_si128(keys.as_ptr() as *const __m128i));
        let mask = (1 << num_children) - 1;
        _mm_movemask_epi8(results) & mask
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
        if keys[mid] == key {
            return Some(mid);
        } else if keys[mid] < key {
            left = mid + 1;
        } else {
            right = mid;
        }
    }
    None
}

fn bit_floor(n: usize) -> usize {
    let mut n = n;
    n |= n >> 1;
    n |= n >> 2;
    n |= n >> 4;
    n |= n >> 8;
    n |= n >> 16;
    n
}

fn bit_ceil(n: usize) -> usize {
    bit_floor(n - 1) + 1
}

pub fn u8_keys_find_key_position<const WIDTH: usize>(
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
            return x86_64_sse_find_key(key, keys, num_children);
        }

        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        {
            return aarch64_neon_find_key(key, &keys, num_children as usize);
        }

        // Fallback to binary search.
        binary_find_key(key, keys, num_children)
    } else {
        // Linear search seems to outperform binary search despite the array being sorted.
        // This is probably because the array is so small.
        (0..num_children).find(|&i| keys[i] == key)
    }
}

pub fn u8_keys_find_insert_position<const WIDTH: usize>(
    key: u8,
    keys: &[u8],
    num_children: usize,
) -> Option<usize> {
    let idx = if WIDTH == 16 {
        #[cfg(all(
            any(target_arch = "x86", target_arch = "x86_64"),
            target_feature = "sse2"
        ))]
        {
            x86_64_sse_seek_insert_pos(key, keys, num_children)
        }
        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        {
            aarch64_neon_seek_insert_pos(key, &keys, num_children as usize)
        }
    } else {
        // Fallback: use linear search to find the insertion point.
        (0..num_children).rev().find(|&i| key < keys[i])
    };
    idx.or(Some(num_children))
}
