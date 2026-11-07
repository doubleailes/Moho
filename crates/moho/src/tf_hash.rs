//! `TfHash::Combine` over strings and over a sorted map of strings, the
//! parts of OpenUSD's `TfHash` that UsdImaging names prims with, on
//! openusd's `tf::HashState` (C++ `Tf_HashState`).
//!
//! UsdImaging's point instancer propagation names a prototype's copy
//! `ForInstancer%zx` of `TfHash::Combine(instancer, propagatedPrototype)`
//! (`piPrototypePropagatingSceneIndex.cpp`), and native instance
//! aggregation names a group with constant primvars `Primvars%zx` of
//! `TfHash::Combine(std::map<std::string, std::string>)`
//! (`niInstanceAggregationSceneIndex.cpp`). Hashing the same bytes the same
//! way gives Hydra's prim names: a string is appended as its bytes, a map
//! as its entries in key order, each key then value.
//!
//! The values are checked against a USD build (`scripts/tf-hash-golden.cpp`).

use std::collections::BTreeMap;

use openusd::tf::HashState;

/// `TfHash::Combine` of strings, in order.
pub(crate) fn combine_strings(strings: &[&str]) -> u64 {
    let mut h = HashState::default();
    for s in strings {
        h.append(s);
    }
    h.code()
}

/// `TfHash::Combine` of a `std::map<std::string, std::string>`: each entry in
/// key order, its key then its value.
pub(crate) fn combine_map(map: &BTreeMap<String, String>) -> u64 {
    let mut h = HashState::default();
    for (key, value) in map {
        h.append(key);
        h.append(value);
    }
    h.code()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Computed with OpenUSD 26.08's `TfHash::Combine` (the reference
    // revision's `tf/hash.h` and `arch/hash.cpp`), by
    // `scripts/tf-hash-golden.cpp`.
    #[test]
    fn combine_of_two_strings_is_tf_hash() {
        let golden: &[(&str, &str, u64)] = &[
            ("/World/Instancer", "/", 0xa61eb064d406b29f),
            ("/Tree/Leaves", "/Trees/tree/ForInstancer1", 0x14855720f51764cd),
            ("", "", 0x941f3c26d090362d),
            ("/a", "/", 0x729f030fde4ead23),
            (
                "/World/sets/Rocks/PointInstancer_with_a_long_name_for_testing_the_32_byte_blocks",
                "/",
                0x716dd884530dcda4,
            ),
            (
                "/Some/Very/Long/Path/That/Goes/On/And/On/For/Quite/A/While/To/Exceed/The/Spooky/Short/Hash/Threshold/Of/One/Hundred/And/Ninety/Two/Bytes/In/Length/For/Sure/Yes/Indeed/More/Than/Enough",
                "/Another/Prototype/Root/ForInstancer123abc",
                0xa5f33d0fd49b7d0f,
            ),
            ("/world/trees", "/", 0x27d4476d2fc5f07a),
            (
                "/world/trees/Prototypes/tree/leaves",
                "/world/trees/Prototypes/tree/ForInstancer27d4476d2fc5f07a",
                0x91211c283fb47b5f,
            ),
        ];
        for (a, b, expected) in golden {
            assert_eq!(combine_strings(&[a, b]), *expected, "Combine({a:?}, {b:?})");
        }
    }

    #[test]
    fn long_messages_take_spooky_hash_long_path() {
        let long192 = format!("/{}", "a".repeat(191));
        let long300: String = (0..30).map(|i| format!("/segment_{i}")).collect();
        assert!(long300.len() >= 192);
        assert_eq!(combine_strings(&[&long192, "/"]), 0x284b94e778823972);
        assert_eq!(combine_strings(&[&long300, &long192]), 0x35ea98eaf50ec046);
    }

    #[test]
    fn combine_of_a_map_is_tf_hash() {
        let map = |entries: &[(&str, &str)]| -> BTreeMap<String, String> {
            entries
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect()
        };
        assert_eq!(
            combine_map(&map(&[("displayColor", "Color")])),
            0x2a699f1bd0ef6389
        );
        assert_eq!(
            combine_map(&map(&[
                ("displayColor", "Color"),
                ("displayOpacity", ""),
                ("st", "TextureCoordinate")
            ])),
            0xde7ff2d00ca8c5c6
        );
        assert_eq!(combine_map(&map(&[("foo", "")])), 0xddc3440b5e101171);
    }
}
