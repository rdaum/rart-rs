use rart::{AdaptiveRadixTree, ArrayKey, OverflowKey, VectorKey};

fn main() {
    fixed_width_numeric_keys();
    heap_backed_dynamic_keys();
    mostly_short_dynamic_keys();
}

fn fixed_width_numeric_keys() {
    let mut tree = AdaptiveRadixTree::<ArrayKey<8>, &'static str>::new();

    tree.insert(7u64, "seven");
    tree.insert(42u64, "forty two");

    assert_eq!(tree.get(7u64), Some(&"seven"));
    assert_eq!(tree.get(42u64), Some(&"forty two"));
}

fn heap_backed_dynamic_keys() {
    let mut tree = AdaptiveRadixTree::<VectorKey, usize>::new();

    tree.insert("short", 1);
    tree.insert("a much longer key than the fixed examples", 2);

    assert_eq!(tree.get("short"), Some(&1));
    assert_eq!(
        tree.get("a much longer key than the fixed examples"),
        Some(&2)
    );
}

fn mostly_short_dynamic_keys() {
    type Key = OverflowKey<32, 8>;

    let mut tree = AdaptiveRadixTree::<Key, usize>::new();

    tree.insert("tenant:a:account:1", 10);
    tree.insert("tenant:a:account:2", 20);
    tree.insert(
        "tenant:with-a-longer-name-than-inline-storage:account:3",
        30,
    );

    assert_eq!(tree.get("tenant:a:account:1"), Some(&10));
    assert_eq!(
        tree.get("tenant:with-a-longer-name-than-inline-storage:account:3"),
        Some(&30)
    );
}
