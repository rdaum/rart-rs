use rart::partials::array_partial::ArrPartial;
use rart::{AdaptiveRadixTree, ArrayKey, Partial};

fn main() {
    let mut art = AdaptiveRadixTree::<ArrayKey<16>, usize>::new();

    // Test modern From/Into usage
    let key1: ArrayKey<16> = "hello".into();
    let key2: ArrayKey<16> = "world".into();

    art.insert(key1, 42);
    art.insert(key2, 84);

    // Test AsRef trait
    println!("key1 as slice: {:?}", key1.as_ref());
    println!("key2 as slice: {:?}", key2.as_ref());

    // Test partial operations with new ergonomic traits
    let partial: ArrPartial<16> = "test".as_bytes().into();
    println!("partial as slice: {:?}", partial.as_ref());
    println!("partial length: {}", partial.len());
    println!("partial is empty: {}", partial.is_empty());

    // Test new convenience methods
    println!("partial starts with 'te': {}", partial.starts_with(b"te"));
    println!("partial ends with 'st': {}", partial.ends_with(b"st"));

    // Test iterator support
    println!("partial bytes:");
    for (i, &byte) in (&partial).into_iter().enumerate() {
        println!("  [{}]: {}", i, byte as char);
    }

    // Or using the iter method
    println!("using iter method:");
    for (i, &byte) in partial.iter().enumerate() {
        println!("  [{}]: {}", i, byte as char);
    }

    println!("All tests passed! Idiomatic interface working correctly.");
}
