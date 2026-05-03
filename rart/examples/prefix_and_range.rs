use rart::keys::KeyTrait;
use rart::{AdaptiveRadixTree, VectorKey};

fn main() {
    let mut tree = AdaptiveRadixTree::<VectorKey, &'static str>::new();

    for (key, value) in [
        (b"app".as_slice(), "root"),
        (b"apple".as_slice(), "fruit"),
        (b"application".as_slice(), "software"),
        (b"banana".as_slice(), "fruit"),
        (b"band".as_slice(), "music"),
    ] {
        tree.insert_k(&key_bytes(key), value);
    }

    let app_values: Vec<_> = tree
        .prefix_iter_k(&key_bytes(b"app"))
        .map(|(key, value)| (display_key(key.as_ref()), *value))
        .collect();
    assert_eq!(
        app_values,
        vec![
            ("app".to_string(), "root"),
            ("apple".to_string(), "fruit"),
            ("application".to_string(), "software"),
        ]
    );

    let ranged_values: Vec<_> = tree
        .range(key_bytes(b"banana")..=key_bytes(b"band"))
        .map(|(key, value)| (display_key(key.as_ref()), *value))
        .collect();
    assert_eq!(
        ranged_values,
        vec![
            ("banana".to_string(), "fruit"),
            ("band".to_string(), "music"),
        ]
    );

    let (matched_key, matched_value) = tree
        .longest_prefix_match_k(&key_bytes(b"application/json"))
        .unwrap();
    assert_eq!(display_key(matched_key.as_ref()), "application");
    assert_eq!(*matched_value, "software");
}

fn key_bytes(bytes: &[u8]) -> VectorKey {
    VectorKey::new_from_slice(bytes)
}

fn display_key(bytes: &[u8]) -> String {
    String::from_utf8_lossy(trim_trailing_nul(bytes)).into_owned()
}

fn trim_trailing_nul(bytes: &[u8]) -> &[u8] {
    bytes.strip_suffix(&[0]).unwrap_or(bytes)
}
