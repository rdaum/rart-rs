use rart::{ArrayKey, VersionedAdaptiveRadixTree};

fn main() {
    let mut live = VersionedAdaptiveRadixTree::<ArrayKey<16>, String>::new();

    live.insert("alpha", "v1".to_string());
    live.insert("bravo", "v1".to_string());

    let snapshot = live.snapshot();

    assert_eq!(
        live.insert_and_replace("alpha", "v2".to_string()),
        Some("v1".to_string())
    );
    assert_eq!(live.remove("bravo"), Some("v1".to_string()));
    live.insert("charlie", "v2".to_string());

    assert_eq!(live.get("alpha"), Some(&"v2".to_string()));
    assert_eq!(live.get("bravo"), None);
    assert_eq!(live.get("charlie"), Some(&"v2".to_string()));

    assert_eq!(snapshot.get("alpha"), Some(&"v1".to_string()));
    assert_eq!(snapshot.get("bravo"), Some(&"v1".to_string()));
    assert_eq!(snapshot.get("charlie"), None);

    let unversioned = live.into_unversioned();
    assert_eq!(unversioned.get("alpha"), Some(&"v2".to_string()));
}
