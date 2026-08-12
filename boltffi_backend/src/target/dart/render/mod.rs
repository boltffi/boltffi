mod callback;
mod class;
mod closure;
mod constant;
mod custom_type;
mod direct_vector;
mod documentation;
mod enumeration;
pub mod function;
mod module;
mod record;
mod returned_closure;
mod stream;

pub use callback::Callback;
pub use class::Class;
pub use constant::{AssociatedConstants, Constant};
pub use custom_type::CustomType;
pub use enumeration::Enumeration;
pub use function::Function;
pub use module::Module;
pub use record::Record;
pub use stream::Stream;

use boltffi_binding::{CanonicalName, FieldKey, Primitive};

use crate::core::{Error, Result};

use super::{name_style::Name, native::NativeType, syntax::Identifier};

pub use documentation::Documentation;

pub fn field_name(key: &FieldKey) -> Result<Identifier> {
    match key {
        FieldKey::Named(name) => Name::new(name).lower_camel(),
        FieldKey::Position(position) => Identifier::parse(format!("value{position}")),
        _ => Err(Error::UnexpectedBindingShape {
            layer: "dart declaration",
            shape: "unknown field key",
        }),
    }
}

pub fn primitive_annotation(primitive: Primitive) -> Result<String> {
    NativeType::primitive(primitive).map(|ty| format!("@{}()", ty.native()))
}

pub fn indent(text: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    text.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn declaration_name(name: &CanonicalName) -> Result<Identifier> {
    Name::new(name).upper_camel()
}
