use boltffi_binding::CanonicalName;

use crate::core::name_case;
use crate::target::dart::name_style::Name as NativeName;

pub struct Name<'name>(&'name CanonicalName);

impl<'name> Name<'name> {
    pub fn new(name: &'name CanonicalName) -> Self {
        Self(name)
    }

    pub fn js_export_name(&self) -> String {
        crate::target::typescript::name_style::Name::new(self.0)
            .identifier()
            .expect("canonical export has a JavaScript identifier")
            .to_string()
    }

    pub fn js_member_name(&self) -> String {
        name_case::lower_camel(self.0)
    }

    pub fn dart_identifier(&self) -> String {
        NativeName::new(self.0)
            .lower_camel()
            .expect(
                "a canonical name lowered from a valid Rust identifier is a valid Dart identifier",
            )
            .as_str()
            .to_owned()
    }

    pub fn dart_type_name(&self) -> String {
        NativeName::new(self.0)
            .upper_camel()
            .expect(
                "a canonical name lowered from a valid Rust identifier is a valid Dart identifier",
            )
            .as_str()
            .to_owned()
    }

    pub fn dart_constant_name(&self) -> String {
        self.dart_identifier()
    }
}
