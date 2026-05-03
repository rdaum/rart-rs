use rart::{AdaptiveRadixTree, ArrayKey};

fn main() {
    let tree = build_from_sorted_items();
    assert_eq!(tree.get("alpha"), Some(&10));
    assert_eq!(tree.get("bravo"), Some(&20));

    let (left, right) = overlapping_trees();
    assert_eq!(left.intersect_count(&right), 2);

    let mut pairs = Vec::new();
    left.intersect_values_with(&right, |left_value, right_value| {
        pairs.push((*left_value, *right_value));
    });
    assert_eq!(pairs, vec![(2, 20), (3, 30)]);
}

fn build_from_sorted_items() -> AdaptiveRadixTree<ArrayKey<16>, i32> {
    let items = vec![
        ("alpha".into(), 1),
        ("alpha".into(), 10),
        ("bravo".into(), 20),
    ];
    AdaptiveRadixTree::bulk_load_sorted(items)
}

fn overlapping_trees() -> (
    AdaptiveRadixTree<ArrayKey<16>, i32>,
    AdaptiveRadixTree<ArrayKey<16>, i32>,
) {
    let mut left = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
    left.insert("alpha", 1);
    left.insert("bravo", 2);
    left.insert("charlie", 3);

    let mut right = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
    right.insert("bravo", 20);
    right.insert("charlie", 30);
    right.insert("delta", 40);

    (left, right)
}
