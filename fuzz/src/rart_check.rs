#![no_main]

use std::collections::BTreeMap;

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
    let capacity = 10_000_000;
    let mut art = AdaptiveRadixTree::<ArrPartial<16>, usize>::new();
    let mut bt_map = BTreeMap::<usize, usize>::new();

    for m_c in methods.chunks(1024) {
        for m in m_c {
            match m {
                MapMethod::Get { key } => {
                    let art_key: ArrayKey<16> = ArrayKey::from(*key);
                    let art_v = art.get(&art_key).copied();
                    let bt_v = bt_map.get(key).copied();
                    assert_eq!(art_v, bt_v);
                }
                MapMethod::Insert { key, val } => {
                    if bt_map.len() < capacity {
                        let art_key: ArrayKey<16> = ArrayKey::from(*key);

                        let btree_insert = bt_map.insert(*key, *val);
                        let a_insert = art.insert(&art_key, *val);
                        eprintln!("Insert: {:?} {:?} {:?}", key, val, a_insert);
                        assert_eq!(a_insert, btree_insert);
                    }
                }
                MapMethod::Update { key, val } => {
                    let old_bt = bt_map.get_mut(key);
                    let art_key: ArrayKey<16> = ArrayKey::from(*key);
                    let old_art = art.get_mut(&art_key);
                    assert_eq!(old_art, old_bt);

                    if let Some(old_bt) = old_bt {
                        *old_bt = *val;
                        *old_art.unwrap() = *val;
                    }

                    let new_bt = bt_map.get(key);
                    let new_art = art.get(&art_key);
                    eprintln!("Update: {:?} {:?} {:?}", key, val, new_art);
                    assert_eq!(new_art, new_bt);
                }
                MapMethod::Delete { key } => {
                    let btr = bt_map.remove(key);
                    let art_key: ArrayKey<16> = ArrayKey::from(*key);
                    let artr = art.remove(&art_key);
                    eprintln!("Delete: {:?} {:?} {:}", key, artr, btr.is_some());
                    assert_eq!(btr.is_some(), artr);
                }
            }
        }
    }

    for (k, expected_value) in bt_map.iter() {
        let art_key: ArrayKey<16> = ArrayKey::from(*k);
        let result = art.get(&art_key);
        eprintln!("Cmp key {:?} {:?} {:?}", k, result, expected_value);
        assert_eq!(
            result,
            Some(expected_value),
            "Expected value for key {}: {:?} != {:?}",
            k,
            art.get(&art_key).copied(),
            *expected_value
        );
    }
});
