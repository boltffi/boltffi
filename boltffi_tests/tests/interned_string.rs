use boltffi::__private::wire::{WireDecode, WireEncode};
use boltffi::{InternedString, InternedStringPool, InternedStringRepr};

boltffi::interned_string_pool! {
    pub BrowserName {
        Chrome = "Chrome",
        Safari = "Safari",
    }
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
