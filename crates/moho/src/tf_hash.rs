//! A port of the parts of OpenUSD's `TfHash` that UsdImaging names prims
//! with: `TfHash::Combine` over strings and over a sorted map of strings.
//!
//! UsdImaging's point instancer propagation names a prototype's copy
//! `ForInstancer%zx` of `TfHash::Combine(instancer, propagatedPrototype)`
//! (`piPrototypePropagatingSceneIndex.cpp`), and native instance
//! aggregation names a group with constant primvars `Primvars%zx` of
//! `TfHash::Combine(std::map<std::string, std::string>)`
//! (`niInstanceAggregationSceneIndex.cpp`). Hashing the same bytes the same
//! way gives Hydra's prim names.
//!
//! - A string is appended as its bytes: `Tf_HashState::_AppendBytes`, which
//!   hashes them with `ArchHash64` (Bob Jenkins' SpookyHash V2, 64 bits)
//!   seeded with the state so far.
//! - A map is appended as its entries in key order, each key then value.
//! - The code is the state times Knuth's multiplicative constant, its bytes
//!   swapped (`Tf_HashState::_GetCode`).
//!
//! The values are checked against a USD build (`scripts/tf-hash-golden.cpp`).

use std::collections::BTreeMap;

/// The accumulated state of `Tf_HashState`, for strings only.
#[derive(Default)]
struct HashState {
    state: u64,
}

impl HashState {
    /// `Tf_HashState::_AppendBytes`: the bytes hashed, seeded with the state.
    fn append_bytes(&mut self, bytes: &[u8]) {
        self.state = spooky::hash64(bytes, self.state);
    }

    /// `Tf_HashState::_GetCode`.
    fn code(&self) -> u64 {
        self.state.wrapping_mul(11_400_714_819_323_198_549).swap_bytes()
    }
}

/// `TfHash::Combine` of strings, in order.
pub(crate) fn combine_strings(strings: &[&str]) -> u64 {
    let mut h = HashState::default();
    for s in strings {
        h.append_bytes(s.as_bytes());
    }
    h.code()
}

/// `TfHash::Combine` of a `std::map<std::string, std::string>`: each entry in
/// key order, its key then its value.
pub(crate) fn combine_map(map: &BTreeMap<String, String>) -> u64 {
    let mut h = HashState::default();
    for (key, value) in map {
        h.append_bytes(key.as_bytes());
        h.append_bytes(value.as_bytes());
    }
    h.code()
}

/// SpookyHash V2 (Bob Jenkins, public domain), as OpenUSD's `arch/hash.cpp`
/// carries it: little-endian reads, `Hash64` returning the first half of the
/// 128-bit hash seeded with `(seed, seed)`.
mod spooky {
    const SC_NUM_VARS: usize = 12;
    const SC_BLOCK_SIZE: usize = SC_NUM_VARS * 8;
    const SC_BUF_SIZE: usize = 2 * SC_BLOCK_SIZE;
    const SC_CONST: u64 = 0xdead_beef_dead_beef;

    fn rot(x: u64, k: u32) -> u64 {
        x.rotate_left(k)
    }

    /// The little-endian `u64` at byte `at` of `bytes`, zero-padded past the
    /// end.
    fn u64_at(bytes: &[u8], at: usize) -> u64 {
        let mut word = [0u8; 8];
        for (i, b) in word.iter_mut().enumerate() {
            *b = bytes.get(at + i).copied().unwrap_or(0);
        }
        u64::from_le_bytes(word)
    }

    /// The little-endian `u32` at byte `at` of `bytes`.
    fn u32_at(bytes: &[u8], at: usize) -> u64 {
        let mut word = [0u8; 4];
        for (i, b) in word.iter_mut().enumerate() {
            *b = bytes.get(at + i).copied().unwrap_or(0);
        }
        u64::from(u32::from_le_bytes(word))
    }

    fn byte(bytes: &[u8], at: usize) -> u64 {
        u64::from(bytes.get(at).copied().unwrap_or(0))
    }

    fn short_mix(h: &mut [u64; 4]) {
        let [h0, h1, h2, h3] = h;
        *h2 = rot(*h2, 50);
        *h2 = h2.wrapping_add(*h3);
        *h0 ^= *h2;
        *h3 = rot(*h3, 52);
        *h3 = h3.wrapping_add(*h0);
        *h1 ^= *h3;
        *h0 = rot(*h0, 30);
        *h0 = h0.wrapping_add(*h1);
        *h2 ^= *h0;
        *h1 = rot(*h1, 41);
        *h1 = h1.wrapping_add(*h2);
        *h3 ^= *h1;
        *h2 = rot(*h2, 54);
        *h2 = h2.wrapping_add(*h3);
        *h0 ^= *h2;
        *h3 = rot(*h3, 48);
        *h3 = h3.wrapping_add(*h0);
        *h1 ^= *h3;
        *h0 = rot(*h0, 38);
        *h0 = h0.wrapping_add(*h1);
        *h2 ^= *h0;
        *h1 = rot(*h1, 37);
        *h1 = h1.wrapping_add(*h2);
        *h3 ^= *h1;
        *h2 = rot(*h2, 62);
        *h2 = h2.wrapping_add(*h3);
        *h0 ^= *h2;
        *h3 = rot(*h3, 34);
        *h3 = h3.wrapping_add(*h0);
        *h1 ^= *h3;
        *h0 = rot(*h0, 5);
        *h0 = h0.wrapping_add(*h1);
        *h2 ^= *h0;
        *h1 = rot(*h1, 36);
        *h1 = h1.wrapping_add(*h2);
        *h3 ^= *h1;
    }

    fn short_end(h: &mut [u64; 4]) {
        let [h0, h1, h2, h3] = h;
        *h3 ^= *h2;
        *h2 = rot(*h2, 15);
        *h3 = h3.wrapping_add(*h2);
        *h0 ^= *h3;
        *h3 = rot(*h3, 52);
        *h0 = h0.wrapping_add(*h3);
        *h1 ^= *h0;
        *h0 = rot(*h0, 26);
        *h1 = h1.wrapping_add(*h0);
        *h2 ^= *h1;
        *h1 = rot(*h1, 51);
        *h2 = h2.wrapping_add(*h1);
        *h3 ^= *h2;
        *h2 = rot(*h2, 28);
        *h3 = h3.wrapping_add(*h2);
        *h0 ^= *h3;
        *h3 = rot(*h3, 9);
        *h0 = h0.wrapping_add(*h3);
        *h1 ^= *h0;
        *h0 = rot(*h0, 47);
        *h1 = h1.wrapping_add(*h0);
        *h2 ^= *h1;
        *h1 = rot(*h1, 54);
        *h2 = h2.wrapping_add(*h1);
        *h3 ^= *h2;
        *h2 = rot(*h2, 32);
        *h3 = h3.wrapping_add(*h2);
        *h0 ^= *h3;
        *h3 = rot(*h3, 25);
        *h0 = h0.wrapping_add(*h3);
        *h1 ^= *h0;
        *h0 = rot(*h0, 63);
        *h1 = h1.wrapping_add(*h0);
    }

    /// `SpookyHash::Short`, for messages under 192 bytes.
    fn short(message: &[u8], seed1: u64, seed2: u64) -> u64 {
        let length = message.len();
        let mut remainder = length % 32;
        let mut h = [seed1, seed2, SC_CONST, SC_CONST];
        let mut at = 0;
        if length > 15 {
            let end = (length / 32) * 32;
            while at < end {
                h[2] = h[2].wrapping_add(u64_at(message, at));
                h[3] = h[3].wrapping_add(u64_at(message, at + 8));
                short_mix(&mut h);
                h[0] = h[0].wrapping_add(u64_at(message, at + 16));
                h[1] = h[1].wrapping_add(u64_at(message, at + 24));
                at += 32;
            }
            if remainder >= 16 {
                h[2] = h[2].wrapping_add(u64_at(message, at));
                h[3] = h[3].wrapping_add(u64_at(message, at + 8));
                short_mix(&mut h);
                at += 16;
                remainder -= 16;
            }
        }
        // The last 0..15 bytes, and the length. The cases fall through in
        // C++; each arm here adds what its fall-through would.
        h[3] = h[3].wrapping_add((length as u64) << 56);
        let b = |i: usize| byte(message, at + i);
        let mut c = 0u64;
        let mut d = 0u64;
        match remainder {
            12..=15 => {
                if remainder >= 15 {
                    d = d.wrapping_add(b(14) << 48);
                }
                if remainder >= 14 {
                    d = d.wrapping_add(b(13) << 40);
                }
                if remainder >= 13 {
                    d = d.wrapping_add(b(12) << 32);
                }
                d = d.wrapping_add(u32_at(message, at + 8));
                c = c.wrapping_add(u64_at(message, at));
            }
            8..=11 => {
                if remainder >= 11 {
                    d = d.wrapping_add(b(10) << 16);
                }
                if remainder >= 10 {
                    d = d.wrapping_add(b(9) << 8);
                }
                if remainder >= 9 {
                    d = d.wrapping_add(b(8));
                }
                c = c.wrapping_add(u64_at(message, at));
            }
            4..=7 => {
                if remainder >= 7 {
                    c = c.wrapping_add(b(6) << 48);
                }
                if remainder >= 6 {
                    c = c.wrapping_add(b(5) << 40);
                }
                if remainder >= 5 {
                    c = c.wrapping_add(b(4) << 32);
                }
                c = c.wrapping_add(u32_at(message, at));
            }
            1..=3 => {
                if remainder >= 3 {
                    c = c.wrapping_add(b(2) << 16);
                }
                if remainder >= 2 {
                    c = c.wrapping_add(b(1) << 8);
                }
                c = c.wrapping_add(b(0));
            }
            _ => {
                c = c.wrapping_add(SC_CONST);
                d = d.wrapping_add(SC_CONST);
            }
        }
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        short_end(&mut h);
        h[0]
    }

    fn mix(data: &[u64; SC_NUM_VARS], s: &mut [u64; SC_NUM_VARS]) {
        // (index added, index xored with s[x10], index xored into, rotation)
        // spelled out as `SpookyHash::Mix`.
        const ROT: [u32; SC_NUM_VARS] = [11, 32, 43, 31, 17, 28, 39, 57, 55, 54, 22, 46];
        for i in 0..SC_NUM_VARS {
            s[i] = s[i].wrapping_add(data[i]);
            s[(i + 2) % SC_NUM_VARS] ^= s[(i + 10) % SC_NUM_VARS];
            s[(i + 11) % SC_NUM_VARS] ^= s[i];
            s[i] = rot(s[i], ROT[i]);
            s[(i + 11) % SC_NUM_VARS] = s[(i + 11) % SC_NUM_VARS].wrapping_add(s[(i + 1) % SC_NUM_VARS]);
        }
    }

    fn end_partial(h: &mut [u64; SC_NUM_VARS]) {
        // `SpookyHash::EndPartial`: line i adds h[i+1] into h[i-1], xors
        // h[i-1] into h[i+2] and rotates h[i+1].
        const ROT: [u32; SC_NUM_VARS] = [44, 15, 34, 21, 38, 33, 10, 13, 38, 53, 42, 54];
        for (i, r) in ROT.iter().enumerate() {
            let a = (i + 11) % SC_NUM_VARS;
            let b = (i + 1) % SC_NUM_VARS;
            let c = (i + 2) % SC_NUM_VARS;
            h[a] = h[a].wrapping_add(h[b]);
            h[c] ^= h[a];
            h[b] = rot(h[b], *r);
        }
    }

    fn end(data: &[u64; SC_NUM_VARS], h: &mut [u64; SC_NUM_VARS]) {
        for i in 0..SC_NUM_VARS {
            h[i] = h[i].wrapping_add(data[i]);
        }
        end_partial(h);
        end_partial(h);
        end_partial(h);
    }

    fn block(message: &[u8], at: usize) -> [u64; SC_NUM_VARS] {
        let mut out = [0u64; SC_NUM_VARS];
        for (i, word) in out.iter_mut().enumerate() {
            *word = u64_at(message, at + i * 8);
        }
        out
    }

    /// `SpookyHash::Hash64(message, length, seed)`.
    pub(super) fn hash64(message: &[u8], seed: u64) -> u64 {
        let length = message.len();
        if length < SC_BUF_SIZE {
            return short(message, seed, seed);
        }
        // h0 = h3 = h6 = h9 and h1 = h4 = h7 = h10 are the seeds, the others
        // the constant.
        let mut h = [0u64; SC_NUM_VARS];
        for (i, word) in h.iter_mut().enumerate() {
            *word = if i % 3 == 2 { SC_CONST } else { seed };
        }
        let whole = (length / SC_BLOCK_SIZE) * SC_BLOCK_SIZE;
        let mut at = 0;
        while at < whole {
            mix(&block(message, at), &mut h);
            at += SC_BLOCK_SIZE;
        }
        let remainder = length - whole;
        let mut last = [0u8; SC_BLOCK_SIZE];
        last[..remainder].copy_from_slice(&message[whole..]);
        last[SC_BLOCK_SIZE - 1] = remainder as u8;
        end(&block(&last, 0), &mut h);
        h[0]
    }
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
