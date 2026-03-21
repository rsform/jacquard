//! Serde spike: empirical validation of serde behaviour with type-parameterised structs.
//!
//! This module answers three questions from the borrow-or-share design plan:
//!
//! 1. Does `#[serde(borrow)]` on an `S`-typed field prevent `DeserializeOwned` when `S = SmolStr`?
//!    **Answer: YES.** `#[serde(borrow)]` is sugar for `#[serde(bound(deserialize = "'de: 'a"))]`
//!    and requires the field type to contain a lifetime. Type params like `S` have no lifetime,
//!    so the macro rejects it outright. Even if it didn't, the injected bound would prevent
//!    `DeserializeOwned`. Strategy A is dead.
//!
//! 2. Does `Deserialize<'de>` work for `S = &'de str` without `#[serde(borrow)]`?
//!    **Tested below** in strategies B and C.
//!
//! 3. What serde attribute combinations should codegen emit?
//!    **Tested below** — strategies B (no attrs) and C (explicit bounds) are the candidates.

use alloc::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

// ---------------------------------------------------------------------------
// Minimal Bos/BorrowOrShare trait copies (will live in jacquard-common later)
// ---------------------------------------------------------------------------

mod bos {
    mod internal {
        pub trait Ref<T: ?Sized> {
            fn cast<'a>(self) -> &'a T
            where
                Self: 'a;
        }

        impl<T: ?Sized> Ref<T> for &T {
            #[inline]
            fn cast<'a>(self) -> &'a T
            where
                Self: 'a,
            {
                self
            }
        }
    }

    use alloc::borrow::ToOwned;

    use internal::Ref;

    /// Borrow or share — the base trait with a GAT for the reference type.
    pub trait Bos<T: ?Sized> {
        type Ref<'this>: Ref<T>
        where
            Self: 'this;

        fn borrow_or_share(this: &Self) -> Self::Ref<'_>;
    }

    /// Convenience trait with split lifetimes for borrowed vs shared access.
    pub trait BorrowOrShare<'i, 'o, T: ?Sized>: Bos<T> {
        fn borrow_or_share(&'i self) -> &'o T;
    }

    impl<'i, 'o, T: ?Sized, B> BorrowOrShare<'i, 'o, T> for B
    where
        B: Bos<T> + ?Sized + 'i,
        B::Ref<'i>: 'o,
    {
        #[inline]
        fn borrow_or_share(&'i self) -> &'o T {
            (B::borrow_or_share(self) as B::Ref<'i>).cast()
        }
    }

    // --- Implementations ---

    impl<'a, T: ?Sized> Bos<T> for &'a T {
        type Ref<'this>
            = &'a T
        where
            Self: 'this;

        #[inline]
        fn borrow_or_share(this: &Self) -> Self::Ref<'_> {
            this
        }
    }

    impl Bos<str> for smol_str::SmolStr {
        type Ref<'this> = &'this str;

        #[inline]
        fn borrow_or_share(this: &Self) -> Self::Ref<'_> {
            this.as_str()
        }
    }

    impl Bos<str> for String {
        type Ref<'this> = &'this str;

        #[inline]
        fn borrow_or_share(this: &Self) -> Self::Ref<'_> {
            this.as_str()
        }
    }

    impl<'a, B: ?Sized + ToOwned> Bos<B> for alloc::borrow::Cow<'a, B> {
        type Ref<'this>
            = &'this B
        where
            Self: 'this;

        #[inline]
        fn borrow_or_share(this: &Self) -> Self::Ref<'_> {
            this.as_ref()
        }
    }

    impl<'a> Bos<str> for jacquard_common::cowstr::CowStr<'a> {
        type Ref<'this>
            = &'this str
        where
            Self: 'this;

        #[inline]
        fn borrow_or_share(this: &Self) -> Self::Ref<'_> {
            this.as_str()
        }
    }
}

use bos::Bos;

// ---------------------------------------------------------------------------
// Strategy B: no serde attributes at all — let serde derive infer everything
// ---------------------------------------------------------------------------

/// Flat struct with no serde annotations on fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlatNoBorrow<S: Bos<str> = SmolStr> {
    pub name: S,
    pub label: Option<S>,
    pub tags: Vec<S>,
}

/// Nested struct containing `FlatNoBorrow<S>`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NestedNoBorrow<S: Bos<str> = SmolStr> {
    pub inner: FlatNoBorrow<S>,
    pub count: u32,
}

/// Struct with `BTreeMap<SmolStr, S>` — mixed ownership (keys always SmolStr).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WithMapNoBorrow<S: Bos<str> = SmolStr> {
    pub title: S,
    pub metadata: BTreeMap<SmolStr, S>,
}

// ---------------------------------------------------------------------------
// Strategy C: explicit #[serde(bound(...))] — override serde's inferred bounds
// ---------------------------------------------------------------------------

/// Flat struct with explicit serde bounds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(bound(serialize = "S: Serialize", deserialize = "S: Deserialize<'de>"))]
pub struct FlatExplicitBound<S: Bos<str> = SmolStr> {
    pub name: S,
    pub label: Option<S>,
    pub tags: Vec<S>,
}

/// Nested struct with explicit serde bounds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(bound(serialize = "S: Serialize", deserialize = "S: Deserialize<'de>"))]
pub struct NestedExplicitBound<S: Bos<str> = SmolStr> {
    pub inner: FlatExplicitBound<S>,
    pub count: u32,
}

/// Map struct with explicit serde bounds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(bound(serialize = "S: Serialize", deserialize = "S: Deserialize<'de>"))]
pub struct WithMapExplicitBound<S: Bos<str> = SmolStr> {
    pub title: S,
    pub metadata: BTreeMap<SmolStr, S>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use jacquard_common::cowstr::CowStr;
    use serde::de::DeserializeOwned;

    const TEST_JSON: &str = r#"{
        "name": "alice",
        "label": "admin",
        "tags": ["rust", "atproto"]
    }"#;

    const TEST_NESTED_JSON: &str = r#"{
        "inner": {
            "name": "alice",
            "label": "admin",
            "tags": ["rust", "atproto"]
        },
        "count": 42
    }"#;

    const TEST_MAP_JSON: &str = r#"{
        "title": "hello",
        "metadata": {
            "key1": "val1",
            "key2": "val2"
        }
    }"#;

    // -----------------------------------------------------------------------
    // Compile-time assertions
    // -----------------------------------------------------------------------

    fn assert_deserialize_owned<T: DeserializeOwned>() {}
    fn assert_deserialize<'de, T: Deserialize<'de>>() {}

    // ===== Strategy B: no attributes =====

    #[test]
    fn strategy_b_smolstr_deserialize_owned() {
        assert_deserialize_owned::<FlatNoBorrow<SmolStr>>();
        assert_deserialize_owned::<NestedNoBorrow<SmolStr>>();
        assert_deserialize_owned::<WithMapNoBorrow<SmolStr>>();
    }

    #[test]
    fn strategy_b_string_deserialize_owned() {
        assert_deserialize_owned::<FlatNoBorrow<String>>();
        assert_deserialize_owned::<NestedNoBorrow<String>>();
        assert_deserialize_owned::<WithMapNoBorrow<String>>();
    }

    #[test]
    fn strategy_b_borrowed_deserialize() {
        // Does &str satisfy Deserialize<'de> via strategy B (no attrs)?
        assert_deserialize::<FlatNoBorrow<&str>>();
        assert_deserialize::<NestedNoBorrow<&str>>();
        assert_deserialize::<WithMapNoBorrow<&str>>();
    }

    // CowStr compile-time shape tests.
    //
    // We can't use assert_deserialize/assert_deserialize_owned for CowStr because:
    // - CowStr<'static> does NOT satisfy DeserializeOwned (the Deserialize impl
    //   has 'de: 'a, and Rust can't specialise that away when 'a = 'static)
    // - CowStr<'_> with an elided lifetime can't relate to the 'de on the helper
    //
    // Instead we prove the shape compiles by writing functions with the right
    // lifetime relationship. The runtime tests below exercise actual behaviour.

    #[allow(dead_code)]
    fn cowstr_deserialize_shape_b(input: &str) -> FlatNoBorrow<CowStr<'_>> {
        serde_json::from_str(input).unwrap()
    }

    #[allow(dead_code)]
    fn cowstr_nested_deserialize_shape_b(input: &str) -> NestedNoBorrow<CowStr<'_>> {
        serde_json::from_str(input).unwrap()
    }

    // ===== Strategy C: explicit bounds =====

    #[test]
    fn strategy_c_smolstr_deserialize_owned() {
        assert_deserialize_owned::<FlatExplicitBound<SmolStr>>();
        assert_deserialize_owned::<NestedExplicitBound<SmolStr>>();
        assert_deserialize_owned::<WithMapExplicitBound<SmolStr>>();
    }

    #[test]
    fn strategy_c_string_deserialize_owned() {
        assert_deserialize_owned::<FlatExplicitBound<String>>();
        assert_deserialize_owned::<NestedExplicitBound<String>>();
        assert_deserialize_owned::<WithMapExplicitBound<String>>();
    }

    #[test]
    fn strategy_c_borrowed_deserialize() {
        assert_deserialize::<FlatExplicitBound<&str>>();
        assert_deserialize::<NestedExplicitBound<&str>>();
        assert_deserialize::<WithMapExplicitBound<&str>>();
    }

    // CowStr shape tests for strategy C (same limitation as B).

    #[allow(dead_code)]
    fn cowstr_deserialize_shape_c(input: &str) -> FlatExplicitBound<CowStr<'_>> {
        serde_json::from_str(input).unwrap()
    }

    #[allow(dead_code)]
    fn cowstr_nested_deserialize_shape_c(input: &str) -> NestedExplicitBound<CowStr<'_>> {
        serde_json::from_str(input).unwrap()
    }

    // -----------------------------------------------------------------------
    // Runtime: JSON roundtrips — Strategy B
    // -----------------------------------------------------------------------

    #[test]
    fn strategy_b_json_roundtrip_flat_smolstr() {
        let parsed: FlatNoBorrow<SmolStr> = serde_json::from_str(TEST_JSON).unwrap();
        assert_eq!(parsed.name, SmolStr::new("alice"));
        assert_eq!(parsed.label, Some(SmolStr::new("admin")));
        assert_eq!(
            parsed.tags,
            vec![SmolStr::new("rust"), SmolStr::new("atproto")]
        );

        let json = serde_json::to_string(&parsed).unwrap();
        let reparsed: FlatNoBorrow<SmolStr> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn strategy_b_json_roundtrip_nested_smolstr() {
        let parsed: NestedNoBorrow<SmolStr> = serde_json::from_str(TEST_NESTED_JSON).unwrap();
        assert_eq!(parsed.inner.name, SmolStr::new("alice"));
        assert_eq!(parsed.count, 42);

        let json = serde_json::to_string(&parsed).unwrap();
        let reparsed: NestedNoBorrow<SmolStr> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn strategy_b_json_roundtrip_map_smolstr() {
        let parsed: WithMapNoBorrow<SmolStr> = serde_json::from_str(TEST_MAP_JSON).unwrap();
        assert_eq!(parsed.title, SmolStr::new("hello"));
        assert_eq!(
            parsed.metadata.get(&SmolStr::new("key1")),
            Some(&SmolStr::new("val1"))
        );

        let json = serde_json::to_string(&parsed).unwrap();
        let reparsed: WithMapNoBorrow<SmolStr> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn strategy_b_json_roundtrip_flat_string() {
        let parsed: FlatNoBorrow<String> = serde_json::from_str(TEST_JSON).unwrap();
        assert_eq!(parsed.name, "alice");

        let json = serde_json::to_string(&parsed).unwrap();
        let reparsed: FlatNoBorrow<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn strategy_b_json_borrowed_flat() {
        let parsed: FlatNoBorrow<&str> = serde_json::from_str(TEST_JSON).unwrap();
        assert_eq!(parsed.name, "alice");
        assert_eq!(parsed.label, Some("admin"));
        assert_eq!(parsed.tags, vec!["rust", "atproto"]);
    }

    #[test]
    fn strategy_b_json_borrowed_nested() {
        let parsed: NestedNoBorrow<&str> = serde_json::from_str(TEST_NESTED_JSON).unwrap();
        assert_eq!(parsed.inner.name, "alice");
        assert_eq!(parsed.count, 42);
    }

    #[test]
    fn strategy_b_json_borrowed_map() {
        let parsed: WithMapNoBorrow<&str> = serde_json::from_str(TEST_MAP_JSON).unwrap();
        assert_eq!(parsed.title, "hello");
        assert_eq!(parsed.metadata.get(&SmolStr::new("key1")), Some(&"val1"));
    }

    #[test]
    fn strategy_b_json_cowstr() {
        let parsed: FlatNoBorrow<CowStr> = serde_json::from_str(TEST_JSON).unwrap();
        assert_eq!(parsed.name.as_str(), "alice");
        assert_eq!(parsed.label.as_ref().map(|c| c.as_str()), Some("admin"));

        let json = serde_json::to_string(&parsed).unwrap();
        let reparsed: FlatNoBorrow<CowStr> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, reparsed);
    }

    // -----------------------------------------------------------------------
    // Runtime: JSON roundtrips — Strategy C
    // -----------------------------------------------------------------------

    #[test]
    fn strategy_c_json_roundtrip_flat_smolstr() {
        let parsed: FlatExplicitBound<SmolStr> = serde_json::from_str(TEST_JSON).unwrap();
        assert_eq!(parsed.name, SmolStr::new("alice"));

        let json = serde_json::to_string(&parsed).unwrap();
        let reparsed: FlatExplicitBound<SmolStr> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn strategy_c_json_roundtrip_nested_smolstr() {
        let parsed: NestedExplicitBound<SmolStr> = serde_json::from_str(TEST_NESTED_JSON).unwrap();
        assert_eq!(parsed.inner.name, SmolStr::new("alice"));
        assert_eq!(parsed.count, 42);

        let json = serde_json::to_string(&parsed).unwrap();
        let reparsed: NestedExplicitBound<SmolStr> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn strategy_c_json_borrowed_flat() {
        let parsed: FlatExplicitBound<&str> = serde_json::from_str(TEST_JSON).unwrap();
        assert_eq!(parsed.name, "alice");
        assert_eq!(parsed.label, Some("admin"));
        assert_eq!(parsed.tags, vec!["rust", "atproto"]);
    }

    #[test]
    fn strategy_c_json_borrowed_nested() {
        let parsed: NestedExplicitBound<&str> = serde_json::from_str(TEST_NESTED_JSON).unwrap();
        assert_eq!(parsed.inner.name, "alice");
        assert_eq!(parsed.count, 42);
    }

    #[test]
    fn strategy_c_json_cowstr() {
        let parsed: FlatExplicitBound<CowStr> = serde_json::from_str(TEST_JSON).unwrap();
        assert_eq!(parsed.name.as_str(), "alice");

        let json = serde_json::to_string(&parsed).unwrap();
        let reparsed: FlatExplicitBound<CowStr> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, reparsed);
    }

    // -----------------------------------------------------------------------
    // DAG-CBOR roundtrips — Strategy B (if JSON works, CBOR should too)
    // -----------------------------------------------------------------------

    #[test]
    fn strategy_b_dagcbor_roundtrip_flat_smolstr() {
        let original = FlatNoBorrow {
            name: SmolStr::new("alice"),
            label: Some(SmolStr::new("admin")),
            tags: vec![SmolStr::new("rust"), SmolStr::new("atproto")],
        };

        let bytes = serde_ipld_dagcbor::to_vec(&original).unwrap();
        let parsed: FlatNoBorrow<SmolStr> = serde_ipld_dagcbor::from_slice(&bytes).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn strategy_b_dagcbor_roundtrip_flat_string() {
        let original = FlatNoBorrow {
            name: String::from("alice"),
            label: Some(String::from("admin")),
            tags: vec![String::from("rust"), String::from("atproto")],
        };

        let bytes = serde_ipld_dagcbor::to_vec(&original).unwrap();
        let parsed: FlatNoBorrow<String> = serde_ipld_dagcbor::from_slice(&bytes).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn strategy_b_dagcbor_roundtrip_nested_smolstr() {
        let original = NestedNoBorrow {
            inner: FlatNoBorrow {
                name: SmolStr::new("bob"),
                label: None,
                tags: vec![],
            },
            count: 99,
        };

        let bytes = serde_ipld_dagcbor::to_vec(&original).unwrap();
        let parsed: NestedNoBorrow<SmolStr> = serde_ipld_dagcbor::from_slice(&bytes).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn strategy_b_dagcbor_borrowed_flat() {
        // DAG-CBOR stores strings as CBOR text strings. Whether borrowed
        // deserialization works depends on whether the deserializer calls
        // visit_borrowed_str. This test documents the actual behaviour.
        let original = FlatNoBorrow {
            name: SmolStr::new("alice"),
            label: Some(SmolStr::new("admin")),
            tags: vec![SmolStr::new("rust")],
        };

        let bytes = serde_ipld_dagcbor::to_vec(&original).unwrap();
        let result: Result<FlatNoBorrow<&str>, _> = serde_ipld_dagcbor::from_slice(&bytes);

        if let Ok(parsed) = &result {
            assert_eq!(parsed.name, "alice");
        }

        // Document the finding regardless of outcome.
        eprintln!(
            "dagcbor borrowed &str deserialization: {}",
            if result.is_ok() {
                "WORKS"
            } else {
                "FAILS (expected — CBOR deserializer may not support borrowing)"
            }
        );
    }

    // -----------------------------------------------------------------------
    // DAG-CBOR — Strategy C
    // -----------------------------------------------------------------------

    #[test]
    fn strategy_c_dagcbor_roundtrip_flat_smolstr() {
        let original = FlatExplicitBound {
            name: SmolStr::new("alice"),
            label: Some(SmolStr::new("admin")),
            tags: vec![SmolStr::new("rust"), SmolStr::new("atproto")],
        };

        let bytes = serde_ipld_dagcbor::to_vec(&original).unwrap();
        let parsed: FlatExplicitBound<SmolStr> = serde_ipld_dagcbor::from_slice(&bytes).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn strategy_c_dagcbor_roundtrip_nested_smolstr() {
        let original = NestedExplicitBound {
            inner: FlatExplicitBound {
                name: SmolStr::new("bob"),
                label: None,
                tags: vec![],
            },
            count: 99,
        };

        let bytes = serde_ipld_dagcbor::to_vec(&original).unwrap();
        let parsed: NestedExplicitBound<SmolStr> = serde_ipld_dagcbor::from_slice(&bytes).unwrap();
        assert_eq!(original, parsed);
    }

    // -----------------------------------------------------------------------
    // Zero-copy verification: prove borrowed &str points into the input buffer
    // -----------------------------------------------------------------------

    /// Returns true if `s` points into the memory range of `buf`.
    fn points_into(s: &str, buf: &str) -> bool {
        let buf_start = buf.as_ptr() as usize;
        let buf_end = buf_start + buf.len();
        let s_start = s.as_ptr() as usize;
        s_start >= buf_start && s_start + s.len() <= buf_end
    }

    /// Same as above but for byte slices.
    fn points_into_bytes(s: &str, buf: &[u8]) -> bool {
        let buf_start = buf.as_ptr() as usize;
        let buf_end = buf_start + buf.len();
        let s_start = s.as_ptr() as usize;
        s_start >= buf_start && s_start + s.len() <= buf_end
    }

    #[test]
    fn json_borrowed_str_is_zero_copy() {
        let input = r#"{"name":"alice","label":"admin","tags":["rust","atproto"]}"#;
        let parsed: FlatNoBorrow<&str> = serde_json::from_str(input).unwrap();

        assert!(
            points_into(parsed.name, input),
            "name should point into input buffer"
        );
        assert!(
            points_into(parsed.label.unwrap(), input),
            "label should point into input buffer"
        );
        for tag in &parsed.tags {
            assert!(
                points_into(tag, input),
                "tag {:?} should point into input buffer",
                tag
            );
        }
    }

    #[test]
    fn dagcbor_borrowed_str_is_zero_copy() {
        let original = FlatNoBorrow {
            name: SmolStr::new("alice"),
            label: Some(SmolStr::new("admin")),
            tags: vec![SmolStr::new("rust")],
        };

        let bytes = serde_ipld_dagcbor::to_vec(&original).unwrap();
        let parsed: FlatNoBorrow<&str> = serde_ipld_dagcbor::from_slice(&bytes).unwrap();

        assert!(
            points_into_bytes(parsed.name, &bytes),
            "name should point into CBOR buffer"
        );
        assert!(
            points_into_bytes(parsed.label.unwrap(), &bytes),
            "label should point into CBOR buffer"
        );
        for tag in &parsed.tags {
            assert!(
                points_into_bytes(tag, &bytes),
                "tag {:?} should point into CBOR buffer",
                tag
            );
        }
    }

    #[test]
    fn json_cowstr_borrows_from_input() {
        // CowStr's Deserialize impl calls visit_borrowed_str -> CowStr::Borrowed,
        // so when deserializing from &str the result should be zero-copy.
        let input = r#"{"name":"alice","label":"admin","tags":["rust","atproto"]}"#;
        let parsed: FlatNoBorrow<CowStr> = serde_json::from_str(input).unwrap();

        assert!(
            matches!(parsed.name, CowStr::Borrowed(_)),
            "name should be CowStr::Borrowed, got Owned"
        );
        assert!(
            points_into(parsed.name.as_str(), input),
            "name should point into input buffer"
        );

        let label = parsed.label.unwrap();
        assert!(
            matches!(label, CowStr::Borrowed(_)),
            "label should be CowStr::Borrowed, got Owned"
        );
        assert!(
            points_into(label.as_str(), input),
            "label should point into input buffer"
        );

        for tag in &parsed.tags {
            assert!(
                matches!(tag, CowStr::Borrowed(_)),
                "tag {:?} should be CowStr::Borrowed, got Owned",
                tag.as_str()
            );
            assert!(
                points_into(tag.as_str(), input),
                "tag {:?} should point into input buffer",
                tag.as_str()
            );
        }
    }

    #[test]
    fn dagcbor_cowstr_borrows_from_buffer() {
        let original = FlatNoBorrow {
            name: SmolStr::new("alice"),
            label: Some(SmolStr::new("admin")),
            tags: vec![SmolStr::new("rust")],
        };

        let bytes = serde_ipld_dagcbor::to_vec(&original).unwrap();
        let parsed: FlatNoBorrow<CowStr> = serde_ipld_dagcbor::from_slice(&bytes).unwrap();

        assert!(
            matches!(parsed.name, CowStr::Borrowed(_)),
            "name should be CowStr::Borrowed, got Owned"
        );
        assert!(
            points_into_bytes(parsed.name.as_str(), &bytes),
            "name should point into CBOR buffer"
        );

        let label = parsed.label.unwrap();
        assert!(
            matches!(label, CowStr::Borrowed(_)),
            "label should be CowStr::Borrowed, got Owned"
        );
        assert!(
            points_into_bytes(label.as_str(), &bytes),
            "label should point into CBOR buffer"
        );

        for tag in &parsed.tags {
            assert!(
                matches!(tag, CowStr::Borrowed(_)),
                "tag {:?} should be CowStr::Borrowed, got Owned",
                tag.as_str()
            );
            assert!(
                points_into_bytes(tag.as_str(), &bytes),
                "tag {:?} should point into CBOR buffer",
                tag.as_str()
            );
        }
    }
}
