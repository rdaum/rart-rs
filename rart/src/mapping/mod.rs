pub mod direct_mapping;
pub mod indexed_mapping;
pub mod keyed_mapping;
pub mod multilevel_node4;
pub mod multilevel_node8;
pub mod sorted_keyed_mapping;

pub trait NodeMapping<N, const NUM_CHILDREN: usize> {
    const NUM_CHILDREN: usize = NUM_CHILDREN;

    fn add_child(&mut self, key: u8, node: N);
    fn seek_child(&self, key: u8) -> Option<&N>;
    fn seek_child_mut(&mut self, key: u8) -> Option<&mut N>;
    fn delete_child(&mut self, key: u8) -> Option<N>;
    fn num_children(&self) -> usize;
    fn width(&self) -> usize {
        Self::NUM_CHILDREN
    }
}
