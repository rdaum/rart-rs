use std::cmp::min;
use std::collections::{Bound, HashMap};
use std::fmt::Debug;

use std::ops::RangeBounds;

use crate::iter::Iter;
use crate::node::{Node, NodeType};
use crate::partials::key::Key;
use crate::range::Range;
use crate::Partial;

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

pub trait PrefixTraits: Partial + Clone + PartialEq + Debug + for<'a> From<&'a [u8]> {}
impl<T: Partial + Clone + PartialEq + Debug + for<'a> From<&'a [u8]>> PrefixTraits for T {}

pub struct AdaptiveRadixTree<P: PrefixTraits, V> {
    root: Option<Node<P, V>>,
}

impl<P: PrefixTraits, V> Default for AdaptiveRadixTree<P, V> {
    fn default() -> Self {
        Self::new()
    }
}

pub fn key_str_rep<K: Key>(k: &K) -> String {
    let s = k.as_slice();
    // hex string
    let s = s
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(" ");
    format!("[{}]", s)
}

pub fn prefix_str_rep<P: PrefixTraits>(p: &P) -> String {
    let s = p.to_slice();
    // hex string
    let s = s
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(" ");
    format!("[{}]", s)
}

fn update_tree_stats<P: Partial + Clone, V>(tree_stats: &mut TreeStats, node: &Node<P, V>) {
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
impl<P: PrefixTraits, V> AdaptiveRadixTree<P, V> {
    pub fn new() -> Self {
        Self { root: None }
    }

    pub fn get<K: Key>(&self, key: &K) -> Option<&V> {
        self.root.as_ref()?;

        let root = self.root.as_ref().unwrap();
        AdaptiveRadixTree::get_iterate(root, key)
    }

    fn get_iterate<'a, K: Key>(cur_node: &'a Node<P, V>, key: &K) -> Option<&'a V> {
        let mut cur_node = cur_node;
        let mut depth = 0;
        loop {
            let key_prefix = key.partial_after(depth);
            let prefix_common_match = cur_node.prefix.prefix_length_slice(key_prefix);
            if prefix_common_match != cur_node.prefix.length() {
                return None;
            }

            if cur_node.prefix.length() == key_prefix.len() {
                return cur_node.value();
            }

            let k = key.at(depth + cur_node.prefix.length());
            let next_node = cur_node.seek_child(k);
            next_node?;
            depth += cur_node.prefix.length();
            // key = key.prefix_after(cur_node.prefix.length()
            cur_node = next_node.unwrap();
        }
    }

    pub fn iter(&self) -> Iter<P, V> {
        Iter::new(self.root.as_ref())
    }

    pub fn range<'a, K: Key, R>(&'a self, range: R) -> Range<K, P, V>
    where
        R: RangeBounds<K> + 'a,
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

        while let Some((k, _)) = iter.next() {
            if start_key.as_slice() == k.as_slice() {
                if let Bound::Excluded(_) = range.start_bound() {
                    iter.next();
                }
                let bound = range.end_bound().cloned();
                return Range::for_iter(iter, bound);
            }
        }

        Range::empty()
    }

    pub fn insert<K: Key>(&mut self, key: &K, value: V) -> Option<V> {
        if self.root.is_none() {
            self.root = Some(Node::new_leaf(key.as_slice().into(), value));
            return None;
        };

        let root = self.root.as_mut().unwrap();

        AdaptiveRadixTree::insert_recurse(root, key, value, 0)
    }

    fn insert_recurse<K: Key>(
        cur_node: &mut Node<P, V>,
        key: &K,
        value: V,
        depth: usize,
    ) -> Option<V> {
        let cur_node_prefix = cur_node.prefix.clone();

        let key_prefix = key.partial_after(depth);
        let longest_common_prefix = cur_node_prefix.prefix_length_slice(key_prefix);

        let is_prefix_match =
            min(cur_node_prefix.length(), key_prefix.len()) == longest_common_prefix;

        // Prefix fully covers this node.
        // Either sets the value or replaces the old value already here.
        if let NodeType::Leaf(ref mut v) = &mut cur_node.ntype {
            if is_prefix_match && cur_node_prefix.length() == key_prefix.len() {
                return Some(std::mem::replace(v, value));
            }
        }

        // Prefix is part of the current node, but doesn't fully cover it.
        // We have to break this node up, creating a new parent node, and a sibling for us..
        if !is_prefix_match {
            cur_node.prefix = cur_node_prefix.partial_after(longest_common_prefix);

            // We will replace this node with a binary inner node. The new value will join the
            // current node as sibling, both a child of the new node.
            let n4 = Node::new_inner(cur_node_prefix.partial_before(longest_common_prefix));

            let k1 = cur_node_prefix.at(longest_common_prefix);
            let k2 = key_prefix[longest_common_prefix];

            let replacement_current = std::mem::replace(cur_node, n4);

            // We've deferred creating the leaf til now so that we can take ownership over the
            // key after other things are done peering at it.
            let new_leaf = Node::new_leaf(key_prefix[longest_common_prefix..].into(), value);

            // Add the old leaf node as a child of the new inner node.
            cur_node.add_child(k1, replacement_current);
            cur_node.add_child(k2, new_leaf);

            return None;
        }

        // We must be an inner node, and either we need a new baby, or one of our children does, so
        // we'll hunt and see.
        let k = key_prefix[longest_common_prefix];

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
        let new_leaf = Node::new_leaf(key_prefix[longest_common_prefix..].into(), value);
        cur_node.add_child(k, new_leaf);
        None
    }

    pub fn remove<K: Key>(&mut self, key: &K) -> bool {
        if self.root.is_none() {
            return false;
        }

        AdaptiveRadixTree::remove_recurse(&mut self.root.as_mut(), key, 0)
    }

    fn remove_recurse<K: Key>(
        cur_node_ptr: &mut Option<&mut Node<P, V>>,
        key: &K,
        depth: usize,
    ) -> bool {
        if cur_node_ptr.is_none() {
            return false;
        }

        let prefix = cur_node_ptr.as_ref().unwrap().prefix.clone();

        let key_prefix = key.partial_after(depth);
        let longest_common_prefix = prefix.prefix_length_slice(key_prefix);

        if prefix.length() != longest_common_prefix {
            // No prefix match, so we can't delete anything.
            return false;
        }
        let prefix_matched = min(prefix.length(), key_prefix.len()) == longest_common_prefix;

        let Some(node) = cur_node_ptr else {
            return false;
        };

        // Simplest scenario, we get to just drop the leaf node.
        if prefix_matched && prefix.length() == key_prefix.len() {
            *cur_node_ptr = None;
            return true;
        }

        let k = key_prefix[longest_common_prefix];
        let mut next = node.seek_child_mut(k);
        if let Some(child_node) = &next {
            // If we have no children, this node can be pruned out.
            if child_node.num_children() == 0 {
                // We can delete this leaf node.
                if child_node.prefix.length() == key_prefix.len() - longest_common_prefix {
                    node.delete_child(k).expect("child not found");
                    return true;
                }
                // Nowhere left to look.
                return false;
            }
            // Go down the tree.
            return AdaptiveRadixTree::remove_recurse(
                &mut next,
                key,
                depth + longest_common_prefix,
            );
        }

        false
    }

    pub fn print_tree(&self) {
        if self.root.is_none() {
            eprintln!("[]]");
            return;
        }

        AdaptiveRadixTree::print_tree_recurse(self.root.as_ref().unwrap(), 0);
    }

    fn print_tree_recurse(node: &Node<P, V>, depth: usize) {
        let indent = "  ".repeat(depth);
        eprintln!(
            "{}{} prefix {}, {} #children",
            indent,
            node.node_type_name(),
            prefix_str_rep(&node.prefix),
            node.num_children()
        );

        for (k, child) in node.iter() {
            eprintln!(
                "{}  ({:02x}) {} =>",
                indent,
                k,
                prefix_str_rep(&child.prefix)
            );
            AdaptiveRadixTree::print_tree_recurse(child, depth + 1);
        }
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

        AdaptiveRadixTree::get_tree_stats_recurse(self.root.as_ref().unwrap(), &mut stats, 1);

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

    fn get_tree_stats_recurse(node: &Node<P, V>, tree_stats: &mut TreeStats, height: usize) {
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
            AdaptiveRadixTree::get_tree_stats_recurse(child, tree_stats, height + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::seq::SliceRandom;
    use rand::{thread_rng, Rng};
    use std::collections::{btree_map, BTreeMap, BTreeSet};
    use std::fmt::Debug;

    use crate::partials::array_partial::ArrPartial;
    use crate::partials::key::{ArrayKey, Key, VectorKey};
    use crate::tree;
    use crate::tree::{key_str_rep, AdaptiveRadixTree, PrefixTraits};

    #[test]
    fn test_root_set_get() {
        let mut q = AdaptiveRadixTree::<ArrPartial<16>, i32>::new();
        let key = VectorKey::from_str("abc");
        q.insert(&key, 1);
        assert_eq!(*q.get(&key).unwrap(), 1);
    }

    #[test]
    fn test_string_keys_get_set() {
        let mut q = AdaptiveRadixTree::<ArrPartial<16>, i32>::new();
        q.insert(&VectorKey::from_str("abcd"), 1);
        q.insert(&VectorKey::from_str("abc"), 2);
        q.insert(&VectorKey::from_str("abcde"), 3);
        q.insert(&VectorKey::from_str("xyz"), 4);
        q.insert(&VectorKey::from_str("xyz"), 5);
        q.insert(&VectorKey::from_str("axyz"), 6);
        q.insert(&VectorKey::from_str("1245zzz"), 6);

        eprintln!("Tree: ");
        q.print_tree();
        eprintln!();

        assert_eq!(*q.get(&VectorKey::from_str("abcd")).unwrap(), 1);
        assert_eq!(*q.get(&VectorKey::from_str("abc")).unwrap(), 2);
        assert_eq!(*q.get(&VectorKey::from_str("abcde")).unwrap(), 3);
        assert_eq!(*q.get(&VectorKey::from_str("axyz")).unwrap(), 6);
        assert_eq!(*q.get(&VectorKey::from_str("xyz")).unwrap(), 5);

        assert!(q.remove(&VectorKey::from_str("abcde")));
        assert_eq!(q.get(&VectorKey::from_str("abcde")), None);
        assert_eq!(*q.get(&VectorKey::from_str("abc")).unwrap(), 2);
        assert_eq!(*q.get(&VectorKey::from_str("axyz")).unwrap(), 6);
        assert!(q.remove(&VectorKey::from_str("abc")));
        assert_eq!(q.get(&VectorKey::from_str("abc")), None);
    }

    #[test]
    fn test_int_keys_get_set() {
        let mut q = AdaptiveRadixTree::<ArrPartial<16>, i32>::new();
        q.insert::<VectorKey>(&500i32.into(), 3);
        assert_eq!(q.get::<VectorKey>(&500i32.into()), Some(&3));
        q.insert::<VectorKey>(&666i32.into(), 2);
        assert_eq!(q.get::<VectorKey>(&666i32.into()), Some(&2));
        q.insert::<VectorKey>(&1i32.into(), 1);
        assert_eq!(q.get::<VectorKey>(&1i32.into()), Some(&1));
    }

    fn gen_random_string_keys(
        l1_prefix: usize,
        l2_prefix: usize,
        suffix: usize,
    ) -> Vec<(VectorKey, String)> {
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
        let mut tree = AdaptiveRadixTree::<ArrPartial<16>, String>::new();
        let keys = gen_random_string_keys(3, 2, 3);
        let mut num_inserted = 0;
        for (_i, key) in keys.iter().enumerate() {
            if tree.insert(&key.0, key.1.clone()).is_none() {
                num_inserted += 1;
                assert!(tree.get(&key.0).is_some());
            }
        }
        let mut rng = thread_rng();
        for _i in 0..5_000_000 {
            let entry = &keys[rng.gen_range(0..keys.len())];
            let val = tree.get(&entry.0);
            assert!(val.is_some());
            assert_eq!(*val.unwrap(), entry.1);
        }

        let stats = tree.get_tree_stats();
        assert_eq!(stats.num_values, num_inserted);
        eprintln!("Tree stats: {:?}", stats);
    }

    #[test]
    fn test_random_numeric_insert_get() {
        let mut tree = AdaptiveRadixTree::<ArrPartial<16>, u64>::new();
        let count = 100_000;
        let mut rng = thread_rng();
        let mut keys_inserted = vec![];
        for i in 0..count {
            let value = i;
            let rnd_key = rng.gen_range(0..count);
            let rnd_key: VectorKey = rnd_key.into();
            if tree.get(&rnd_key).is_none() && tree.insert(&rnd_key, value).is_none() {
                let result = tree.get(&rnd_key);
                assert!(result.is_some());
                assert_eq!(*result.unwrap(), value);
                keys_inserted.push((rnd_key.clone(), value));
            }
        }

        let stats = tree.get_tree_stats();
        assert_eq!(stats.num_values, keys_inserted.len());

        let mut n_checked = 0;
        for (key, value) in &keys_inserted {
            n_checked += 1;
            let result = tree.get(key);
            assert!(
                result.is_some(),
                "key: {:} should be {} was None; check #{}",
                key_str_rep(key),
                value,
                n_checked
            );
            assert_eq!(
                *result.unwrap(),
                *value,
                "key: {:} should be {} was {}, check #{}",
                key_str_rep(key),
                value,
                result.unwrap(),
                n_checked
            );
        }
        eprintln!("stats: {:?}", stats);
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
        let mut tree = AdaptiveRadixTree::<ArrPartial<16>, u64>::new();
        let count = 10000;
        let mut rng = thread_rng();
        let mut keys_inserted = BTreeSet::new();
        for i in 0..count {
            let _value = i;
            let rnd_val = rng.gen_range(0..count);
            let rnd_key: ArrayKey<16> = rnd_val.into();
            if tree.get(&rnd_key).is_none() && tree.insert(&rnd_key, rnd_val).is_none() {
                let result = tree.get(&rnd_key);
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

    // Compare the results of a range query on an AdaptiveRadixTree and a BTreeMap, because we can
    // safely assume the latter exhibits correct behavior.
    fn test_range_matches<'a, K: Key, P: PrefixTraits, V: PartialEq + Debug + 'a>(
        art_range: tree::Range<'a, K, P, V>,
        btree_range: btree_map::Range<'a, u64, V>,
    ) {
        for (art_entry, btree_entry) in art_range.zip(btree_range) {
            let art_key = from_be_bytes_key(&art_entry.0);
            assert_eq!(art_key, *btree_entry.0);
            assert_eq!(art_entry.1, btree_entry.1);
        }
    }

    #[test]
    fn test_range() {
        let mut tree = AdaptiveRadixTree::<ArrPartial<16>, u64>::new();
        let count = 10000;
        let mut rng = thread_rng();
        let mut keys_inserted = BTreeMap::new();
        for i in 0..count {
            let _value = i;
            let rnd_val = rng.gen_range(0..count);
            let rnd_key: ArrayKey<16> = rnd_val.into();
            if tree.get(&rnd_key).is_none() && tree.insert(&rnd_key, rnd_val).is_none() {
                let result = tree.get(&rnd_key);
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
