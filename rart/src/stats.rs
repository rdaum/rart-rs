//! Statistics and introspection for RART.
//!
//! This module provides functionality to gather statistics about the internal
//! structure and performance characteristics of Adaptive Radix Trees.
//!
//! Statistics can be useful for:
//! - Performance analysis and optimization
//! - Understanding memory usage patterns  
//! - Debugging tree structure issues
//! - Academic research and benchmarking

use crate::node::Node;
use crate::partials::Partial;
use std::collections::HashMap;

pub trait TreeStatsTrait {
    fn get_tree_stats(&self) -> TreeStats;
}

#[derive(Debug)]
pub struct NodeStats {
    pub width: usize,
    pub total_nodes: usize,
    pub total_children: usize,
    pub density: f64,
}

impl Default for NodeStats {
    fn default() -> Self {
        Self {
            width: 0,
            total_nodes: 0,
            total_children: 0,
            density: 0.0,
        }
    }
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

impl Default for TreeStats {
    fn default() -> Self {
        Self {
            node_stats: Default::default(),
            num_leaves: 0,
            num_values: 0,
            num_inner_nodes: 0,
            total_density: 0.0,
            max_height: 0,
        }
    }
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
