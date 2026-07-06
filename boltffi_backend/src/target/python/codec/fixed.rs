use std::fmt::Write;

use boltffi_binding::{DirectFieldDecl, Primitive, RecordLayout};

use crate::{
    core::{Error, Result},
    target::python::syntax::Identifier,
};

/// The `struct`-module spelling of a direct record's memory image.
///
/// The layout itself is an IR fact shared by every renderer; this value
/// carries only the Python side of it: the precompiled format string and
/// the module-level binding the generated package reads it through.
///
/// # Example
///
/// A record with fields `id: i64, lat: f64, flag: bool` spells as
/// `struct.Struct("<qd?7x")`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedStruct {
    format: String,
    struct_global: Identifier,
}

impl FixedStruct {
    /// Spells the padded memory image of a direct record.
    ///
    /// A direct record crosses encoded containers as its raw memory bytes,
    /// so the format places each scalar at its layout offset and skips
    /// padding with `x` bytes up to the full struct size.
    pub fn from_layout(
        class_name: &Identifier,
        fields: &[DirectFieldDecl],
        layout: &RecordLayout,
    ) -> Result<Self> {
        let mut format = String::from("<");
        let mut cursor = 0u64;
        for (field, slot) in fields.iter().zip(layout.fields()) {
            let offset = slot.offset().get();
            let gap = offset.checked_sub(cursor).ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "direct record layout offsets out of order",
            })?;
            if gap > 0 {
                write!(format, "{gap}x").expect("format string write");
            }
            let primitive = field.ty().primitive();
            format.push(struct_code(primitive)?);
            cursor = offset + primitive.wire_size().get();
        }
        let trailing = layout
            .size()
            .get()
            .checked_sub(cursor)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "direct record layout smaller than its fields",
            })?;
        if trailing > 0 {
            write!(format, "{trailing}x").expect("format string write");
        }
        Ok(Self {
            format,
            struct_global: Identifier::parse(format!("_BOLTFFI_STRUCT_{class_name}"))?,
        })
    }

    /// Returns the `struct` module format string.
    pub fn format(&self) -> &str {
        &self.format
    }

    /// Returns the module-level `struct.Struct` binding.
    pub fn struct_global(&self) -> &Identifier {
        &self.struct_global
    }
}

/// Returns the module-level `struct.Struct` binding for a scalar wire value.
pub fn scalar_struct_global(primitive: Primitive) -> Result<Identifier> {
    let stem = match primitive {
        Primitive::Bool => "BOOL",
        Primitive::I8 => "I8",
        Primitive::U8 => "U8",
        Primitive::I16 => "I16",
        Primitive::U16 => "U16",
        Primitive::I32 => "I32",
        Primitive::U32 => "U32",
        Primitive::I64 | Primitive::ISize => "I64",
        Primitive::U64 | Primitive::USize => "U64",
        Primitive::F32 => "F32",
        Primitive::F64 => "F64",
        _ => {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown primitive wire struct",
            });
        }
    };
    Identifier::parse(format!("_BOLTFFI_STRUCT_{stem}"))
}

fn struct_code(primitive: Primitive) -> Result<char> {
    Ok(match primitive {
        Primitive::Bool => '?',
        Primitive::I8 => 'b',
        Primitive::U8 => 'B',
        Primitive::I16 => 'h',
        Primitive::U16 => 'H',
        Primitive::I32 => 'i',
        Primitive::U32 => 'I',
        Primitive::I64 | Primitive::ISize => 'q',
        Primitive::U64 | Primitive::USize => 'Q',
        Primitive::F32 => 'f',
        Primitive::F64 => 'd',
        _ => {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown primitive wire struct",
            });
        }
    })
}
