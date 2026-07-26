use std::collections::{BTreeMap, HashMap};

use boltffi::__private::wire::{WireDecode, WireEncode};
use boltffi::{InternedString, InternedStringPool, InternedStringRepr};

boltffi::interned_string_pool! {
    pub BrowserName {
        Chrome = "Chrome",
        Safari = "Safari",
    }
}

#[derive(Debug)]
struct DuplicatePool;

impl InternedStringPool for DuplicatePool {
    const VALUES: &'static [&'static str] = &["same", "same"];
}

#[test]
fn interned_string_pool_macro_generates_static_constants() {
    let value = BrowserName::CHROME;
    assert!(matches!(value.repr(), InternedStringRepr::Interned(0)));
    assert_eq!(BrowserName::VALUES, &["Chrome", "Safari"]);
}

#[test]
fn interned_string_from_str_uses_static_pool_when_possible() {
    let value = InternedString::<BrowserName>::from_str("Safari");
    assert!(matches!(value.repr(), InternedStringRepr::Interned(1)));
}

#[test]
fn interned_string_from_str_uses_dynamic_fallback_for_unknown_values() {
    let value = InternedString::<BrowserName>::from_str("Unknown");
    assert!(matches!(value.repr(), InternedStringRepr::Dynamic(text) if text == "Unknown"));
}

#[test]
fn interned_string_wire_encoding_uses_tagged_static_or_dynamic_payload() {
    let static_value = BrowserName::CHROME;
    let mut buffer = vec![0; static_value.wire_size()];
    let written = static_value.encode_to(&mut buffer);
    assert_eq!(written, 5);
    assert_eq!(buffer, vec![0, 0, 0, 0, 0]);

    let dynamic_value = InternedString::<BrowserName>::from_str("Unknown");
    let mut buffer = vec![0; dynamic_value.wire_size()];
    let written = dynamic_value.encode_to(&mut buffer);
    assert_eq!(written, 1 + 4 + "Unknown".len());
    assert_eq!(buffer[0], 1);
    assert_eq!(&buffer[1..5], &("Unknown".len() as u32).to_le_bytes());
    assert_eq!(&buffer[5..], b"Unknown");
}

#[test]
fn interned_string_wire_decode_round_trips_static_and_dynamic_values() {
    let (static_value, used) = InternedString::<BrowserName>::decode_from(&[0, 1, 0, 0, 0])
        .expect("static interned string decodes");
    assert_eq!(used, 5);
    assert!(matches!(
        static_value.repr(),
        InternedStringRepr::Interned(1)
    ));

    let mut dynamic = vec![1];
    dynamic.extend_from_slice(&("Unknown".len() as u32).to_le_bytes());
    dynamic.extend_from_slice(b"Unknown");
    let (dynamic_value, used) = InternedString::<BrowserName>::decode_from(&dynamic)
        .expect("dynamic interned string decodes");
    assert_eq!(used, dynamic.len());
    assert!(matches!(dynamic_value.repr(), InternedStringRepr::Dynamic(text) if text == "Unknown"));
}

#[test]
fn interned_string_wire_decode_canonicalizes_duplicate_pool_ids() {
    let (decoded, used) = InternedString::<DuplicatePool>::decode_from(&[0, 1, 0, 0, 0])
        .expect("duplicate pool id decodes");
    let canonical = InternedString::<DuplicatePool>::from_str("same");

    assert_eq!(used, 5);
    assert!(matches!(decoded.repr(), InternedStringRepr::Interned(0)));
    assert_eq!(decoded, canonical);

    let keys = HashMap::from([(canonical, "canonical")]);
    assert_eq!(keys.get(&decoded), Some(&"canonical"));
}

#[test]
fn interned_string_dynamic_constructor_canonicalizes_known_pool_values() {
    // Regression: dynamic("Chrome") must equal BrowserName::CHROME, not a
    // Dynamic variant that compares unequal due to different representations.
    let via_dynamic = InternedString::<BrowserName>::dynamic("Chrome");
    assert_eq!(
        via_dynamic,
        BrowserName::CHROME,
        "dynamic(\"Chrome\") must equal the pool constant"
    );
    assert!(
        matches!(via_dynamic.repr(), InternedStringRepr::Interned(0)),
        "dynamic with known value must canonicalize to Interned repr"
    );

    // Unknown values remain dynamic.
    let unknown = InternedString::<BrowserName>::dynamic("Firefox");
    assert!(
        matches!(unknown.repr(), InternedStringRepr::Dynamic(text) if text == "Firefox"),
        "dynamic with unknown value stays Dynamic"
    );
}

#[test]
fn interned_string_wire_decode_canonicalizes_known_dynamic_payloads() {
    let mut dynamic = vec![1];
    dynamic.extend_from_slice(&("Chrome".len() as u32).to_le_bytes());
    dynamic.extend_from_slice(b"Chrome");

    let (value, used) = InternedString::<BrowserName>::decode_from(&dynamic)
        .expect("dynamic interned string decodes");

    assert_eq!(used, dynamic.len());
    assert_eq!(value, BrowserName::CHROME);
    assert!(matches!(value.repr(), InternedStringRepr::Interned(0)));
}

#[test]
fn interned_strings_are_hash_and_ordered_map_keys_without_pool_bounds() {
    struct UnboundedPool;
    fn requires_hash_and_order<T: std::hash::Hash + Ord>() {}
    requires_hash_and_order::<InternedString<UnboundedPool>>();

    let static_key = BrowserName::CHROME;
    let canonical_static = InternedString::<BrowserName>::dynamic("Chrome");
    let dynamic_key = InternedString::<BrowserName>::dynamic("Firefox");
    assert_eq!(static_key, canonical_static);

    let hash = HashMap::from([
        (static_key, 1_u32),
        (canonical_static, 2),
        (dynamic_key.clone(), 3),
    ]);
    assert_eq!(hash.len(), 2);
    assert_eq!(hash[&BrowserName::CHROME], 2);
    assert_eq!(hash[&dynamic_key], 3);

    let ordered = BTreeMap::from([(BrowserName::CHROME, 2_u32), (dynamic_key.clone(), 3)]);
    assert_eq!(ordered[&BrowserName::CHROME], 2);
    assert_eq!(ordered[&dynamic_key], 3);
}

#[test]
fn interned_string_map_keys_round_trip_over_the_wire() {
    let static_key = BrowserName::SAFARI;
    let dynamic_key = InternedString::<BrowserName>::dynamic("Firefox");

    let hash = HashMap::from([(static_key.clone(), 7_u32), (dynamic_key.clone(), 11)]);
    let mut hash_buffer = vec![0; hash.wire_size()];
    let hash_written = hash.encode_to(&mut hash_buffer);
    let (decoded_hash, hash_used) =
        HashMap::<InternedString<BrowserName>, u32>::decode_from(&hash_buffer)
            .expect("hash map with interned keys decodes");
    assert_eq!(hash_written, hash_used);
    assert_eq!(decoded_hash, hash);

    let ordered = BTreeMap::from([(static_key, 13_u32), (dynamic_key, 17)]);
    let mut ordered_buffer = vec![0; ordered.wire_size()];
    let ordered_written = ordered.encode_to(&mut ordered_buffer);
    let (decoded_ordered, ordered_used) =
        BTreeMap::<InternedString<BrowserName>, u32>::decode_from(&ordered_buffer)
            .expect("ordered map with interned keys decodes");
    assert_eq!(ordered_written, ordered_used);
    assert_eq!(decoded_ordered, ordered);
}
