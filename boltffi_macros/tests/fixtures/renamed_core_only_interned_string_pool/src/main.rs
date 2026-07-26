boltffi_core_renamed::interned_string_pool! {
    pub BrowserName {
        Safari = "Safari",
    }
}

fn main() {
    let value = boltffi_core_renamed::InternedString::<BrowserName>::from_str("Safari");
    assert_eq!(value, BrowserName::SAFARI);
}
