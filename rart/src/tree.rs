use std::cmp::min;
use std::collections::{Bound, HashMap};
use std::fmt::Debug;
use std::ops::RangeBounds;

use crate::iter::Iter;
use crate::keys::KeyTrait;
use crate::node::{Node, NodeType};
use crate::partials::Partial;
use crate::range::Range;

#[derive(Debug)]
pub struct NodeStats {
    width: usize,
    total_nodes: usize,
    total_children: usize,
    density: f64,
}
#[derive(Debug)]
pub struct TreeStats {
    pub node_stats: HashMap<usize, NodeStats>,
    pub num_leaves: usize,
    pub num_values: usize,
    pub num_inner_nodes: usize,
    pub total_density: f64,
    pub max_height: usize,
}

pub struct AdaptiveRadixTree<KeyType, ValueType>
where
    KeyType: KeyTrait,
{
    root: Option<Node<KeyType::PartialType, ValueType>>,
    _phantom: std::marker::PhantomData<KeyType>,
}

impl<KeyType: KeyTrait, ValueType> Default for AdaptiveRadixTree<KeyType, ValueType> {
    fn default() -> Self {
        Self::new()
    }
}

fn update_tree_stats<PartialType: Partial, ValueType>(
    tree_stats: &mut TreeStats,
    node: &Node<PartialType, ValueType>,
) {
    tree_stats
        .node_stats
        .entry(node.capacity())
        .and_modify(|e| {
            e.total_nodes += 1;
            e.total_children += node.num_children();
        })
        .or_insert(NodeStats {
            width: node.capacity(),
            total_nodes: 1,
            total_children: node.num_children(),
            density: 0.0,
        });
}

impl<KeyType, ValueType> AdaptiveRadixTree<KeyType, ValueType>
where
    KeyType: KeyTrait,
{
    pub fn new() -> Self {
        Self {
            root: None,
            _phantom: Default::default(),
        }
    }

    #[inline]
    pub fn get<Key>(&self, key: Key) -> Option<&ValueType>
    where
        Key: Into<KeyType>,
    {
        self.get_k(&key.into())
    }

    #[inline]
    pub fn get_k(&self, key: &KeyType) -> Option<&ValueType> {
        AdaptiveRadixTree::get_iterate(self.root.as_ref()?, key)
    }

    fn get_iterate<'a>(
        cur_node: &'a Node<KeyType::PartialType, ValueType>,
        key: &KeyType,
    ) -> Option<&'a ValueType> {
        let mut cur_node = cur_node;
        let mut depth = 0;
        loop {
            let prefix_common_match = cur_node.prefix.prefix_length_key(key, depth);
            if prefix_common_match != cur_node.prefix.len() {
                return None;
            }

            if cur_node.prefix.len() == key.length_at(depth) {
                return cur_node.value();
            }
            let k = key.at(depth + cur_node.prefix.len());
            depth += cur_node.prefix.len();
            cur_node = cur_node.seek_child(k)?
        }
    }

    #[inline]
    pub fn get_mut<Key>(&mut self, key: Key) -> Option<&mut ValueType>
    where
        Key: Into<KeyType>,
    {
        self.get_mut_k(&key.into())
    }

    #[inline]
    pub fn get_mut_k(&mut self, key: &KeyType) -> Option<&mut ValueType> {
        AdaptiveRadixTree::get_iterate_mut(self.root.as_mut()?, key)
    }

    fn get_iterate_mut<'a>(
        cur_node: &'a mut Node<KeyType::PartialType, ValueType>,
        key: &KeyType,
    ) -> Option<&'a mut ValueType> {
        let mut cur_node = cur_node;
        let mut depth = 0;
        loop {
            let prefix_common_match = cur_node.prefix.prefix_length_key(key, depth);
            if prefix_common_match != cur_node.prefix.len() {
                return None;
            }

            if cur_node.prefix.len() == key.length_at(depth) {
                return cur_node.value_mut();
            }

            let k = key.at(depth + cur_node.prefix.len());
            depth += cur_node.prefix.len();
            cur_node = cur_node.seek_child_mut(k)?;
        }
    }

    pub fn iter(&self) -> Iter<KeyType::PartialType, ValueType> {
        Iter::new(self.root.as_ref())
    }

    pub fn range<'a, R>(&'a self, range: R) -> Range<KeyType, ValueType>
    where
        R: RangeBounds<KeyType> + 'a,
    {
        if self.root.is_none() {
            return Range::empty();
        }

        let mut iter = self.iter();

        let start_key = match range.start_bound() {
            Bound::Included(start_key) | Bound::Excluded(start_key) => start_key,
            Bound::Unbounded => {
                let bound = range.end_bound().cloned();
                return Range::for_iter(iter, bound);
            }
        };

        while let Some((k_vec, _)) = iter.next() {
            if start_key.matches_slice(k_vec.as_slice()) {
                if let Bound::Excluded(_) = range.start_bound() {
                    iter.next();
                }
                let bound = range.end_bound().cloned();
                return Range::for_iter(iter, bound);
            }
        }

        Range::empty()
    }

    pub fn insert<KV>(&mut self, key: KV, value: ValueType) -> Option<ValueType>
    where
        KV: Into<KeyType>,
    {
        self.insert_k(&key.into(), value)
    }

    pub fn insert_k(&mut self, key: &KeyType, value: ValueType) -> Option<ValueType> {
        if self.root.is_none() {
            self.root = Some(Node::new_leaf(key.to_partial(0), value));
            return None;
        };

        let root = self.root.as_mut().unwrap();

        AdaptiveRadixTree::insert_recurse(root, key, value, 0)
    }

    fn insert_recurse(
        cur_node: &mut Node<KeyType::PartialType, ValueType>,
        key: &KeyType,
        value: ValueType,
        depth: usize,
    ) -> Option<ValueType> {
        let longest_common_prefix = cur_node.prefix.prefix_length_key(key, depth);

        let is_prefix_match =
            min(cur_node.prefix.len(), key.length_at(depth)) == longest_common_prefix;

        // Prefix fully covers this node.
        // Either sets the value or replaces the old value already here.
        if is_prefix_match && cur_node.prefix.len() == key.length_at(depth) {
            if let NodeType::Leaf(ref mut v) = &mut cur_node.ntype {
                return Some(std::mem::replace(v, value));
            } else {
                panic!("Node type mismatch")
            }
        }

        // Prefix is part of the current node, but doesn't fully cover it.
        // We have to break this node up, creating a new parent node, and a sibling for our leaf.
        if !is_prefix_match {
            let new_prefix = cur_node.prefix.partial_after(longest_common_prefix);
            let old_node_prefix = std::mem::replace(&mut cur_node.prefix, new_prefix);

            // We will replace this leaf node with a new inner node. The new value will join the
            // current node as sibling, both a child of the new node.
            let n4 = Node::new_inner(old_node_prefix.partial_before(longest_common_prefix));

            let k1 = old_node_prefix.at(longest_common_prefix);
            let k2 = key.at(depth + longest_common_prefix);

            let replacement_current = std::mem::replace(cur_node, n4);

            // We've deferred creating the leaf til now so that we can take ownership over the
            // key after other things are done peering at it.
            let new_leaf = Node::new_leaf(key.to_partial(depth + longest_common_prefix), value);

            // Add the old leaf node as a child of the new inner node.
            cur_node.add_child(k1, replacement_current);
            cur_node.add_child(k2, new_leaf);

            return None;
        }

        // We must be an inner node, and either we need a new baby, or one of our children does, so
        // we'll hunt and see.
        let k = key.at(depth + longest_common_prefix);

        let child_for_key = cur_node.seek_child_mut(k);
        if let Some(child) = child_for_key {
            return AdaptiveRadixTree::insert_recurse(
                child,
                key,
                value,
                depth + longest_common_prefix,
            );
        };

        // We should not be a leaf at this point. If so, something bad has happened.
        assert!(cur_node.is_inner());
        let new_leaf = Node::new_leaf(key.to_partial(depth + longest_common_prefix), value);
        cur_node.add_child(k, new_leaf);
        None
    }

    pub fn remove<KV>(&mut self, key: KV) -> Option<ValueType>
    where
        KV: Into<KeyType>,
    {
        self.remove_k(&key.into())
    }

    pub fn remove_k(&mut self, key: &KeyType) -> Option<ValueType> {
        let Some(root) = self.root.as_mut() else {
            return None;
        };

        let prefix_common_match = root.prefix.prefix_length_key(key, 0);
        if prefix_common_match != root.prefix.len() {
            return None;
        }

        // Special case, if the root is a leaf and matches the key, we can just remove it
        // immediately. If it doesn't match our key, then we have nothing to do here anyways.
        if root.is_leaf() {
            // Move the value of the leaf in root. To do this, replace self.root  with None and
            // then unwrap the value out of the Option & Leaf.
            let stolen = self.root.take().unwrap();
            let leaf = match stolen.ntype {
                NodeType::Leaf(v) => v,
                _ => unreachable!(),
            };
            return Some(leaf);
        }

        let result = AdaptiveRadixTree::remove_recurse(root, key, prefix_common_match);
        if root.is_inner() && root.num_children() == 0 {
            // Prune root if it's now empty.
            self.root = None;
        }
        result
    }

    fn remove_recurse(
        parent_node: &mut Node<KeyType::PartialType, ValueType>,
        key: &KeyType,
        depth: usize,
    ) -> Option<ValueType> {
        // Seek the child that matches the key at this depth, which is the first character at the
        // depth we're at.
        let c = key.at(depth);
        let child_node = parent_node.seek_child_mut(c)?;

        let prefix_common_match = child_node.prefix.prefix_length_key(key, depth);
        if prefix_common_match != child_node.prefix.len() {
            return None;
        }

        // If the child is a leaf, and the prefix matches the key, we can remove it from this parent
        // node. If the prefix does not match, then we have nothing to do here.
        if child_node.is_leaf() {
            if child_node.prefix.len() != (key.length_at(depth)) {
                return None;
            }
            let node = parent_node.delete_child(c).unwrap();
            let v = match node.ntype {
                NodeType::Leaf(v) => v,
                _ => unreachable!(),
            };
            return Some(v);
        }

        // Otherwise, recurse down the branch in that direction.
        let result =
            AdaptiveRadixTree::remove_recurse(child_node, key, depth + child_node.prefix.len());

        // If after this our child we just recursed into no longer has children of its own, it can
        // be collapsed into us. In this way we can prune the tree as we go.
        if result.is_some() && child_node.is_inner() && child_node.num_children() == 0 {
            let prefix = child_node.prefix.clone();
            let deleted = parent_node.delete_child(c).unwrap();
            assert_eq!(prefix.to_slice(), deleted.prefix.to_slice());
        }

        result
    }

    pub fn get_tree_stats(&self) -> TreeStats {
        let mut stats = TreeStats {
            node_stats: Default::default(),
            num_leaves: 0,
            num_values: 0,
            num_inner_nodes: 0,
            total_density: 0.0,
            max_height: 0,
        };
        if self.root.is_none() {
            return stats;
        }

        AdaptiveRadixTree::<KeyType, ValueType>::get_tree_stats_recurse(
            self.root.as_ref().unwrap(),
            &mut stats,
            1,
        );

        let total_inner_nodes = stats
            .node_stats
            .values()
            .map(|ns| ns.total_nodes)
            .sum::<usize>();
        let mut total_children = 0;
        let mut total_width = 0;
        for ns in stats.node_stats.values_mut() {
            total_children += ns.total_children;
            total_width += ns.width * ns.total_nodes;
            ns.density = ns.total_children as f64 / (ns.width * ns.total_nodes) as f64;
        }
        let total_density = total_children as f64 / total_width as f64;
        stats.num_inner_nodes = total_inner_nodes;
        stats.total_density = total_density;

        stats
    }

    fn get_tree_stats_recurse(
        node: &Node<KeyType::PartialType, ValueType>,
        tree_stats: &mut TreeStats,
        height: usize,
    ) {
        if height > tree_stats.max_height {
            tree_stats.max_height = height;
        }
        if node.value().is_some() {
            tree_stats.num_values += 1;
        }
        match node.ntype {
            NodeType::Leaf(_) => {
                tree_stats.num_leaves += 1;
            }
            _ => {
                update_tree_stats(tree_stats, node);
            }
        }
        for (_k, child) in node.iter() {
            AdaptiveRadixTree::<KeyType, ValueType>::get_tree_stats_recurse(
                child,
                tree_stats,
                height + 1,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{btree_map, BTreeMap, BTreeSet};
    use std::fmt::Debug;

    use rand::seq::SliceRandom;
    use rand::{thread_rng, Rng};

    use crate::keys::array_key::ArrayKey;
    use crate::keys::KeyTrait;
    use crate::tree;
    use crate::tree::AdaptiveRadixTree;

    #[test]
    fn test_root_set_get() {
        let mut q = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        let key = ArrayKey::new_from_str("abc");
        assert!(q.insert("abc", 1).is_none());
        assert_eq!(q.get_k(&key), Some(&1));
    }

    #[test]
    fn test_string_keys_get_set() {
        let mut q = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        q.insert("abcd", 1);
        q.insert("abc", 2);
        q.insert("abcde", 3);
        q.insert("xyz", 4);
        q.insert("xyz", 5);
        q.insert("axyz", 6);
        q.insert("1245zzz", 6);

        assert_eq!(*q.get("abcd").unwrap(), 1);
        assert_eq!(*q.get("abc").unwrap(), 2);
        assert_eq!(*q.get("abcde").unwrap(), 3);
        assert_eq!(*q.get("axyz").unwrap(), 6);
        assert_eq!(*q.get("xyz").unwrap(), 5);

        assert_eq!(q.remove("abcde"), Some(3));
        assert_eq!(q.get("abcde"), None);
        assert_eq!(*q.get("abc").unwrap(), 2);
        assert_eq!(*q.get("axyz").unwrap(), 6);
        assert_eq!(q.remove("abc"), Some(2));
        assert_eq!(q.get("abc"), None);
    }

    #[test]
    fn test_int_keys_get_set() {
        let mut q = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        q.insert_k(&500i32.into(), 3);
        assert_eq!(q.get_k(&500i32.into()), Some(&3));
        q.insert_k(&666i32.into(), 2);
        assert_eq!(q.get_k(&666i32.into()), Some(&2));
        q.insert_k(&1i32.into(), 1);
        assert_eq!(q.get_k(&1i32.into()), Some(&1));
    }

    fn gen_random_string_keys<const S: usize>(
        l1_prefix: usize,
        l2_prefix: usize,
        suffix: usize,
    ) -> Vec<(ArrayKey<S>, String)> {
        let mut keys = Vec::new();
        let chars: Vec<char> = ('a'..='z').collect();
        for i in 0..chars.len() {
            let level1_prefix = chars[i].to_string().repeat(l1_prefix);
            for i in 0..chars.len() {
                let level2_prefix = chars[i].to_string().repeat(l2_prefix);
                let key_prefix = level1_prefix.clone() + &level2_prefix;
                for _ in 0..=u8::MAX {
                    let suffix: String = (0..suffix)
                        .map(|_| chars[thread_rng().gen_range(0..chars.len())])
                        .collect();
                    let string = key_prefix.clone() + &suffix;
                    let k = string.clone().into();
                    keys.push((k, string));
                }
            }
        }

        keys.shuffle(&mut thread_rng());
        keys
    }

    #[test]
    fn test_bulk_random_string_query() {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, String>::new();
        let keys = gen_random_string_keys(3, 2, 3);
        let mut num_inserted = 0;
        for (_i, key) in keys.iter().enumerate() {
            if tree.insert_k(&key.0, key.1.clone()).is_none() {
                num_inserted += 1;
                assert!(tree.get_k(&key.0).is_some());
            }
        }
        let mut rng = thread_rng();
        for _i in 0..5_000_000 {
            let entry = &keys[rng.gen_range(0..keys.len())];
            let val = tree.get_k(&entry.0);
            assert!(val.is_some());
            assert_eq!(*val.unwrap(), entry.1);
        }

        let stats = tree.get_tree_stats();
        assert_eq!(stats.num_values, num_inserted);
        eprintln!("Tree stats: {:?}", stats);
    }

    #[test]
    fn test_random_numeric_insert_get() {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
        let count = 9_000_000;
        let mut rng = thread_rng();
        let mut keys_inserted = vec![];
        for i in 0..count {
            let value = i;
            let rnd_key = rng.gen_range(0..count);
            if tree.get(rnd_key).is_none() && tree.insert(rnd_key, value).is_none() {
                let result = tree.get(rnd_key);
                assert!(result.is_some());
                assert_eq!(*result.unwrap(), value);
                keys_inserted.push((rnd_key, value));
            }
        }

        let stats = tree.get_tree_stats();
        assert_eq!(stats.num_values, keys_inserted.len());

        for (key, value) in &keys_inserted {
            let result = tree.get(key);
            assert!(result.is_some(),);
            assert_eq!(*result.unwrap(), *value,);
        }
    }

    fn from_be_bytes_key(k: &Vec<u8>) -> u64 {
        let k = if k.len() < 8 {
            let mut new_k = vec![0; 8];
            new_k[8 - k.len()..].copy_from_slice(k);
            new_k
        } else {
            k.clone()
        };
        let k = k.as_slice();

        u64::from_be_bytes(k[0..8].try_into().unwrap())
    }

    #[test]
    fn test_iter() {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
        let count = 10000;
        let mut rng = thread_rng();
        let mut keys_inserted = BTreeSet::new();
        for i in 0..count {
            let _value = i;
            let rnd_val = rng.gen_range(0..count);
            let rnd_key: ArrayKey<16> = rnd_val.into();
            if tree.get_k(&rnd_key).is_none() && tree.insert_k(&rnd_key, rnd_val).is_none() {
                let result = tree.get_k(&rnd_key);
                assert!(result.is_some());
                assert_eq!(*result.unwrap(), rnd_val);
                keys_inserted.insert((rnd_val, rnd_val));
            }
        }

        // Iteration of keys_inserted and tree should be the same, so we should be able to zip the
        // keys of the tree and the elements of keys_inserted and get the same result.
        let tree_iter = tree.iter();
        let keys_inserted_iter = keys_inserted.iter();
        for (tree_entry, (inserted_key, _)) in tree_iter.zip(keys_inserted_iter) {
            let k = from_be_bytes_key(&tree_entry.0);
            // eprintln!("k: {}, inserted_key: {}", k, inserted_key);
            assert_eq!(
                k,
                *inserted_key,
                "k: {}, inserted_key: {}; prefix: {:?}, inserted_be: {:?}, value: {}",
                k,
                inserted_key,
                tree_entry.0.as_slice(),
                inserted_key.to_be_bytes(),
                tree_entry.1
            );
        }
    }

    #[test]
    // The following cases were found by fuzzing, and identified bugs in `remove`
    fn test_delete_regressions() {
        // DO_INSERT,12297829382473034287,72245244022401706
        // DO_INSERT,12297829382473034410,5425513372477729450
        // DO_DELETE,12297829382473056255,Some(5425513372477729450),None
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, usize>::new();
        assert!(tree
            .insert(12297829382473034287usize, 72245244022401706usize)
            .is_none());
        assert!(tree
            .insert(12297829382473034410usize, 5425513372477729450usize)
            .is_none());
        // assert!(tree.remove(&ArrayKey::new_from_unsigned(12297829382473056255usize)).is_none());

        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, usize>::new();
        // DO_INSERT,0,8101975729639522304
        // DO_INSERT,4934144,18374809624973934592
        // DO_DELETE,0,None,Some(8101975729639522304)
        assert!(tree.insert(0usize, 8101975729639522304usize).is_none());
        assert!(tree
            .insert(4934144usize, 18374809624973934592usize)
            .is_none());
        assert_eq!(tree.get(0usize), Some(&8101975729639522304usize));
        assert_eq!(tree.remove(0usize), Some(8101975729639522304usize));
        assert_eq!(tree.get(4934144usize), Some(&18374809624973934592usize));

        // DO_INSERT,8102098874941833216,8101975729639522416
        // DO_INSERT,8102099357864587376,18374810107896688752
        // DO_DELETE,0,Some(8101975729639522416),None
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, usize>::new();
        assert!(tree
            .insert(8102098874941833216usize, 8101975729639522416usize)
            .is_none());
        assert!(tree
            .insert(8102099357864587376usize, 18374810107896688752usize)
            .is_none());
        assert_eq!(tree.get(0usize), None);
        assert_eq!(tree.remove(0usize), None);
    }

    #[test]
    fn test_delete() {
        // Insert a bunch of random keys and values into both a btree and our tree, then iterate
        // over the btree and delete the keys from our tree. Then, iterate over our tree and make
        // sure it's empty.
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
        let mut btree = BTreeMap::new();
        let count = 5_000;
        let mut rng = thread_rng();
        for i in 0..count {
            let _value = i;
            let rnd_val = rng.gen_range(0..u64::MAX);
            let rnd_key: ArrayKey<16> = rnd_val.into();
            tree.insert_k(&rnd_key, rnd_val);
            btree.insert(rnd_val, rnd_val);
        }

        for (key, value) in btree.iter() {
            let key: ArrayKey<16> = (*key).into();
            let get_result = tree.get_k(&key);
            assert_eq!(
                get_result.cloned(),
                Some(*value),
                "Key with prefix {:?} not found in tree; it should be",
                key.to_partial(0).to_slice()
            );
            let result = tree.remove_k(&key);
            assert_eq!(result, Some(*value));
        }
    }
    // Compare the results of a range query on an AdaptiveRadixTree and a BTreeMap, because we can
    // safely assume the latter exhibits correct behavior.
    fn test_range_matches<'a, KeyType: KeyTrait, ValueType: PartialEq + Debug + 'a>(
        art_range: tree::Range<'a, KeyType, ValueType>,
        btree_range: btree_map::Range<'a, u64, ValueType>,
    ) {
        for (art_entry, btree_entry) in art_range.zip(btree_range) {
            let art_key = from_be_bytes_key(&art_entry.0);
            assert_eq!(art_key, *btree_entry.0);
            assert_eq!(art_entry.1, btree_entry.1);
        }
    }

    #[test]
    fn test_range() {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
        let count = 10000;
        let mut rng = thread_rng();
        let mut keys_inserted = BTreeMap::new();
        for i in 0..count {
            let _value = i;
            let rnd_val = rng.gen_range(0..count);
            let rnd_key: ArrayKey<16> = rnd_val.into();
            if tree.get_k(&rnd_key).is_none() && tree.insert_k(&rnd_key, rnd_val).is_none() {
                let result = tree.get_k(&rnd_key);
                assert!(result.is_some());
                assert_eq!(*result.unwrap(), rnd_val);
                keys_inserted.insert(rnd_val, rnd_val);
            }
        }

        // Test for range with unbounded start and exclusive end
        let end_key: ArrayKey<16> = 100.into();
        let t_r = tree.range(..end_key);
        let k_r = keys_inserted.range(..100);
        test_range_matches(t_r, k_r);

        // Test for range with unbounded start and inclusive end.
        let t_r = tree.range(..=end_key);
        let k_r = keys_inserted.range(..=100);
        test_range_matches(t_r, k_r);

        // Test for range with unbounded end and exclusive start
        let start_key: ArrayKey<16> = 100.into();
        let t_r = tree.range(start_key..);
        let k_r = keys_inserted.range(100..);
        test_range_matches(t_r, k_r);

        // Test for range with bounded start and end (exclusive)
        let end_key: ArrayKey<16> = 1000.into();
        let t_r = tree.range(start_key..end_key);
        let k_r = keys_inserted.range(100..1000);
        test_range_matches(t_r, k_r);

        // Test for range with bounded start and end (inclusive)
        let t_r = tree.range(start_key..=end_key);
        let k_r = keys_inserted.range(100..=1000);
        test_range_matches(t_r, k_r);
    }
}
