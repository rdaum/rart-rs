pub mod direct_mapping;
pub mod indexed_mapping;
pub mod keyed_mapping;
pub mod sorted_keyed_mapping;

pub trait NodeMapping<N, const NUM_CHILDREN: usize> {
    fn add_child(&mut self, key: u8, node: N);
    fn update_child(&mut self, key: u8, node: N);
    fn seek_child(&self, key: u8) -> Option<&N>;
    fn seek_child_mut(&mut self, key: u8) -> Option<&mut N>;
    fn delete_child(&mut self, key: u8) -> Option<N>;
    fn num_children(&self) -> usize;
    fn width(&self) -> usize {
        NUM_CHILDREN
    }
}
