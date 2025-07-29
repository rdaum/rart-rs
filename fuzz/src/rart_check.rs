#![no_main]

use std::collections::HashMap;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use rart::keys::array_key::ArrayKey;

use rart::tree::AdaptiveRadixTree;

#[derive(Arbitrary, Debug)]
enum MapMethod {
    Get { key: usize },
    Insert { key: usize, val: usize },
    Update { key: usize, val: usize },
    Delete { key: usize },
}

fuzz_target!(|methods: Vec<MapMethod>| {
    let mut art = AdaptiveRadixTree::<ArrayKey<16>, usize>::new();
    let mut bt_map = HashMap::<usize, usize>::new();

    for m in methods {
        match m {
            MapMethod::Get { key } => {
                let art_v = art.get(key).copied();
                let bt_v = bt_map.get(&key).copied();
                assert_eq!(art_v, bt_v);
            }
            MapMethod::Insert { key, val } => {
                let btree_insert = bt_map.insert(key, val);
                let a_insert = art.insert(key, val);
                assert_eq!(a_insert, btree_insert);
            }
            MapMethod::Update { key, val } => {
                let old_bt = bt_map.get_mut(&key);
                let old_art = art.get_mut(key);
                assert_eq!(old_art, old_bt);

                if let Some(old_bt) = old_bt {
                    *old_bt = val;
                    *old_art.unwrap() = val;

                    let new_bt = bt_map.get(&key);
                    let new_art = art.get(key);
                    assert_eq!(new_art, new_bt);
                }
            }
            MapMethod::Delete { key } => {
                let bt_result = bt_map.remove(&key);
                let art_result = art.remove(key);
                assert_eq!(bt_result, art_result);
            }
        }
    }

    for (k, expected_value) in bt_map.iter() {
        let result = art.get(k);
        assert_eq!(
            result,
            Some(expected_value),
            "Expected value for key {}: {:?} != {:?}, got {:?}",
            k,
            art.get(k).copied(),
            *expected_value,
            result
        );
    }
});
