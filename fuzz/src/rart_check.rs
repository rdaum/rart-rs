#![no_main]

use std::collections::HashMap;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use rart::keys::array_key::ArrayKey;
use rart::partials::array_partial::ArrPartial;
use rart::tree::AdaptiveRadixTree;

#[derive(Arbitrary, Debug)]
enum MapMethod {
    Get { key: usize },
    Insert { key: usize, val: usize },
    Update { key: usize, val: usize },
    Delete { key: usize },
}

fuzz_target!(|methods: Vec<MapMethod>| {
    let mut art = AdaptiveRadixTree::<ArrPartial<16>, usize>::new();
    let mut bt_map = HashMap::<usize, usize>::new();

    for m in methods {
        match m {
            MapMethod::Get { key } => {
                let art_key: ArrayKey<16> = ArrayKey::new_from_unsigned(key);
                let art_v = art.get(&art_key).copied();
                let bt_v = bt_map.get(&key).copied();
                assert_eq!(art_v, bt_v);
            }
            MapMethod::Insert { key, val } => {
                let art_key: ArrayKey<16> = ArrayKey::new_from_unsigned(key);

                let btree_insert = bt_map.insert(key, val);
                let a_insert = art.insert(&art_key, val);
                assert_eq!(a_insert, btree_insert);
            }
            MapMethod::Update { key, val } => {
                let old_bt = bt_map.get_mut(&key);
                let art_key: ArrayKey<16> = ArrayKey::new_from_unsigned(key);
                let old_art = art.get_mut(&art_key);
                assert_eq!(old_art, old_bt);

                if let Some(old_bt) = old_bt {
                    *old_bt = val;
                    *old_art.unwrap() = val;

                    let new_bt = bt_map.get(&key);
                    let new_art = art.get(&art_key);
                    assert_eq!(new_art, new_bt);
                }
            }
            MapMethod::Delete { key } => {
                let bt_result = bt_map.remove(&key);
                let art_key: ArrayKey<16> = ArrayKey::new_from_unsigned(key);
                let art_result = art.remove(&art_key);
                assert_eq!(bt_result, art_result);
            }
        }
    }

    for (k, expected_value) in bt_map.iter() {
        let art_key: ArrayKey<16> = ArrayKey::new_from_unsigned(*k);
        let result = art.get(&art_key);
        assert_eq!(
            result,
            Some(expected_value),
            "Expected value for key {}: {:?} != {:?}, got {:?}",
            k,
            art.get(&art_key).copied(),
            *expected_value,
            result
        );
    }
});
