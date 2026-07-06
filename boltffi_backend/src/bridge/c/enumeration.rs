use boltffi_binding::{CStyleEnumDecl, DataEnumDecl, EnumDecl, Native};

use crate::core::{Error, Result};

use super::{C_BRIDGE_LAYER, Identifier, Type, name, names::Names};

/// A C enum typedef with integer-valued variants.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Enum {
    name: Identifier,
    repr: Type,
    variants: Vec<EnumVariant>,
}

/// A C enum variant constant.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct EnumVariant {
    name: Identifier,
    value: i128,
}

impl Enum {
    /// Returns the C typedef name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the C integer representation.
    pub fn repr(&self) -> &Type {
        &self.repr
    }

    /// Returns the enum constants in declaration order.
    pub fn variants(&self) -> &[EnumVariant] {
        &self.variants
    }
}

impl Enum {
    /// Creates the C enum declaration for a lowered enum.
    pub fn from_decl(enumeration: &EnumDecl<Native>, names: &Names) -> Result<Self> {
        match enumeration {
            EnumDecl::CStyle(enumeration) => Self::c_style(enumeration, names),
            EnumDecl::Data(enumeration) => Self::data(enumeration, names),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown enum declaration",
            }),
        }
    }

    /// Creates the C enum declaration for a C-style enum.
    pub fn c_style(enumeration: &CStyleEnumDecl<Native>, names: &Names) -> Result<Self> {
        Ok(Self {
            name: names.enumeration(enumeration.id())?,
            repr: Type::primitive(enumeration.repr().primitive())?,
            variants: enumeration
                .variants()
                .iter()
                .map(|variant| {
                    EnumVariant::new(
                        name::EnumConstant::new(enumeration.name(), variant.name()).spelling(),
                        variant.discriminant().get(),
                    )
                })
                .collect::<Result<Vec<_>>>()?,
        })
    }

    fn data(enumeration: &DataEnumDecl<Native>, names: &Names) -> Result<Self> {
        Ok(Self {
            name: names.enumeration(enumeration.id())?,
            repr: Type::Uint32,
            variants: enumeration
                .variants()
                .iter()
                .map(|variant| {
                    EnumVariant::new(
                        name::EnumConstant::new(enumeration.name(), variant.name()).spelling(),
                        i128::from(variant.tag().get()),
                    )
                })
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl EnumVariant {
    /// Returns the C constant name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the integer constant value.
    pub const fn value(&self) -> i128 {
        self.value
    }
}

impl EnumVariant {
    fn new(name: impl Into<String>, value: i128) -> Result<Self> {
        Ok(Self {
            name: Identifier::parse(name)?,
            value,
        })
    }
}
