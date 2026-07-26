boltffi_renamed::interned_string_pool! {
    pub FacadeBrowserName {
        Chrome = "Chrome",
    }
}

fn main() {
    let value = boltffi_renamed::InternedString::<FacadeBrowserName>::from_str("Chrome");
    assert_eq!(value, FacadeBrowserName::CHROME);
}
