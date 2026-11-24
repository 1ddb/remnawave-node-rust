//! Byte-for-byte compatible port of `@remnawave/hashed-set`.
//!
//! The original implementation uses two 32-bit DJB2 accumulators XOR-ed over
//! the set members. The resulting `hash64String` is sent to (and compared by)
//! the panel, so the algorithm must not change.

use std::collections::HashSet;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HashedSet {
    inner: HashSet<String>,
    hash_high: u32,
    hash_low: u32,
}

impl HashedSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn contains(&self, value: &str) -> bool {
        self.inner.contains(value)
    }

    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.inner.iter()
    }

    /// Mirrors `Set.prototype.add`: if the value was already present the hash is
    /// left untouched.
    pub fn add(&mut self, value: impl Into<String>) {
        let value = value.into();
        if !self.inner.contains(&value) {
            let (high, low) = djb2_dual(&value);
            self.hash_high ^= high;
            self.hash_low ^= low;
        }
        self.inner.insert(value);
    }

    pub fn delete(&mut self, value: &str) -> bool {
        if self.inner.remove(value) {
            let (high, low) = djb2_dual(value);
            self.hash_high ^= high;
            self.hash_low ^= low;
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.inner.clear();
        self.hash_high = 0;
        self.hash_low = 0;
    }

    pub fn hash_high(&self) -> u32 {
        self.hash_high
    }

    pub fn hash_low(&self) -> u32 {
        self.hash_low
    }

    pub fn hash64_string(&self) -> String {
        format!("{}{}", js_i32_hex(self.hash_high as i32), js_i32_hex(self.hash_low as i32))
    }

    pub fn has_same_hash(&self, other: &Self) -> bool {
        self.hash_high == other.hash_high && self.hash_low == other.hash_low
    }

    pub fn fast_equals(&self, other: &Self) -> bool {
        self.len() == other.len() && self.has_same_hash(other)
    }
}

impl Extend<String> for HashedSet {
    fn extend<I: IntoIterator<Item = String>>(&mut self, iter: I) {
        for item in iter {
            self.add(item);
        }
    }
}

impl FromIterator<String> for HashedSet {
    fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        let mut set = Self::new();
        set.extend(iter);
        set
    }
}

/// Reproduces `Number.prototype.toString(16).padStart(8, "0")` for a value
/// obtained from JavaScript bitwise XOR (i.e. a signed 32-bit integer).
///
/// Negative values keep the minus sign and the absolute value in hex, exactly
/// like JavaScript; leading zeroes are inserted before the whole string.
fn js_i32_hex(value: i32) -> String {
    let raw = if value < 0 {
        format!("-{:x}", value.unsigned_abs())
    } else {
        format!("{:x}", value as u32)
    };
    if raw.len() >= 8 {
        raw
    } else {
        format!("{}{raw}", "0".repeat(8 - raw.len()))
    }
}

fn djb2_dual(s: &str) -> (u32, u32) {
    let mut high: i32 = 5381;
    let mut low: i32 = 5387;

    // JS iterates UTF-16 code units; `encode_utf16` reproduces that exactly.
    for unit in s.encode_utf16() {
        let ch = unit as i32;
        high = (high << 5).wrapping_add(high).wrapping_add(ch);
        low = (low << 6).wrapping_add(low).wrapping_add(ch.wrapping_mul(37));
    }

    (high as u32, low as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference values produced by @remnawave/hashed-set@0.0.4 running on
    // Node.js 22 (`new HashedSet([...]).hash64String`).
    #[test]
    fn empty_hash() {
        let set = HashedSet::new();
        assert_eq!(set.hash64_string(), "0000000000000000");
    }

    #[test]
    fn single_value() {
        let mut set = HashedSet::new();
        set.add("user-1");
        assert_eq!(set.len(), 1);
        assert_eq!(set.hash64_string(), "2124e58235d265fc");
    }

    #[test]
    fn duplicate_add_is_noop() {
        let mut set = HashedSet::new();
        set.add("abc");
        assert_eq!(set.hash64_string(), "0b885c8b5918f4c9");
        set.add("abc");
        assert_eq!(set.hash64_string(), "0b885c8b5918f4c9");
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn delete_restores_hash() {
        let mut set = HashedSet::new();
        set.add("a");
        set.add("b");
        set.delete("a");
        let mut expected = HashedSet::new();
        expected.add("b");
        assert_eq!(set.hash64_string(), expected.hash64_string());
        assert_eq!(set.hash64_string(), "0002b607000565f5");
    }

    #[test]
    fn matches_reference_for_sets() {
        let cases: &[(&[&str], &str)] = &[
            (&["a", "b"], "0000000100000025"),
            (&["a", "b", "c"], "0002b6090005663f"),
            (&["user-1", "user-2", "user-3"], "2124e58535d2659b"),
            (&["dup", "dup"], "0b886bce5920d118"),
        ];
        for (members, expected) in cases {
            let set = HashedSet::from_iter(members.iter().map(|s| (*s).to_string()));
            assert_eq!(&set.hash64_string(), expected, "members {members:?}");
        }
    }

    #[test]
    fn matches_reference_for_utf16_and_negative_hashes() {
        let cases: &[(&str, &str)] = &[
            ("Привет", "-be3c67b0e680ecb"),
            ("🎉", "0076298a096b0504"),
            ("", "000015050000150b"),
            (
                "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
                "-5ac2bb9b-60130d95",
            ),
        ];
        for (value, expected) in cases {
            let set = HashedSet::from_iter([(*value).to_string()]);
            assert_eq!(&set.hash64_string(), expected, "value {value:?}");
        }

        let set = HashedSet::from_iter(["🎉".to_string(), "Привет".to_string()]);
        assert_eq!(set.hash64_string(), "-b95eff107030bcf");
    }

    #[test]
    fn js_hex_matches_signed_string_representation() {
        assert_eq!(js_i32_hex(0), "00000000");
        assert_eq!(js_i32_hex(0x0002b606), "0002b606");
        assert_eq!(js_i32_hex(-0x0be3c67b), "-be3c67b");
        assert_eq!(js_i32_hex(i32::MIN), "-80000000");
        assert_eq!(js_i32_hex(-0x5a), "00000-5a");
    }
}
