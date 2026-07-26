use boltffi_core::{InternedString, InternedStringPool, InternedStringRepr};

boltffi_core::interned_string_pool! {
    pub BrowserName {
        Chrome = "Chrome",
    }
}

fn main() {
    let value = InternedString::<BrowserName>::from_str("Chrome");
    assert_eq!(value, BrowserName::CHROME);
    assert!(matches!(value.repr(), InternedStringRepr::Interned(0)));
    assert_eq!(BrowserName::VALUES, &["Chrome"]);
}
