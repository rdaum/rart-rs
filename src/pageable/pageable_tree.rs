use std::cmp::min;
use std::fmt::Debug;

use crate::pageable::node_store::{AddChildResult, NodeStore};
use crate::Partial;
use crate::partials::key::Key;

pub trait PrefixTraits: Partial + Clone + PartialEq + Debug + for<'a> From<&'a [u8]> {}
impl<T: Partial + Clone + PartialEq +  Debug + for<'a> From<&'a [u8]>> PrefixTraits for T {}

/// Encapsulates the traversal state of the tree during inserts or removals.
struct Cursor<NP: Clone> {
    // The current node being traversed.
    node_ptr: NP,
    // The parent of the current node, or Root if the current node is the root.
    parent_link: ParentLink<NP>,
    // The (prefix) depth (in bytes) of the current iteration position.
    depth: usize,
}

enum ParentLink<NP> {
    // the parent node and the key of our current node in the parent
    Parent { key: u8, parent: NP },
    // we are the root node, we have no parent
    Root,
}

impl<NP: Clone> Cursor<NP> {
    fn start(node_ptr: NP) -> Self {
        Self {
            node_ptr,
            parent_link: ParentLink::Root,
            depth: 0,
        }
    }

    fn descend(self, child: NP, at_key: u8, prefix_len: usize) -> Self {
        Self {
            node_ptr: child,
            parent_link: ParentLink::Parent {
                key: at_key,
                parent: self.node_ptr,
            },
            depth: self.depth + prefix_len,
        }
    }
}

pub struct PageableAdaptiveRadixTree<NP, P, V, NS>
where
    P: PrefixTraits,
    NP: Clone,
    NS: NodeStore<NP, P, V>,
{
    root: Option<NP>,
    pub store: NS,
    phantom: std::marker::PhantomData<(P, V)>,
}

/// A version of the adaptive radix tree where the node pointers are references into separate,
/// pluggable storage.
/// The intent being to be able to provide features like zero-copy access and/or paged storage.
impl<NP: Clone, P: PrefixTraits, V, NS: NodeStore<NP, P, V>>
    PageableAdaptiveRadixTree<NP, P, V, NS>
{
    pub fn new(store: NS) -> Self {
        Self {
            root: None,
            store,
            phantom: Default::default(),
        }
    }

    pub fn get<K: Key>(&self, key: &K) -> Option<&V> {
        self.root.as_ref()?;

        let mut cur_node_ptr = self.root.as_ref().unwrap();
        let mut depth = 0;
        loop {
            let cur_node_prefix = self.store.node_prefix(cur_node_ptr);
            let key_prefix = key.partial_after(depth);
            let prefix_common_match = cur_node_prefix.prefix_length_slice(key_prefix);
            if prefix_common_match != cur_node_prefix.length() {
                return None;
            }

            if cur_node_prefix.length() == key_prefix.len() {
                return self.store.leaf_value(cur_node_ptr);
            }

            let k = key.at(depth + cur_node_prefix.length());
            let next_node = self.store.seek_child(cur_node_ptr, k);
            next_node?;
            depth += cur_node_prefix.length();
            cur_node_ptr = next_node.unwrap();
        }
    }

    pub fn insert<K: Key>(&mut self, key: &K, value: V) -> Option<V> {
        let Some(root) = &self.root else {
            let leaf = self.store.new_leaf(key.as_slice(), value);
            self.root = Some(leaf);
            return None;
        };
        let mut cursor = Cursor::start(root.clone());

        loop {
            let cur_node_prefix = self.store.node_prefix(&cursor.node_ptr);

            let key_prefix = key.partial_after(cursor.depth);
            let longest_common_prefix = cur_node_prefix.prefix_length_slice(key_prefix);

            let is_prefix_match =
                min(cur_node_prefix.length(), key_prefix.len()) == longest_common_prefix;

            // Prefix fully covers this node.
            // Either sets the value or replaces the old value already here.
            if self.store.is_leaf(&cursor.node_ptr)
                && is_prefix_match
                && cur_node_prefix.length() == key_prefix.len()
            {
                return Some(self.store.set_leaf_value(&cursor.node_ptr, value));
            }

            // Prefix is part of the current node, but doesn't fully cover it.
            // We have to break this node up, creating a new parent node, and a sibling for us..
            if !is_prefix_match {
                // We will replace this node with a fresh inner node. The new value will join the
                // current node as sibling, both a child of the new node.
                let new_prefix = cur_node_prefix.partial_before(longest_common_prefix);
                let new_parent_prefix = cur_node_prefix.partial_after(longest_common_prefix);
                let k1 = cur_node_prefix.at(longest_common_prefix);
                drop(cur_node_prefix);

                let n4 = self.store.new_inner(new_prefix);

                let k2 = key_prefix[longest_common_prefix];

                // Create a new leaf node for the new value.
                let new_leaf = self
                    .store
                    .new_leaf(key_prefix[longest_common_prefix..].into(), value);

                // Add the old leaf node as a child of the new inner node, and update its prefix.

                self.store
                    .set_node_prefix(&cursor.node_ptr, new_parent_prefix);
                let AddChildResult::Same = self.store
                    .add_child_to_node(&n4, k1, cursor.node_ptr) else {
                    unreachable!("Grow occurred when adding child to new node");
                };

                // And the new one, too.
                let AddChildResult::Same = self.store.add_child_to_node(&n4, k2, new_leaf) else {
                    unreachable!("Grow occurred when adding child to new node");
                };

                // Replace the current node with the new inner node.
                // This means that the current node is now a child of the new inner node.
                // To do this requires rewriting the node ptr in the parent node.
                // Unfortunately roots require some special treatment.
                return match &cursor.parent_link {
                    ParentLink::Root => {
                        self.root = Some(n4);
                        None
                    }
                    ParentLink::Parent { key, parent } => {
                        self.store.update_child_in_node(parent, *key, n4);
                        None
                    }
                };
            }

            // We must be an inner node, and either we need a new baby, or one of our children does, so
            // we'll hunt and see.
            let k = key_prefix[longest_common_prefix];

            let child_for_key = self.store.seek_child(&cursor.node_ptr, k);

            // If there's no existing child for this key, we'll create a new leaf node for it, add
            // in this prefix, and move on.
            let Some(child_node_ptr) = child_for_key else {
                // If this is a leaf, something is wrong.
                assert!(!self.store.is_leaf(&cursor.node_ptr));

                let new_leaf = self.store.new_leaf(key_prefix[longest_common_prefix..].into(), value);
                self.store.add_child_to_node(&cursor.node_ptr, k, new_leaf);
                return None
            };

            // Otherwise, descend down the tree in that direction...
            cursor = cursor.descend(child_node_ptr.clone(), k, longest_common_prefix);
            continue;
        }
    }

    pub fn remove<K: Key>(&mut self, key: &K) -> bool {
        let Some(root) = &self.root else {
            return false;
        };
        let mut cursor = Cursor::start(root.clone());

        loop {
            let prefix = self.store.node_prefix(&cursor.node_ptr).clone();
            let key_prefix = key.partial_after(cursor.depth);
            let longest_common_prefix = prefix.prefix_length_slice(key_prefix);

            if prefix.length() != longest_common_prefix {
                // No prefix match, so we can't delete anything.
                return false;
            }
            let prefix_matched = min(prefix.length(), key_prefix.len()) == longest_common_prefix;

            // Simplest scenario, we get to just drop the leaf node.
            if prefix_matched && prefix.length() == key_prefix.len() {
                match &mut cursor.parent_link {
                    ParentLink::Root => {
                        self.store.free_node(cursor.node_ptr);
                        self.root = None;
                    }
                    ParentLink::Parent { key, parent } => {
                        self.store.delete_node_from_parent(parent, *key);
                    }
                };
                return true;
            }

            let k = key_prefix[longest_common_prefix];
            let next = self.store.seek_child(&cursor.node_ptr, k);
            if let Some(child_node) = next {
                // If we have no children, this node can be pruned out.
                if self.store.num_children(child_node) == 0 {
                    // We can delete this leaf node, but we also need to remove the pointer to it in the
                    // parent
                    if self.store.node_prefix(child_node).length()
                        == key_prefix.len() - longest_common_prefix
                    {
                        match &mut cursor.parent_link {
                            ParentLink::Root => {
                                self.store.free_node(cursor.node_ptr);
                                self.root = None;
                            }
                            ParentLink::Parent { key, parent } => {
                                self.store.delete_node_from_parent(parent, *key);
                            }
                        };
                        return true;
                    }
                    // Nowhere left to look.
                    return false;
                }
                // Go down the tree
                cursor = cursor.descend(child_node.clone(), k, longest_common_prefix);
                continue;
            }

            return false;
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::{Rng, thread_rng};
    use rand::seq::SliceRandom;

    use crate::pageable::pageable_tree::PageableAdaptiveRadixTree;
    use crate::pageable::vector_node_store::VectorNodeStore;
    use crate::partials::array_partial::ArrPartial;
    use crate::partials::key::{ArrayKey, VectorKey};

    #[test]
    fn test_root_set_get() {
        let mut q = PageableAdaptiveRadixTree::new(VectorNodeStore::<ArrPartial<16>, u64>::new());
        let key = VectorKey::from_str("abc");
        q.insert(&key, 1);
        assert_eq!(*q.get(&key).unwrap(), 1);
    }

    #[test]
    fn test_string_keys_get_set() {
        let mut q = PageableAdaptiveRadixTree::new(VectorNodeStore::<ArrPartial<16>, u64>::new());
        q.insert(&VectorKey::from_str("abcd"), 1);
        q.insert(&VectorKey::from_str("abc"), 2);
        q.insert(&VectorKey::from_str("abcde"), 3);
        q.insert(&VectorKey::from_str("xyz"), 4);
        q.insert(&VectorKey::from_str("xyz"), 5);
        q.insert(&VectorKey::from_str("axyz"), 6);
        q.insert(&VectorKey::from_str("1245zzz"), 6);

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
        let mut q = PageableAdaptiveRadixTree::new(VectorNodeStore::<ArrPartial<8>, u64>::new());
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
        let node_store = VectorNodeStore::<ArrPartial<16>, String>::new();
        let mut tree = PageableAdaptiveRadixTree::new(node_store);
        let keys = gen_random_string_keys(3, 2, 3);
        let mut num_inserted = 0;
        for (_i, key) in keys.iter().enumerate() {
            if tree.insert(&key.0, key.1.clone()).is_none() {
                num_inserted += 1;
                assert!(tree.get(&key.0).is_some());
            }
        }
        let mut rng = thread_rng();
        for _i in 0..1_000_000 {
            let entry = &keys[rng.gen_range(0..keys.len())];
            let val = tree.get(&entry.0);
            assert!(val.is_some());
            assert_eq!(*val.unwrap(), entry.1);
        }
        assert_eq!(tree.store.num_leaves(), num_inserted);
        assert_eq!(
            tree.store.num_prefixes(),
            tree.store.num_leaves() + tree.store.num_inners()
        );
    }

    #[test]
    fn test_bulk_insert_rand_remove() {
        let node_store = VectorNodeStore::<ArrPartial<16>, String>::new();
        let mut tree = PageableAdaptiveRadixTree::new(node_store);
        let keys = gen_random_string_keys(3, 2, 3);
        for (_i, key) in keys.iter().enumerate() {
            if tree.insert(&key.0, key.1.clone()).is_none() {
                assert!(tree.get(&key.0).is_some());
            }
        }
        let mut rng = thread_rng();
        for _i in 0..5_000 {
            let entry = &keys[rng.gen_range(0..keys.len())];
            tree.remove(&entry.0);
        }
    }

    #[test]
    fn test_seq_remove() {
        let mut tree = PageableAdaptiveRadixTree::new(VectorNodeStore::<ArrPartial<8>, _>::new());

        let count = 250_000;
        for i in 0..count {
            tree.insert::<ArrayKey<8>>(&i.into(), i);
        }
        for i in 0..count {
            assert_eq!(tree.get::<ArrayKey<8>>(&i.into()), Some(&i));
        }
        for i in 0..count {
            tree.remove::<ArrayKey<8>>(&i.into());
        }
        for i in 0..count {
            assert!(tree.get::<ArrayKey<8>>(&i.into()).is_none());
        }
    }

    #[test]
    fn test_random_numeric_insert_get() {
        let mut tree = PageableAdaptiveRadixTree::new(VectorNodeStore::<ArrPartial<8>, u64>::new());
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
    }
}
