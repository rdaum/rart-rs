//! N-way merge join operations for Adaptive Radix Trees.
//!
//! Provides streaming merge join algorithms that find common keys across multiple trees
//! by merging their sorted key streams.

use crate::iter::Iter;
use crate::keys::KeyTrait;
use crate::tree::AdaptiveRadixTree;
use crate::versioned_tree::VersionedAdaptiveRadixTree;

/// N-way merge join implementation for regular ART
impl<KeyType, ValueType> AdaptiveRadixTree<KeyType, ValueType>
where
    KeyType: KeyTrait + Clone + Ord,
{
    /// Stream the intersection of keys across multiple trees using merge join.
    ///
    /// This performs an N-way merge join finding keys that exist in ALL provided trees.
    /// Uses a streaming merge algorithm leveraging ART's ordered iteration.
    ///
    /// # Complexity
    /// - Time: O(sum of all input tree sizes)
    /// - Space: O(N) where N is number of trees
    ///
    /// # Examples
    /// ```rust
    /// use rart::{AdaptiveRadixTree, ArrayKey};
    ///
    /// let mut tree1 = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
    /// let mut tree2 = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
    ///
    /// tree1.insert("apple", 1);
    /// tree1.insert("banana", 2);
    /// tree2.insert("apple", 10);
    /// tree2.insert("cherry", 30);
    ///
    /// let trees = vec![&tree1, &tree2];
    /// let common_keys: Vec<_> = AdaptiveRadixTree::merge_join_keys(&trees).collect();
    /// // common_keys contains ["apple"] - the only key in both trees
    /// ```
    pub fn merge_join_keys<'a>(trees: &'a [&'a Self]) -> impl Iterator<Item = KeyType> + 'a {
        OptimizedMergeJoinIterator::new(trees)
    }

    /// Stream keys with access to values from all matching trees.
    ///
    /// For each key that exists in all trees, yields the key along with
    /// references to values from each tree.
    pub fn merge_join_with_values<'a>(
        trees: &'a [&'a Self],
    ) -> impl Iterator<Item = (KeyType, Vec<&'a ValueType>)> + 'a {
        NWayMergeJoinWithValuesIterator::new(trees)
    }
}

/// N-way merge join implementation for versioned ART  
impl<KeyType, ValueType> VersionedAdaptiveRadixTree<KeyType, ValueType>
where
    KeyType: KeyTrait + Clone + Ord,
    ValueType: Clone,
{
    /// Stream the intersection of keys across multiple versioned trees.
    pub fn merge_join_keys<'a>(trees: &'a [&'a Self]) -> impl Iterator<Item = KeyType> + 'a {
        VersionedNWayMergeJoinIterator::new(trees)
    }

    /// Stream keys with access to values from all matching versioned trees.
    ///
    /// For each key that exists in all trees, yields the key along with
    /// references to values from each tree.
    pub fn merge_join_with_values<'a>(
        trees: &'a [&'a Self],
    ) -> impl Iterator<Item = (KeyType, Vec<&'a ValueType>)> + 'a {
        VersionedNWayMergeJoinWithValuesIterator::new(trees)
    }
}

/// Iterator state for tracking position in a single tree
struct TreeIterState<'a, KeyType: KeyTrait, ValueType> {
    iter: Iter<'a, KeyType, KeyType::PartialType, ValueType>,
    current_entry: Option<(KeyType, &'a ValueType)>,
}

impl<'a, KeyType: KeyTrait, ValueType> TreeIterState<'a, KeyType, ValueType> {
    fn new(tree: &'a AdaptiveRadixTree<KeyType, ValueType>, _index: usize) -> Self {
        let mut iter = tree.iter();
        let current_entry = iter.next().map(|(k, v)| (k.clone(), v));

        Self {
            iter,
            current_entry,
        }
    }

    fn current_key(&self) -> Option<&KeyType> {
        self.current_entry.as_ref().map(|(k, _)| k)
    }

    fn current_value(&self) -> Option<&'a ValueType> {
        self.current_entry.as_ref().map(|(_, v)| *v)
    }

    fn advance(&mut self) {
        if let Some((key, value)) = self.iter.next() {
            self.current_entry = Some((key.clone(), value));
        } else {
            self.current_entry = None;
        }
    }
}

/// Streaming N-way merge join iterator
pub struct NWayMergeJoinIterator<'a, KeyType: KeyTrait, ValueType> {
    tree_states: Vec<TreeIterState<'a, KeyType, ValueType>>,
}

impl<'a, KeyType, ValueType> NWayMergeJoinIterator<'a, KeyType, ValueType>
where
    KeyType: KeyTrait + Clone + Ord,
{
    fn new(trees: &'a [&'a AdaptiveRadixTree<KeyType, ValueType>]) -> Self {
        let tree_states = trees
            .iter()
            .enumerate()
            .map(|(i, tree)| TreeIterState::new(tree, i))
            .collect::<Vec<_>>();

        // If any tree is empty, the join result is empty
        if tree_states
            .iter()
            .any(|state| state.current_entry.is_none())
        {
            return Self {
                tree_states: Vec::new(),
            };
        }

        Self { tree_states }
    }
}

impl<'a, KeyType, ValueType> Iterator for NWayMergeJoinIterator<'a, KeyType, ValueType>
where
    KeyType: KeyTrait + Clone + Ord,
{
    type Item = KeyType;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Check if any iterator is exhausted - if so, no more joins possible
            if self.tree_states.is_empty()
                || self
                    .tree_states
                    .iter()
                    .any(|state| state.current_entry.is_none())
            {
                return None;
            }

            // Find the minimum key across all current positions
            let min_key = self
                .tree_states
                .iter()
                .filter_map(|state| state.current_key())
                .min()
                .cloned()?;

            // Check if all trees have this minimum key
            let mut all_match = true;
            for state in &mut self.tree_states {
                if let Some(current_key) = state.current_key() {
                    if *current_key == min_key {
                        // This tree matches, advance it
                        state.advance();
                    } else if *current_key > min_key {
                        // This tree is ahead, we don't have a complete match
                        all_match = false;
                    }
                } else {
                    // Tree is exhausted
                    all_match = false;
                }
            }

            // Advance trees that had keys smaller than min_key (shouldn't happen in correct algorithm)
            for state in &mut self.tree_states {
                if let Some(current_key) = state.current_key() {
                    if *current_key < min_key {
                        state.advance();
                    }
                }
            }

            if all_match {
                return Some(min_key);
            }
        }
    }
}

/// Iterator that also provides access to values
pub struct NWayMergeJoinWithValuesIterator<'a, KeyType: KeyTrait, ValueType> {
    tree_states: Vec<TreeIterState<'a, KeyType, ValueType>>,
}

impl<'a, KeyType, ValueType> NWayMergeJoinWithValuesIterator<'a, KeyType, ValueType>
where
    KeyType: KeyTrait + Clone + Ord,
{
    fn new(trees: &'a [&'a AdaptiveRadixTree<KeyType, ValueType>]) -> Self {
        let tree_states = trees
            .iter()
            .enumerate()
            .map(|(i, tree)| TreeIterState::new(tree, i))
            .collect::<Vec<_>>();

        // If any tree is empty, the join result is empty
        if tree_states
            .iter()
            .any(|state| state.current_entry.is_none())
        {
            return Self {
                tree_states: Vec::new(),
            };
        }

        Self { tree_states }
    }
}

impl<'a, KeyType, ValueType> Iterator for NWayMergeJoinWithValuesIterator<'a, KeyType, ValueType>
where
    KeyType: KeyTrait + Clone + Ord,
{
    type Item = (KeyType, Vec<&'a ValueType>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Check if any iterator is exhausted - if so, no more joins possible
            if self.tree_states.is_empty()
                || self
                    .tree_states
                    .iter()
                    .any(|state| state.current_entry.is_none())
            {
                return None;
            }

            // Find the minimum key across all current positions
            let min_key = self
                .tree_states
                .iter()
                .filter_map(|state| state.current_key())
                .min()
                .cloned()?;

            // Check if all trees have this minimum key
            let mut all_match = true;
            let mut values = Vec::new();

            // First pass: check if all trees have the min_key and collect values from matching trees
            for state in &self.tree_states {
                if let Some(current_key) = state.current_key() {
                    if *current_key == min_key {
                        if let Some(value) = state.current_value() {
                            values.push(value);
                        }
                    } else if *current_key > min_key {
                        // This tree is ahead, we don't have a complete match
                        all_match = false;
                    }
                } else {
                    // Tree is exhausted
                    all_match = false;
                }
            }

            // Second pass: advance all trees appropriately
            for state in &mut self.tree_states {
                if let Some(current_key) = state.current_key() {
                    if *current_key == min_key {
                        // This tree matches, advance it
                        state.advance();
                    } else if *current_key < min_key {
                        // This tree is behind, advance it
                        state.advance();
                    }
                    // Trees with current_key > min_key don't advance
                }
            }

            if all_match {
                return Some((min_key, values));
            }
        }
    }
}

/// Iterator state for tracking position in a versioned tree
struct VersionedTreeIterState<'a, KeyType: KeyTrait, ValueType> {
    iter: crate::versioned_tree::VersionedIter<'a, KeyType, KeyType::PartialType, ValueType>,
    current_entry: Option<(KeyType, &'a ValueType)>,
}

impl<'a, KeyType: KeyTrait + 'a, ValueType: Clone> VersionedTreeIterState<'a, KeyType, ValueType>
where
    KeyType::PartialType: 'a,
{
    fn new(tree: &'a VersionedAdaptiveRadixTree<KeyType, ValueType>) -> Self {
        let mut iter = tree.iter();
        let current_entry = iter.next().map(|(k, v)| (k, v));

        Self {
            iter,
            current_entry,
        }
    }

    fn current_key(&self) -> Option<&KeyType> {
        self.current_entry.as_ref().map(|(k, _)| k)
    }

    fn current_value(&self) -> Option<&'a ValueType> {
        self.current_entry.as_ref().map(|(_, v)| *v)
    }

    fn advance(&mut self) {
        if let Some((key, value)) = self.iter.next() {
            self.current_entry = Some((key, value));
        } else {
            self.current_entry = None;
        }
    }
}

/// Streaming N-way merge join iterator for versioned trees
pub struct VersionedNWayMergeJoinIterator<'a, KeyType: KeyTrait, ValueType> {
    tree_states: Vec<VersionedTreeIterState<'a, KeyType, ValueType>>,
}

impl<'a, KeyType, ValueType> VersionedNWayMergeJoinIterator<'a, KeyType, ValueType>
where
    KeyType: KeyTrait + Clone + Ord,
    ValueType: Clone,
{
    fn new(trees: &'a [&'a VersionedAdaptiveRadixTree<KeyType, ValueType>]) -> Self {
        let tree_states = trees
            .iter()
            .map(|tree| VersionedTreeIterState::new(tree))
            .collect::<Vec<_>>();

        // If any tree is empty, the join result is empty
        if tree_states
            .iter()
            .any(|state| state.current_entry.is_none())
        {
            return Self {
                tree_states: Vec::new(),
            };
        }

        Self { tree_states }
    }
}

impl<'a, KeyType, ValueType> Iterator for VersionedNWayMergeJoinIterator<'a, KeyType, ValueType>
where
    KeyType: KeyTrait + Clone + Ord + 'a,
    KeyType::PartialType: 'a,
    ValueType: Clone,
{
    type Item = KeyType;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Check if any iterator is exhausted - if so, no more joins possible
            if self.tree_states.is_empty()
                || self
                    .tree_states
                    .iter()
                    .any(|state| state.current_entry.is_none())
            {
                return None;
            }

            // Find the minimum key across all current positions
            let min_key = self
                .tree_states
                .iter()
                .filter_map(|state| state.current_key())
                .min()
                .cloned()?;

            // Check if all trees have this minimum key
            let mut all_match = true;
            for state in &mut self.tree_states {
                if let Some(current_key) = state.current_key() {
                    if *current_key == min_key {
                        // This tree matches, advance it
                        state.advance();
                    } else if *current_key > min_key {
                        // This tree is ahead, we don't have a complete match
                        all_match = false;
                    }
                } else {
                    // Tree is exhausted
                    all_match = false;
                }
            }

            // Advance trees that had keys smaller than min_key (shouldn't happen in correct algorithm)
            for state in &mut self.tree_states {
                if let Some(current_key) = state.current_key() {
                    if *current_key < min_key {
                        state.advance();
                    }
                }
            }

            if all_match {
                return Some(min_key);
            }
        }
    }
}

/// Iterator that provides access to values from versioned trees
pub struct VersionedNWayMergeJoinWithValuesIterator<'a, KeyType: KeyTrait, ValueType> {
    tree_states: Vec<VersionedTreeIterState<'a, KeyType, ValueType>>,
}

impl<'a, KeyType, ValueType> VersionedNWayMergeJoinWithValuesIterator<'a, KeyType, ValueType>
where
    KeyType: KeyTrait + Clone + Ord + 'a,
    KeyType::PartialType: 'a,
    ValueType: Clone,
{
    fn new(trees: &'a [&'a VersionedAdaptiveRadixTree<KeyType, ValueType>]) -> Self {
        let tree_states = trees
            .iter()
            .map(|tree| VersionedTreeIterState::new(tree))
            .collect::<Vec<_>>();

        // If any tree is empty, the join result is empty
        if tree_states
            .iter()
            .any(|state| state.current_entry.is_none())
        {
            return Self {
                tree_states: Vec::new(),
            };
        }

        Self { tree_states }
    }
}

impl<'a, KeyType, ValueType> Iterator
    for VersionedNWayMergeJoinWithValuesIterator<'a, KeyType, ValueType>
where
    KeyType: KeyTrait + Clone + Ord + 'a,
    KeyType::PartialType: 'a,
    ValueType: Clone,
{
    type Item = (KeyType, Vec<&'a ValueType>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Check if any iterator is exhausted - if so, no more joins possible
            if self.tree_states.is_empty()
                || self
                    .tree_states
                    .iter()
                    .any(|state| state.current_entry.is_none())
            {
                return None;
            }

            // Find the minimum key across all current positions
            let min_key = self
                .tree_states
                .iter()
                .filter_map(|state| state.current_key())
                .min()
                .cloned()?;

            // Check if all trees have this minimum key
            let mut all_match = true;
            let mut values = Vec::new();

            // First pass: check if all trees have the min_key and collect values from matching trees
            for state in &self.tree_states {
                if let Some(current_key) = state.current_key() {
                    if *current_key == min_key {
                        if let Some(value) = state.current_value() {
                            values.push(value);
                        }
                    } else if *current_key > min_key {
                        // This tree is ahead, we don't have a complete match
                        all_match = false;
                    }
                } else {
                    // Tree is exhausted
                    all_match = false;
                }
            }

            // Second pass: advance all trees appropriately
            for state in &mut self.tree_states {
                if let Some(current_key) = state.current_key() {
                    if *current_key == min_key {
                        // This tree matches, advance it
                        state.advance();
                    } else if *current_key < min_key {
                        // This tree is behind, advance it
                        state.advance();
                    }
                    // Trees with current_key > min_key don't advance
                }
            }

            if all_match {
                return Some((min_key, values));
            }
        }
    }
}

/// Two-way merge join iterator optimized for the common 2-tree case
pub enum TwoWayMergeJoinIterator<'a, KeyType: KeyTrait, ValueType> {
    TwoWay {
        iter1: Iter<'a, KeyType, KeyType::PartialType, ValueType>,
        iter2: Iter<'a, KeyType, KeyType::PartialType, ValueType>,
        current1: Option<(KeyType, &'a ValueType)>,
        current2: Option<(KeyType, &'a ValueType)>,
    },
    Fallback(NWayMergeJoinIterator<'a, KeyType, ValueType>),
}

impl<'a, KeyType, ValueType> TwoWayMergeJoinIterator<'a, KeyType, ValueType>
where
    KeyType: KeyTrait + Clone + Ord,
{
    fn new(
        tree1: &'a AdaptiveRadixTree<KeyType, ValueType>,
        tree2: &'a AdaptiveRadixTree<KeyType, ValueType>,
    ) -> Self {
        let mut iter1 = tree1.iter();
        let mut iter2 = tree2.iter();
        let current1 = iter1.next();
        let current2 = iter2.next();

        Self::TwoWay {
            iter1,
            iter2,
            current1,
            current2,
        }
    }
}

impl<'a, KeyType, ValueType> Iterator for TwoWayMergeJoinIterator<'a, KeyType, ValueType>
where
    KeyType: KeyTrait + Clone + Ord,
{
    type Item = KeyType;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Fallback(nway) => nway.next(),
            Self::TwoWay {
                iter1,
                iter2,
                current1,
                current2,
            } => {
                loop {
                    // If either iterator is exhausted, we're done
                    let (key1, _val1) = current1.as_ref()?;
                    let (key2, _val2) = current2.as_ref()?;

                    match key1.cmp(key2) {
                        std::cmp::Ordering::Equal => {
                            // Found a match! Return it and advance both
                            let result = key1.clone();
                            *current1 = iter1.next();
                            *current2 = iter2.next();
                            return Some(result);
                        }
                        std::cmp::Ordering::Less => {
                            // tree1 is behind, advance it
                            *current1 = iter1.next();
                        }
                        std::cmp::Ordering::Greater => {
                            // tree2 is behind, advance it
                            *current2 = iter2.next();
                        }
                    }
                }
            }
        }
    }
}

/// Optimized merge join iterator that dispatches to specialized implementations
/// based on the number of trees at runtime
pub enum OptimizedMergeJoinIterator<'a, KeyType: KeyTrait, ValueType> {
    TwoWay(TwoWayMergeJoinIterator<'a, KeyType, ValueType>),
    ThreeWay(StaticMergeJoinIterator<'a, 3, KeyType, ValueType>),
    FourWay(StaticMergeJoinIterator<'a, 4, KeyType, ValueType>),
    Dynamic(NWayMergeJoinIterator<'a, KeyType, ValueType>),
}

impl<'a, KeyType, ValueType> OptimizedMergeJoinIterator<'a, KeyType, ValueType>
where
    KeyType: KeyTrait + Clone + Ord,
{
    fn new(trees: &'a [&'a AdaptiveRadixTree<KeyType, ValueType>]) -> Self {
        match trees.len() {
            2 => Self::TwoWay(TwoWayMergeJoinIterator::new(&trees[0], &trees[1])),
            3 => {
                Self::ThreeWay(StaticMergeJoinIterator::<3, KeyType, ValueType>::new_static(trees))
            }
            4 => Self::FourWay(StaticMergeJoinIterator::<4, KeyType, ValueType>::new_static(trees)),
            _ => Self::Dynamic(NWayMergeJoinIterator::new(trees)),
        }
    }
}

impl<'a, KeyType, ValueType> Iterator for OptimizedMergeJoinIterator<'a, KeyType, ValueType>
where
    KeyType: KeyTrait + Clone + Ord,
{
    type Item = KeyType;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::TwoWay(iter) => iter.next(),
            Self::ThreeWay(iter) => iter.next(),
            Self::FourWay(iter) => iter.next(),
            Self::Dynamic(iter) => iter.next(),
        }
    }
}

/// Const-generic static merge join iterator optimized for compile-time known number of trees
pub enum StaticMergeJoinIterator<'a, const N: usize, KeyType: KeyTrait, ValueType> {
    Static {
        iters: [Iter<'a, KeyType, KeyType::PartialType, ValueType>; N],
        current_entries: [Option<(KeyType, &'a ValueType)>; N],
    },
    Empty, // For when any input tree is empty
}

impl<'a, const N: usize, KeyType, ValueType> StaticMergeJoinIterator<'a, N, KeyType, ValueType>
where
    KeyType: KeyTrait + Clone + Ord,
{
    /// Create a static iterator for exactly N trees
    fn new_static(trees: &'a [&'a AdaptiveRadixTree<KeyType, ValueType>]) -> Self {
        assert_eq!(
            trees.len(),
            N,
            "StaticMergeJoinIterator requires exactly N trees"
        );

        // Create arrays for iterators and current entries
        let mut iters: [Option<Iter<'a, KeyType, KeyType::PartialType, ValueType>>; N] =
            [const { None }; N];
        let mut current_entries: [Option<(KeyType, &'a ValueType)>; N] = [const { None }; N];

        // Initialize all iterators and first entries
        for (i, tree) in trees.iter().enumerate() {
            let mut iter = tree.iter();
            let first_entry = iter.next();
            iters[i] = Some(iter);
            current_entries[i] = first_entry;
        }

        // Check if any tree is empty - if so, result is empty
        if current_entries.iter().any(|entry| entry.is_none()) {
            return Self::Empty;
        }

        // Convert Option arrays to concrete arrays
        let concrete_iters = iters.map(|opt| opt.unwrap());

        Self::Static {
            iters: concrete_iters,
            current_entries,
        }
    }
}

impl<'a, const N: usize, KeyType, ValueType> Iterator
    for StaticMergeJoinIterator<'a, N, KeyType, ValueType>
where
    KeyType: KeyTrait + Clone + Ord,
{
    type Item = KeyType;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty => None,
            Self::Static {
                iters,
                current_entries,
            } => {
                loop {
                    // Check if any iterator is exhausted
                    if current_entries.iter().any(|entry| entry.is_none()) {
                        return None;
                    }

                    // Find minimum key across all current positions
                    let min_key = current_entries
                        .iter()
                        .filter_map(|entry| entry.as_ref().map(|(k, _)| k))
                        .min()
                        .cloned()?;

                    // Check if all trees have this minimum key and advance matching ones
                    let mut all_match = true;

                    for i in 0..N {
                        if let Some((current_key, _)) = &current_entries[i] {
                            if *current_key == min_key {
                                // This tree matches, advance it
                                current_entries[i] = iters[i].next();
                            } else if *current_key > min_key {
                                // This tree is ahead, no complete match
                                all_match = false;
                            }
                        } else {
                            // Tree exhausted
                            all_match = false;
                        }
                    }

                    // Advance any trees behind the min_key
                    for i in 0..N {
                        if let Some((current_key, _)) = &current_entries[i] {
                            if *current_key < min_key {
                                current_entries[i] = iters[i].next();
                            }
                        }
                    }

                    if all_match {
                        return Some(min_key);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::array_key::ArrayKey;

    #[test]
    fn test_two_way_join() {
        let mut tree1 = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        let mut tree2 = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();

        // Common keys
        tree1.insert("apple", 1);
        tree1.insert("banana", 2);
        tree1.insert("cherry", 3);

        tree2.insert("apple", 10);
        tree2.insert("banana", 20);
        tree2.insert("date", 40);

        let trees = vec![&tree1, &tree2];
        let results: Vec<_> = AdaptiveRadixTree::merge_join_keys(&trees).collect();

        // Should find "apple" and "banana" as common keys
        assert_eq!(results.len(), 2);
        assert!(results.contains(&"apple".into()));
        assert!(results.contains(&"banana".into()));
    }

    #[test]
    fn test_three_way_join() {
        let mut tree1 = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        let mut tree2 = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        let mut tree3 = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();

        // Only "apple" is in all three trees
        tree1.insert("apple", 1);
        tree1.insert("banana", 2);

        tree2.insert("apple", 10);
        tree2.insert("cherry", 30);

        tree3.insert("apple", 100);
        tree3.insert("date", 400);

        let trees = vec![&tree1, &tree2, &tree3];
        let results: Vec<_> = AdaptiveRadixTree::merge_join_keys(&trees).collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "apple".into());
    }

    #[test]
    fn test_no_common_keys() {
        let mut tree1 = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        let mut tree2 = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();

        tree1.insert("apple", 1);
        tree2.insert("banana", 2);

        let trees = vec![&tree1, &tree2];
        let results: Vec<_> = AdaptiveRadixTree::merge_join_keys(&trees).collect();

        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_join_with_values() {
        let mut tree1 = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        let mut tree2 = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();

        tree1.insert("apple", 42);
        tree2.insert("apple", 99);

        let trees = vec![&tree1, &tree2];
        let results: Vec<_> = AdaptiveRadixTree::merge_join_with_values(&trees).collect();

        assert_eq!(results.len(), 1);
        let (key, values) = &results[0];
        assert_eq!(*key, "apple".into());
        assert_eq!(values.len(), 2);
        assert_eq!(*values[0], 42);
        assert_eq!(*values[1], 99);
    }

    #[test]
    fn test_empty_tree_join() {
        let mut tree1 = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        let tree2 = AdaptiveRadixTree::<ArrayKey<16>, i32>::new(); // empty

        tree1.insert("apple", 1);

        let trees = vec![&tree1, &tree2];
        let results: Vec<_> = AdaptiveRadixTree::merge_join_keys(&trees).collect();

        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_versioned_two_way_join() {
        let mut tree1 = VersionedAdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        let mut tree2 = VersionedAdaptiveRadixTree::<ArrayKey<16>, i32>::new();

        // Common keys
        tree1.insert("apple", 1);
        tree1.insert("banana", 2);
        tree1.insert("cherry", 3);

        tree2.insert("apple", 10);
        tree2.insert("banana", 20);
        tree2.insert("date", 40);

        let trees = vec![&tree1, &tree2];
        let results: Vec<_> = VersionedAdaptiveRadixTree::merge_join_keys(&trees).collect();

        // Should find "apple" and "banana" as common keys
        assert_eq!(results.len(), 2);
        assert!(results.contains(&"apple".into()));
        assert!(results.contains(&"banana".into()));
    }

    #[test]
    fn test_versioned_join_with_values() {
        let mut tree1 = VersionedAdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        let mut tree2 = VersionedAdaptiveRadixTree::<ArrayKey<16>, i32>::new();

        tree1.insert("apple", 42);
        tree2.insert("apple", 99);

        let trees = vec![&tree1, &tree2];
        let results: Vec<_> = VersionedAdaptiveRadixTree::merge_join_with_values(&trees).collect();

        assert_eq!(results.len(), 1);
        let (key, values) = &results[0];
        assert_eq!(*key, "apple".into());
        assert_eq!(values.len(), 2);
        assert_eq!(*values[0], 42);
        assert_eq!(*values[1], 99);
    }

    #[test]
    fn test_two_way_optimized_join() {
        let mut tree1 = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        let mut tree2 = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();

        // Common keys
        tree1.insert("apple", 1);
        tree1.insert("banana", 2);
        tree1.insert("cherry", 3);

        tree2.insert("apple", 10);
        tree2.insert("banana", 20);
        tree2.insert("date", 40);

        let trees = vec![&tree1, &tree2];
        let results: Vec<_> = AdaptiveRadixTree::merge_join_keys(&trees).collect();

        // Should find "apple" and "banana" as common keys
        assert_eq!(results.len(), 2);
        assert!(results.contains(&"apple".into()));
        assert!(results.contains(&"banana".into()));

        // Test 3-way join still works (uses fallback)
        let mut tree3 = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        tree3.insert("apple", 100);
        let trees_3way = vec![&tree1, &tree2, &tree3];
        let results_3way: Vec<_> = AdaptiveRadixTree::merge_join_keys(&trees_3way).collect();
        assert_eq!(results_3way.len(), 1);
        assert!(results_3way.contains(&"apple".into()));
    }

    // Property-based tests using proptest
    #[cfg(test)]
    mod proptests {
        use super::*;
        use proptest::prelude::*;
        use std::collections::BTreeSet;

        /// Generate a strategy for creating test trees with random string keys
        /// Returns just the keys, we'll build trees in the actual tests
        fn arb_tree_keys() -> impl Strategy<Value = Vec<String>> {
            prop::collection::vec("[a-z]{1,8}", 0..50)
        }

        /// Generate multiple key sets for N-way join testing
        fn arb_multiple_key_sets() -> impl Strategy<Value = Vec<Vec<String>>> {
            prop::collection::vec(arb_tree_keys(), 1..6) // 1-5 key sets
        }

        /// Helper to build a tree from keys
        fn build_tree(keys: &[String]) -> AdaptiveRadixTree<ArrayKey<32>, i32> {
            let mut tree = AdaptiveRadixTree::<ArrayKey<32>, i32>::new();
            for (i, key) in keys.iter().enumerate() {
                tree.insert(key.as_str(), i as i32);
            }
            tree
        }

        proptest! {
            /// Property: merge_join result should equal set intersection of input key sets
            #[test]
            fn prop_merge_join_equals_set_intersection(key_sets in arb_multiple_key_sets()) {
                if key_sets.is_empty() {
                    return Ok(()); // Skip empty case
                }

                // Build trees from key sets
                let trees: Vec<_> = key_sets.iter().map(|keys| build_tree(keys)).collect();
                let tree_refs: Vec<_> = trees.iter().collect();

                // Compute expected intersection using standard library
                let btree_sets: Vec<BTreeSet<String>> = key_sets.iter()
                    .map(|keys| keys.iter().cloned().collect())
                    .collect();

                let mut expected_intersection = btree_sets[0].clone();
                for key_set in btree_sets.iter().skip(1) {
                    expected_intersection = expected_intersection.intersection(key_set).cloned().collect();
                }

                // Compute actual result using our merge join
                let actual_result: BTreeSet<String> = AdaptiveRadixTree::merge_join_keys(&tree_refs)
                    .map(|key: ArrayKey<32>| {
                        // ArrayKey stores strings with null termination, need to trim
                        let bytes = key.as_ref();
                        let null_pos = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                        String::from_utf8_lossy(&bytes[..null_pos]).into_owned()
                    })
                    .collect();

                prop_assert_eq!(actual_result, expected_intersection);
            }

            /// Property: merge_join result should always be sorted
            #[test]
            fn prop_merge_join_result_is_sorted(key_sets in arb_multiple_key_sets()) {
                if key_sets.is_empty() {
                    return Ok(());
                }

                let trees: Vec<_> = key_sets.iter().map(|keys| build_tree(keys)).collect();
                let tree_refs: Vec<_> = trees.iter().collect();
                let results: Vec<ArrayKey<32>> = AdaptiveRadixTree::merge_join_keys(&tree_refs).collect();

                // Check that results are sorted
                for window in results.windows(2) {
                    prop_assert!(window[0] <= window[1], "Results should be sorted");
                }
            }

            /// Property: merge_join should have no duplicates
            #[test]
            fn prop_merge_join_no_duplicates(key_sets in arb_multiple_key_sets()) {
                if key_sets.is_empty() {
                    return Ok(());
                }

                let trees: Vec<_> = key_sets.iter().map(|keys| build_tree(keys)).collect();
                let tree_refs: Vec<_> = trees.iter().collect();
                let results: Vec<ArrayKey<32>> = AdaptiveRadixTree::merge_join_keys(&tree_refs).collect();
                let unique_results: BTreeSet<ArrayKey<32>> = results.iter().cloned().collect();

                prop_assert_eq!(results.len(), unique_results.len(), "No duplicates allowed");
            }

            /// Property: if any tree is empty, result should be empty
            #[test]
            fn prop_empty_tree_gives_empty_result(mut key_sets in arb_multiple_key_sets()) {
                if key_sets.is_empty() {
                    return Ok(());
                }

                // Make one key set empty
                key_sets[0] = vec![];

                let trees: Vec<_> = key_sets.iter().map(|keys| build_tree(keys)).collect();
                let tree_refs: Vec<_> = trees.iter().collect();
                let results: Vec<ArrayKey<32>> = AdaptiveRadixTree::merge_join_keys(&tree_refs).collect();

                prop_assert_eq!(results.len(), 0, "Empty tree should result in empty join");
            }

            /// Property: single tree join should return all its keys
            #[test]
            fn prop_single_tree_join_returns_all_keys(keys in arb_tree_keys()) {
                let tree = build_tree(&keys);
                let trees = vec![&tree];
                let results: BTreeSet<String> = AdaptiveRadixTree::merge_join_keys(&trees)
                    .map(|key: ArrayKey<32>| {
                        // ArrayKey stores strings with null termination, need to trim
                        let bytes = key.as_ref();
                        let null_pos = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                        String::from_utf8_lossy(&bytes[..null_pos]).into_owned()
                    })
                    .collect();

                let expected_set: BTreeSet<String> = keys.into_iter().collect();
                prop_assert_eq!(results, expected_set);
            }

            /// Property: join is associative for 3 trees
            /// merge_join(A, B, C) should equal merge_join(merge_join_result(A,B), C)
            #[test]
            fn prop_join_associativity(key_sets in prop::collection::vec(arb_tree_keys(), 3..=3)) {
                let [keys_a, keys_b, keys_c] = <[_; 3]>::try_from(key_sets).unwrap();
                let tree_a = build_tree(&keys_a);
                let tree_b = build_tree(&keys_b);
                let tree_c = build_tree(&keys_c);

                // Direct 3-way join
                let three_way: BTreeSet<ArrayKey<32>> = AdaptiveRadixTree::merge_join_keys(&vec![&tree_a, &tree_b, &tree_c])
                    .collect();

                // Two-step join: first A∩B, then result∩C
                let ab_intersection: BTreeSet<ArrayKey<32>> = AdaptiveRadixTree::merge_join_keys(&vec![&tree_a, &tree_b])
                    .collect();

                // Create temporary tree from AB intersection
                let mut temp_tree = AdaptiveRadixTree::<ArrayKey<32>, i32>::new();
                for key in ab_intersection {
                    temp_tree.insert_k(&key, 0);
                }

                let two_step: BTreeSet<ArrayKey<32>> = AdaptiveRadixTree::merge_join_keys(&vec![&temp_tree, &tree_c])
                    .collect();

                prop_assert_eq!(three_way, two_step, "Join should be associative");
            }

            /// Property: join with values should have same keys as key-only join
            #[test]
            fn prop_join_with_values_same_keys(key_sets in arb_multiple_key_sets()) {
                if key_sets.is_empty() {
                    return Ok(());
                }

                let trees: Vec<_> = key_sets.iter().map(|keys| build_tree(keys)).collect();
                let tree_refs: Vec<_> = trees.iter().collect();

                let keys_only: BTreeSet<ArrayKey<32>> = AdaptiveRadixTree::merge_join_keys(&tree_refs).collect();
                let with_values: BTreeSet<ArrayKey<32>> = AdaptiveRadixTree::merge_join_with_values(&tree_refs)
                    .map(|(key, _)| key)
                    .collect();

                prop_assert_eq!(keys_only, with_values, "Keys should be identical between join methods");
            }
        }

        use crate::versioned_tree::VersionedAdaptiveRadixTree;

        /// Helper to build a versioned tree from keys
        fn build_versioned_tree(keys: &[String]) -> VersionedAdaptiveRadixTree<ArrayKey<32>, i32> {
            let mut tree = VersionedAdaptiveRadixTree::<ArrayKey<32>, i32>::new();
            for (i, key) in keys.iter().enumerate() {
                tree.insert(key.as_str(), i as i32);
            }
            tree
        }

        proptest! {
            /// Property: versioned merge_join result should equal set intersection of input key sets
            #[test]
            fn prop_versioned_merge_join_equals_set_intersection(key_sets in arb_multiple_key_sets()) {
                if key_sets.is_empty() {
                    return Ok(()); // Skip empty case
                }

                // Build versioned trees from key sets
                let trees: Vec<_> = key_sets.iter().map(|keys| build_versioned_tree(keys)).collect();
                let tree_refs: Vec<_> = trees.iter().collect();

                // Compute expected intersection using standard library
                let btree_sets: Vec<BTreeSet<String>> = key_sets.iter()
                    .map(|keys| keys.iter().cloned().collect())
                    .collect();

                let mut expected_intersection = btree_sets[0].clone();
                for key_set in btree_sets.iter().skip(1) {
                    expected_intersection = expected_intersection.intersection(key_set).cloned().collect();
                }

                // Compute actual result using our versioned merge join
                let actual_result: BTreeSet<String> = VersionedAdaptiveRadixTree::merge_join_keys(&tree_refs)
                    .map(|key: ArrayKey<32>| {
                        // ArrayKey stores strings with null termination, need to trim
                        let bytes = key.as_ref();
                        let null_pos = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                        String::from_utf8_lossy(&bytes[..null_pos]).into_owned()
                    })
                    .collect();

                prop_assert_eq!(actual_result, expected_intersection);
            }

            /// Property: versioned join should match regular join for same data
            #[test]
            fn prop_versioned_join_matches_regular_join(key_sets in arb_multiple_key_sets()) {
                if key_sets.is_empty() {
                    return Ok(());
                }

                // Build both regular and versioned trees
                let regular_trees: Vec<_> = key_sets.iter().map(|keys| build_tree(keys)).collect();
                let versioned_trees: Vec<_> = key_sets.iter().map(|keys| build_versioned_tree(keys)).collect();

                let regular_refs: Vec<_> = regular_trees.iter().collect();
                let versioned_refs: Vec<_> = versioned_trees.iter().collect();

                // Compare results
                let regular_result: BTreeSet<String> = AdaptiveRadixTree::merge_join_keys(&regular_refs)
                    .map(|key: ArrayKey<32>| {
                        let bytes = key.as_ref();
                        let null_pos = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                        String::from_utf8_lossy(&bytes[..null_pos]).into_owned()
                    })
                    .collect();

                let versioned_result: BTreeSet<String> = VersionedAdaptiveRadixTree::merge_join_keys(&versioned_refs)
                    .map(|key: ArrayKey<32>| {
                        let bytes = key.as_ref();
                        let null_pos = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                        String::from_utf8_lossy(&bytes[..null_pos]).into_owned()
                    })
                    .collect();

                prop_assert_eq!(regular_result, versioned_result,
                    "Regular and versioned joins should produce identical results");
            }

            /// Property: versioned merge_join result should always be sorted
            #[test]
            fn prop_versioned_merge_join_result_is_sorted(key_sets in arb_multiple_key_sets()) {
                if key_sets.is_empty() {
                    return Ok(());
                }

                let trees: Vec<_> = key_sets.iter().map(|keys| build_versioned_tree(keys)).collect();
                let tree_refs: Vec<_> = trees.iter().collect();
                let results: Vec<ArrayKey<32>> = VersionedAdaptiveRadixTree::merge_join_keys(&tree_refs).collect();

                // Check that results are sorted
                for window in results.windows(2) {
                    prop_assert!(window[0] <= window[1], "Results should be sorted");
                }
            }

            /// Property: versioned merge_join should have no duplicates
            #[test]
            fn prop_versioned_merge_join_no_duplicates(key_sets in arb_multiple_key_sets()) {
                if key_sets.is_empty() {
                    return Ok(());
                }

                let trees: Vec<_> = key_sets.iter().map(|keys| build_versioned_tree(keys)).collect();
                let tree_refs: Vec<_> = trees.iter().collect();
                let results: Vec<ArrayKey<32>> = VersionedAdaptiveRadixTree::merge_join_keys(&tree_refs).collect();
                let unique_results: BTreeSet<ArrayKey<32>> = results.iter().cloned().collect();

                prop_assert_eq!(results.len(), unique_results.len(), "No duplicates allowed");
            }

            /// Property: if any versioned tree is empty, result should be empty
            #[test]
            fn prop_versioned_empty_tree_gives_empty_result(mut key_sets in arb_multiple_key_sets()) {
                if key_sets.is_empty() {
                    return Ok(());
                }

                // Make one key set empty
                key_sets[0] = vec![];

                let trees: Vec<_> = key_sets.iter().map(|keys| build_versioned_tree(keys)).collect();
                let tree_refs: Vec<_> = trees.iter().collect();
                let results: Vec<ArrayKey<32>> = VersionedAdaptiveRadixTree::merge_join_keys(&tree_refs).collect();

                prop_assert_eq!(results.len(), 0, "Empty tree should result in empty join");
            }

            /// Property: single versioned tree join should return all its keys
            #[test]
            fn prop_versioned_single_tree_join_returns_all_keys(keys in arb_tree_keys()) {
                let tree = build_versioned_tree(&keys);
                let trees = vec![&tree];
                let results: BTreeSet<String> = VersionedAdaptiveRadixTree::merge_join_keys(&trees)
                    .map(|key: ArrayKey<32>| {
                        let bytes = key.as_ref();
                        let null_pos = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                        String::from_utf8_lossy(&bytes[..null_pos]).into_owned()
                    })
                    .collect();

                let expected_set: BTreeSet<String> = keys.into_iter().collect();
                prop_assert_eq!(results, expected_set);
            }

            /// Property: versioned join with values should have same keys as key-only join
            #[test]
            fn prop_versioned_join_with_values_same_keys(key_sets in arb_multiple_key_sets()) {
                if key_sets.is_empty() {
                    return Ok(());
                }

                let trees: Vec<_> = key_sets.iter().map(|keys| build_versioned_tree(keys)).collect();
                let tree_refs: Vec<_> = trees.iter().collect();

                let keys_only: BTreeSet<ArrayKey<32>> = VersionedAdaptiveRadixTree::merge_join_keys(&tree_refs).collect();
                let with_values: BTreeSet<ArrayKey<32>> = VersionedAdaptiveRadixTree::merge_join_with_values(&tree_refs)
                    .map(|(key, _)| key)
                    .collect();

                prop_assert_eq!(keys_only, with_values, "Keys should be identical between versioned join methods");
            }
        }
    }
}
