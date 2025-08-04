//! MultilevelNode4 - A node that spans multiple key bytes while maintaining 4 children max
//! Reduces tree height by handling longer key sequences in a single node

use std::mem::MaybeUninit;
use crate::mapping::NodeMapping;

/// A multilevel node that can store up to 4 children with keys up to 4 bytes each.
/// This allows spanning multiple levels of the radix tree while staying within one cache line.
pub struct MultilevelNode4<N> {
    /// Keys for each child, up to 4 bytes each
    keys: [[u8; 4]; 4],
    /// Actual length of each key (1-4 bytes)
    key_lengths: [u8; 4],
    /// Child nodes (boxed to match other mapping patterns)
    children: Box<[MaybeUninit<N>; 4]>,
    /// Number of children currently stored
    num_children: u8,
}

impl<N> Default for MultilevelNode4<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<N> MultilevelNode4<N> {
    #[inline]
    pub fn new() -> Self {
        Self {
            keys: [[0; 4]; 4],
            key_lengths: [0; 4],
            children: Box::new([const { MaybeUninit::uninit() }; 4]),
            num_children: 0,
        }
    }

    /// Add a child with a multilevel key
    pub fn add_multilevel_child(&mut self, key_bytes: &[u8], child: N) -> Result<(), &'static str> {
        if self.num_children >= 4 {
            return Err("MultilevelNode4 is full");
        }
        if key_bytes.is_empty() || key_bytes.len() > 4 {
            return Err("Key must be 1-4 bytes");
        }

        let idx = self.num_children as usize;
        
        // Copy key bytes
        for (i, &byte) in key_bytes.iter().enumerate() {
            self.keys[idx][i] = byte;
        }
        // Zero out remaining bytes for consistent comparison
        for i in key_bytes.len()..4 {
            self.keys[idx][i] = 0;
        }
        
        self.key_lengths[idx] = key_bytes.len() as u8;
        self.children[idx].write(child);
        self.num_children += 1;
        
        Ok(())
    }

    /// Seek a child by multilevel key
    /// Finds the longest matching key (most specific match first)
    pub fn seek_multilevel_child(&self, key_bytes: &[u8]) -> Option<&N> {
        if key_bytes.is_empty() {
            return None;
        }

        let mut best_match = None;
        let mut best_key_len = 0;

        for i in 0..self.num_children as usize {
            let key_len = self.key_lengths[i] as usize;
            if key_bytes.len() >= key_len && 
               &key_bytes[..key_len] == &self.keys[i][..key_len] &&
               key_len > best_key_len {
                best_match = Some(unsafe { self.children[i].assume_init_ref() });
                best_key_len = key_len;
            }
        }
        best_match
    }

    /// Seek a mutable child by multilevel key
    /// Finds the longest matching key (most specific match first)
    pub fn seek_multilevel_child_mut(&mut self, key_bytes: &[u8]) -> Option<&mut N> {
        if key_bytes.is_empty() {
            return None;
        }

        let mut best_idx = None;
        let mut best_key_len = 0;

        for i in 0..self.num_children as usize {
            let key_len = self.key_lengths[i] as usize;
            if key_bytes.len() >= key_len && 
               &key_bytes[..key_len] == &self.keys[i][..key_len] &&
               key_len > best_key_len {
                best_idx = Some(i);
                best_key_len = key_len;
            }
        }
        
        best_idx.map(|idx| unsafe { self.children[idx].assume_init_mut() })
    }

    /// Delete a child by multilevel key
    /// Finds the longest matching key (most specific match first)
    pub fn delete_multilevel_child(&mut self, key_bytes: &[u8]) -> Option<N> {
        if key_bytes.is_empty() {
            return None;
        }

        // Find the child to delete (longest matching key)
        let mut delete_idx = None;
        let mut best_key_len = 0;
        
        for i in 0..self.num_children as usize {
            let key_len = self.key_lengths[i] as usize;
            if key_bytes.len() >= key_len && 
               &key_bytes[..key_len] == &self.keys[i][..key_len] &&
               key_len > best_key_len {
                delete_idx = Some(i);
                best_key_len = key_len;
            }
        }

        let delete_idx = delete_idx?;
        
        // Extract the child
        let child = std::mem::replace(&mut self.children[delete_idx], MaybeUninit::uninit());
        
        // Shift remaining elements left
        for i in delete_idx..(self.num_children as usize - 1) {
            self.keys[i] = self.keys[i + 1];
            self.key_lengths[i] = self.key_lengths[i + 1];
            self.children[i] = std::mem::replace(&mut self.children[i + 1], MaybeUninit::uninit());
        }
        
        // Clear the last slot
        self.keys[self.num_children as usize - 1] = [0; 4];
        self.key_lengths[self.num_children as usize - 1] = 0;
        self.children[self.num_children as usize - 1] = MaybeUninit::uninit();
        
        self.num_children -= 1;
        
        Some(unsafe { child.assume_init() })
    }

    /// Get an iterator over (key_bytes, child) pairs
    pub fn iter(&self) -> impl Iterator<Item = (&[u8], &N)> {
        (0..self.num_children as usize).map(move |i| {
            let key_len = self.key_lengths[i] as usize;
            let key_bytes = &self.keys[i][..key_len];
            let child = unsafe { self.children[i].assume_init_ref() };
            (key_bytes, child)
        })
    }

    /// Check if this node is full
    pub fn is_full(&self) -> bool {
        self.num_children >= 4
    }

    /// Check if this node is empty
    pub fn is_empty(&self) -> bool {
        self.num_children == 0
    }
}

impl<N> Drop for MultilevelNode4<N> {
    fn drop(&mut self) {
        // Drop all initialized children
        for i in 0..self.num_children as usize {
            unsafe { self.children[i].assume_init_drop() };
        }
        self.num_children = 0;
    }
}

// For compatibility with single-byte NodeMapping trait, we implement it
// but only for the first byte of multilevel keys
impl<N> NodeMapping<N, 4> for MultilevelNode4<N> {
    #[inline]
    fn add_child(&mut self, key: u8, node: N) {
        // Add as a single-byte multilevel key
        self.add_multilevel_child(&[key], node).expect("Node should not be full");
    }

    #[inline]
    fn seek_child(&self, key: u8) -> Option<&N> {
        // Find any multilevel key that starts with this byte
        for i in 0..self.num_children as usize {
            if self.key_lengths[i] > 0 && self.keys[i][0] == key {
                return Some(unsafe { self.children[i].assume_init_ref() });
            }
        }
        None
    }

    #[inline]
    fn seek_child_mut(&mut self, key: u8) -> Option<&mut N> {
        // Find any multilevel key that starts with this byte
        for i in 0..self.num_children as usize {
            if self.key_lengths[i] > 0 && self.keys[i][0] == key {
                return Some(unsafe { self.children[i].assume_init_mut() });
            }
        }
        None
    }

    #[inline]
    fn delete_child(&mut self, key: u8) -> Option<N> {
        // Find any multilevel key that starts with this byte
        for i in 0..self.num_children as usize {
            if self.key_lengths[i] > 0 && self.keys[i][0] == key {
                // Extract the child
                let child = std::mem::replace(&mut self.children[i], MaybeUninit::uninit());
                
                // Shift remaining elements left
                for j in i..(self.num_children as usize - 1) {
                    self.keys[j] = self.keys[j + 1];
                    self.key_lengths[j] = self.key_lengths[j + 1];
                    self.children[j] = std::mem::replace(&mut self.children[j + 1], MaybeUninit::uninit());
                }
                
                // Clear the last slot
                self.keys[self.num_children as usize - 1] = [0; 4];
                self.key_lengths[self.num_children as usize - 1] = 0;
                self.children[self.num_children as usize - 1] = MaybeUninit::uninit();
                
                self.num_children -= 1;
                
                return Some(unsafe { child.assume_init() });
            }
        }
        None
    }

    #[inline]
    fn num_children(&self) -> usize {
        self.num_children as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fits_in_cache_line() {
        // Verify MultilevelNode4 fits within a 64-byte cache line
        assert!(std::mem::size_of::<MultilevelNode4<u8>>() <= 64);
        
        // Also test with a typical node type to be realistic
        use crate::node::DefaultNode;
        use crate::partials::array_partial::ArrPartial;
        
        assert!(std::mem::size_of::<MultilevelNode4<DefaultNode<ArrPartial<16>, u32>>>() <= 64);
    }

    #[test]
    fn test_basic_operations() {
        let mut node = MultilevelNode4::<u32>::new();
        
        // Test single-byte keys
        assert!(node.add_multilevel_child(&[1], 10).is_ok());
        assert!(node.add_multilevel_child(&[2], 20).is_ok());
        
        assert_eq!(node.seek_multilevel_child(&[1]), Some(&10));
        assert_eq!(node.seek_multilevel_child(&[2]), Some(&20));
        assert_eq!(node.seek_multilevel_child(&[3]), None);
        
        assert_eq!(node.delete_multilevel_child(&[1]), Some(10));
        assert_eq!(node.seek_multilevel_child(&[1]), None);
        assert_eq!(node.num_children(), 1);
    }

    #[test]
    fn test_multilevel_keys() {
        let mut node = MultilevelNode4::<u32>::new();
        
        // Test multilevel keys
        assert!(node.add_multilevel_child(&[1, 2], 12).is_ok());
        assert!(node.add_multilevel_child(&[1, 2, 3], 123).is_ok());
        assert!(node.add_multilevel_child(&[2, 3, 4, 5], 2345).is_ok());
        
        assert_eq!(node.seek_multilevel_child(&[1, 2, 0, 0]), Some(&12));
        assert_eq!(node.seek_multilevel_child(&[1, 2, 3, 0]), Some(&123));
        assert_eq!(node.seek_multilevel_child(&[2, 3, 4, 5]), Some(&2345));
        
        // Partial matches should work
        assert_eq!(node.seek_multilevel_child(&[1, 2, 9, 9]), Some(&12));
        assert_eq!(node.seek_multilevel_child(&[1, 2, 3, 9]), Some(&123));
    }

    #[test]
    fn test_node_mapping_trait() {
        let mut node = MultilevelNode4::<u32>::new();
        
        // Test NodeMapping trait methods
        node.add_child(1, 10);
        node.add_child(2, 20);
        
        assert_eq!(node.seek_child(1), Some(&10));
        assert_eq!(node.seek_child(2), Some(&20));
        assert_eq!(node.num_children(), 2);
        
        assert_eq!(node.delete_child(1), Some(10));
        assert_eq!(node.num_children(), 1);
    }

    #[test]
    fn test_capacity_limits() {
        let mut node = MultilevelNode4::<u32>::new();
        
        // Fill to capacity
        for i in 0..4 {
            assert!(node.add_multilevel_child(&[i], i as u32).is_ok());
        }
        
        // Should be full now
        assert!(node.is_full());
        assert!(node.add_multilevel_child(&[4], 4).is_err());
        
        // Delete one and should work again
        assert!(node.delete_multilevel_child(&[0]).is_some());
        assert!(!node.is_full());
        assert!(node.add_multilevel_child(&[4], 4).is_ok());
    }

    #[test]
    fn test_invalid_keys() {
        let mut node = MultilevelNode4::<u32>::new();
        
        // Empty key should fail
        assert!(node.add_multilevel_child(&[], 0).is_err());
        
        // Too long key should fail
        assert!(node.add_multilevel_child(&[1, 2, 3, 4, 5], 0).is_err());
    }

    #[test]
    fn test_iterator() {
        let mut node = MultilevelNode4::<u32>::new();
        
        node.add_multilevel_child(&[1], 10).unwrap();
        node.add_multilevel_child(&[2, 3], 23).unwrap();
        node.add_multilevel_child(&[4, 5, 6], 456).unwrap();
        
        let items: Vec<_> = node.iter().collect();
        assert_eq!(items.len(), 3);
        
        // Check that we can find our items (order may vary)
        assert!(items.iter().any(|(k, v)| k == &[1] && **v == 10));
        assert!(items.iter().any(|(k, v)| k == &[2, 3] && **v == 23));
        assert!(items.iter().any(|(k, v)| k == &[4, 5, 6] && **v == 456));
    }
}