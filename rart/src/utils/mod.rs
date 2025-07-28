use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::MutexGuard;

pub mod bitarray;
pub mod bitset;
pub mod u8_keys;

pub type PhantomUnsync = PhantomData<Cell<()>>;
pub type PhantomUnsend = PhantomData<MutexGuard<'static, ()>>;
