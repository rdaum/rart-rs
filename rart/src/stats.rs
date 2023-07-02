use crate::node::Node;
use crate::partials::Partial;
use std::collections::HashMap;

#[derive(Debug)]
pub struct NodeStats {
    pub width: usize,
    pub total_nodes: usize,
    pub total_children: usize,
    pub density: f64,
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

pub(crate) fn update_tree_stats<NodeType, PartialType, ValueType>(
    tree_stats: &mut TreeStats,
    node: &NodeType,
) where
    NodeType: Node<PartialType, ValueType>,
    PartialType: Partial,
{
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
