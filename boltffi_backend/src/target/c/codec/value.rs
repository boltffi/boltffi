use boltffi_binding::{BinderId, ValueRef, ValueRoot};

use crate::{
    core::{Error, Result},
    target::c::name_style::Name,
};

pub fn reference(value: &ValueRef, root: &str) -> Result<String> {
    let root = match value.root() {
        ValueRoot::SelfValue | ValueRoot::Named(_) | ValueRoot::Local(_) => format!("({root})"),
        ValueRoot::Binder(binder) => format!("(boltffi_bound_{})", binder.raw()),
        _ => {
            return Err(Error::UnsupportedTarget {
                target: "c",
                shape: "codec value root",
            });
        }
    };
    value.path().iter().try_fold(root, |expression, field| {
        let member = Name::field(field)?;
        Ok(format!("({expression}.{member})"))
    })
}

pub fn bind(statements: &str, binder: BinderId, value: &str) -> String {
    statements.replace(&format!("(boltffi_bound_{})", binder.raw()), value)
}
