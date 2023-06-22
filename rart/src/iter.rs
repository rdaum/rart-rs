use crate::node::Node;
use crate::tree::PrefixTraits;

type IterEntry<'a, P, V> = (u8, &'a Node<P, V>);
type NodeIterator<'a, P, V> = dyn Iterator<Item = IterEntry<'a, P, V>> + 'a;

pub struct Iter<'a, P: PrefixTraits + 'a, V> {
    inner: Box<dyn Iterator<Item = (Vec<u8>, &'a V)> + 'a>,
    _marker: std::marker::PhantomData<P>,
}

struct IterInner<'a, P: PrefixTraits + 'a, V> {
    node_iter_stack: Vec<Box<NodeIterator<'a, P, V>>>,

    // Pushed and popped with prefix portions as we descend the tree,
    cur_key: Vec<u8>,
    cur_prefix_length: usize,
}

impl<'a, P: PrefixTraits + 'a, V> IterInner<'a, P, V> {
    pub fn new(node: &'a Node<P, V>) -> Self {
        let node_iter_stack = vec![node.iter()];

        Self {
            node_iter_stack,
            cur_key: Vec::new(),
            cur_prefix_length: 0,
        }
    }
}

impl<'a, P: PrefixTraits + 'a, V> Iter<'a, P, V> {
    pub(crate) fn new(node: Option<&'a Node<P, V>>) -> Self {
        if node.is_none() {
            return Self {
                inner: Box::new(std::iter::empty()),
                _marker: Default::default(),
            };
        }

        Self {
            inner: Box::new(IterInner::new(node.unwrap())),
            _marker: Default::default(),
        }
    }
}

impl<'a, P: PrefixTraits + 'a, V> Iterator for Iter<'a, P, V> {
    type Item = (Vec<u8>, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'a, P: PrefixTraits + 'a, V> Iterator for IterInner<'a, P, V> {
    type Item = (Vec<u8>, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        // Grab the last iterator from the stack, and see if there's more to iterate off of it.
        // If not, pop it off, and continue the loop.
        // If there is, loop through it looking for nodes; if the node is a leaf, return its value.
        // If it's a node, grab its child iterator, and push it onto the stack and continue the loop.
        loop {
            let Some(last_iter) = self.node_iter_stack.last_mut() else {
                return None;
            };

            let Some((_k, node)) = last_iter.next() else {
                self.node_iter_stack.pop();
                self.cur_key.truncate(self.cur_prefix_length);
                continue;
            };

            if let Some(v) = node.value() {
                let mut key = self.cur_key.clone();
                key.extend_from_slice(node.prefix.to_slice());
                return Some((key, v));
            }
        }
    }
}
