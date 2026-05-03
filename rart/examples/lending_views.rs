use rart::keys::KeyTrait;
use rart::{AdaptiveRadixTree, OverflowKey};

type Key = OverflowKey<32, 8>;

fn main() {
    let mut tree = AdaptiveRadixTree::<Key, usize>::new();

    for (idx, key) in [
        b"tenant:a:account:1".as_slice(),
        b"tenant:a:account:2".as_slice(),
        b"tenant:b:account:1".as_slice(),
    ]
    .into_iter()
    .enumerate()
    {
        tree.insert_k(&Key::new_from_slice(key), idx);
    }

    let mut all_keys = Vec::new();
    tree.for_each_view(|key, value| {
        all_keys.push((display_key(&key.to_vec()), *value));
    });
    assert_eq!(
        all_keys,
        vec![
            ("tenant:a:account:1".to_string(), 0),
            ("tenant:a:account:2".to_string(), 1),
            ("tenant:b:account:1".to_string(), 2),
        ]
    );

    let mut tenant_a_values = Vec::new();
    tree.prefix_for_each_view_k(&Key::new_from_slice(b"tenant:a"), |key, value| {
        tenant_a_values.push((display_key(&key.to_vec()), *value));
    });
    assert_eq!(
        tenant_a_values,
        vec![
            ("tenant:a:account:1".to_string(), 0),
            ("tenant:a:account:2".to_string(), 1),
        ]
    );

    let found = tree.with_longest_prefix_match_view_k(
        &Key::new_from_slice(b"tenant:a:account:2:settings"),
        |key, value| {
            assert_eq!(display_key(&key.to_vec()), "tenant:a:account:2");
            assert_eq!(*value, 1);
        },
    );
    assert!(found);
}

fn display_key(bytes: &[u8]) -> String {
    String::from_utf8_lossy(trim_trailing_nul(bytes)).into_owned()
}

fn trim_trailing_nul(bytes: &[u8]) -> &[u8] {
    bytes.strip_suffix(&[0]).unwrap_or(bytes)
}
