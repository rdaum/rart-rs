use crate::keys::KeyTrait;
use crate::node::{DefaultNode, Node};
use crate::partials::Partial;

type IterEntry<'a, P, V> = (u8, &'a DefaultNode<P, V>);
type NodeIterator<'a, P, V> = dyn Iterator<Item = IterEntry<'a, P, V>> + 'a;

pub struct Iter<'a, K: KeyTrait<PartialType = P>, P: Partial + 'a, V> {
    inner: Box<dyn Iterator<Item = (K, &'a V)> + 'a>,
    _marker: std::marker::PhantomData<(K, P)>,
}

struct IterInner<'a, K: KeyTrait<PartialType = P>, P: Partial + 'a, V> {
    node_iter_stack: Vec<(usize, Box<NodeIterator<'a, P, V>>)>,

    // Pushed and popped with prefix portions as we descend the tree,
    cur_key: K,
}

impl<'a, K: KeyTrait<PartialType = P>, P: Partial + 'a, V> IterInner<'a, K, P, V> {
    pub fn new(node: &'a DefaultNode<P, V>) -> Self {
        let node_iter_stack = vec![(
            node.prefix.len(), /* initial tree depth*/
            node.iter(),       /* root node iter*/
        )];

        Self {
            node_iter_stack,
            cur_key: K::new_from_partial(&node.prefix),
        }
    }
}

impl<'a, K: KeyTrait<PartialType = P> + 'a, P: Partial + 'a, V> Iter<'a, K, P, V> {
    pub(crate) fn new(node: Option<&'a DefaultNode<P, V>>) -> Self {
        let Some(root_node) = node else {
            return Self {
                inner: Box::new(std::iter::empty()),
                _marker: Default::default(),
            };
        };

        // If root is a leaf, we can just return it.
        if root_node.is_leaf() {
            let root_key = K::new_from_partial(&root_node.prefix);
            let root_value = root_node
                .value()
                .expect("corruption: missing data at leaf node during iteration");
            return Self {
                inner: Box::new(std::iter::once((root_key, root_value))),
                _marker: Default::default(),
            };
        }

        Self {
            inner: Box::new(IterInner::<K, P, V>::new(root_node)),
            _marker: Default::default(),
        }
    }
}

impl<'a, K: KeyTrait<PartialType = P>, P: Partial + 'a, V> Iterator for Iter<'a, K, P, V> {
    type Item = (K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'a, K: KeyTrait<PartialType = P>, P: Partial + 'a, V> Iterator for IterInner<'a, K, P, V> {
    type Item = (K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Get working node iterator off the stack. If there is none, we're done.
            let Some((tree_depth, last_iter)) = self.node_iter_stack.last_mut() else {
                return None;
            };
            let tree_depth = *tree_depth;

            // Pull the next node from the node iterator. If there's none, pop that iterator off
            // the stack, truncate our working key length back to the parent's depth, return to our
            // parent, and continue there.
            let Some((_k, node)) = last_iter.next() else {
                let _ = self.node_iter_stack.pop().unwrap();
                // Get the parent-depth, and truncate our working key to that depth. If there is no
                // parent, no need to truncate, we'll be done in the next loop
                if let Some((parent_depth, _)) = self.node_iter_stack.last() {
                    self.cur_key = self.cur_key.truncate(*parent_depth);
                };
                continue;
            };

            // We're at a non-exhausted inner node, so go further down the tree by pushing node
            // iterator into the stack. We also extend our working key with this node's prefix.
            if node.is_inner() {
                self.node_iter_stack
                    .push((tree_depth + node.prefix.len(), node.iter()));
                self.cur_key = self.cur_key.extend_from_partial(&node.prefix);
                continue;
            }

            // We've got a value, so tack it onto our working key, and return it. If there's nothing
            // here, that's an issue, leaf nodes should always have values.
            let v = node
                .value()
                .expect("corruption: missing data at leaf node during iteration");
            let key = self.cur_key.extend_from_partial(&node.prefix);
            return Some((key, v));
        }
    }
}
