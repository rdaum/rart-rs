use std::mem::MaybeUninit;
use std::ops::{Index, IndexMut};

// We use a u32 here instead of usize under the assumption there simply won't be that many entries
// and so that we can save some bytes in structs that use these indices.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FVIndex(pub u32);

/// A place to store (owned) values that can be accessed by an index, with holes being re-used.
/// Maintains a separate free list. A poor man's slot map or arena, really, but designed to allocate
/// and access fast.
pub struct FillVector<V> {
    values: Vec<MaybeUninit<V>>,
    free_list: Vec<u32>,
    size: usize,
}

impl<V> FillVector<V> {
    pub fn new() -> Self {
        Self {
            values: vec![],
            free_list: Vec::with_capacity(16),
            size: 0,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            values: Vec::with_capacity(capacity),
            free_list: Default::default(),
            size: 0,
        }
    }

    pub fn add<F: FnOnce(FVIndex) -> V>(&mut self, f: F) -> FVIndex {
        let id = match self.free_list.pop() {
            None => {
                let id = FVIndex(self.values.len() as u32);
                self.values.push(MaybeUninit::new(f(id)));
                id
            }
            Some(idx) => {
                let id = FVIndex(idx);
                self.values[idx as usize] = MaybeUninit::new(f(id));
                id
            }
        };
        self.size += 1;
        id
    }

    pub fn free(&mut self, id: FVIndex) -> bool {
        let idx = id.0 as usize;
        assert!(idx < self.values.len());
        // If this index is already free, return false.
        // To know that we check to see if the item is in the free list, which is O(n), or if the
        // item is off the end of the vector, which is O(1).
        // A very small performance improvement can be made by skipping this check, so it's up to
        // you if you want to use this or free_unchecked.
        // Even with this check, this is still faster than using a Vec<Option<V>>.
        if idx >= self.values.len() || self.free_list.contains(&id.0) {
            return false;
        }
        self.free_unchecked(id);
        true
    }

    pub fn get(&self, id: &FVIndex) -> Option<&V> {
        let idx = id.0 as usize;
        assert!(idx < self.values.len());
        if idx >= self.values.len() || self.free_list.contains(&id.0) {
            None
        } else {
            Some(unsafe { self.values[idx].assume_init_ref() })
        }
    }

    pub fn get_mut(&mut self, idx: &FVIndex) -> Option<&mut V> {
        let i = idx.0 as usize;
        assert!(i < self.values.len());
        if i >= self.values.len() || self.free_list.contains(&idx.0) {
            None
        } else {
            Some(unsafe { self.values[i].assume_init_mut() })
        }
    }

    pub fn swap(&mut self, left: &FVIndex, right: &FVIndex) {
        assert!(left.0 < self.values.len() as u32);
        assert!(right.0 < self.values.len() as u32);
        self.values.swap(left.0 as usize, right.0 as usize);
    }

    pub fn free_unchecked(&mut self, id: FVIndex) {
        assert!(id.0 < self.values.len() as u32);

        let idx = id.0 as usize;
        if idx == self.values.len() - 1 {
            unsafe {
                self.values.last_mut().unwrap().assume_init_drop();
            }
            self.values.pop();
        } else {
            unsafe {
                self.values[idx].assume_init_drop();
            }
            self.free_list.push(id.0);
        }
        self.size -= 1;
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn size(&self) -> usize {
        self.size
    }
}

impl<V> Index<FVIndex> for FillVector<V> {
    type Output = V;

    fn index(&self, index: FVIndex) -> &Self::Output {
        assert!(index.0 < self.values.len() as u32);
        unsafe { self.values[index.0 as usize].assume_init_ref() }
    }
}

impl<V> IndexMut<FVIndex> for FillVector<V> {
    fn index_mut(&mut self, index: FVIndex) -> &mut Self::Output {
        assert!(index.0 < self.values.len() as u32);
        unsafe { self.values[index.0 as usize].assume_init_mut() }
    }
}

impl<V> Index<usize> for FillVector<V> {
    type Output = V;

    fn index(&self, index: usize) -> &Self::Output {
        assert!(index < self.values.len());
        unsafe { self.values[index].assume_init_ref() }
    }
}

impl<V> IndexMut<usize> for FillVector<V> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        assert!(index < self.values.len());
        unsafe { self.values[index].assume_init_mut() }
    }
}

impl<V> Default for FillVector<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> Drop for FillVector<V> {
    fn drop(&mut self) {
        for (i, v) in self.values.iter_mut().enumerate() {
            if !self.free_list.is_empty() && self.free_list.contains(&(i as u32)) {
                continue;
            }
            unsafe {
                v.assume_init_drop();
            }
        }
        self.values.clear();
    }
}
