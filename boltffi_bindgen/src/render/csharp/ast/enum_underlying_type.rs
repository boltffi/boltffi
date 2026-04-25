use std::fmt;

use crate::ir::types::PrimitiveType;

/// The integral type after `:` in a C# enum declaration. The C#
/// spec calls this the enum's underlying type.
///
/// Examples:
/// ```csharp
/// enum Status : byte { Idle, Active }
/// //            ^^^^
///
/// enum Flags : uint { None = 0, Read = 1 }
/// //           ^^^^
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CSharpEnumUnderlyingType {
    SByte,
    Byte,
    Short,
    UShort,
    Int,
    UInt,
    Long,
    ULong,
}

impl CSharpEnumUnderlyingType {
    /// Lifts an IR primitive into an enum underlying type when the
    /// primitive is one of the eight C# permits. Returns `None` for
    /// `bool`, `f32`, `f64`, `isize`, `usize` so the caller can drop
    /// the enum from the supported set instead of constructing an
    /// illegal `enum : nuint`.
    pub(crate) fn for_primitive(primitive: PrimitiveType) -> Option<Self> {
        match primitive {
            PrimitiveType::I8 => Some(Self::SByte),
            PrimitiveType::U8 => Some(Self::Byte),
            PrimitiveType::I16 => Some(Self::Short),
            PrimitiveType::U16 => Some(Self::UShort),
            PrimitiveType::I32 => Some(Self::Int),
            PrimitiveType::U32 => Some(Self::UInt),
            PrimitiveType::I64 => Some(Self::Long),
            PrimitiveType::U64 => Some(Self::ULong),
            PrimitiveType::Bool
            | PrimitiveType::ISize
            | PrimitiveType::USize
            | PrimitiveType::F32
            | PrimitiveType::F64 => None,
        }
    }
}

impl fmt::Display for CSharpEnumUnderlyingType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SByte => "sbyte",
            Self::Byte => "byte",
            Self::Short => "short",
            Self::UShort => "ushort",
            Self::Int => "int",
            Self::UInt => "uint",
            Self::Long => "long",
            Self::ULong => "ulong",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::i8(PrimitiveType::I8, Some(CSharpEnumUnderlyingType::SByte))]
    #[case::u8(PrimitiveType::U8, Some(CSharpEnumUnderlyingType::Byte))]
    #[case::i32(PrimitiveType::I32, Some(CSharpEnumUnderlyingType::Int))]
    #[case::u64(PrimitiveType::U64, Some(CSharpEnumUnderlyingType::ULong))]
    #[case::bool(PrimitiveType::Bool, None)]
    #[case::f32(PrimitiveType::F32, None)]
    #[case::usize(PrimitiveType::USize, None)]
    fn for_primitive_admits_eight_integrals_rejects_others(
        #[case] primitive: PrimitiveType,
        #[case] expected: Option<CSharpEnumUnderlyingType>,
    ) {
        assert_eq!(CSharpEnumUnderlyingType::for_primitive(primitive), expected);
    }

    #[rstest]
    #[case(CSharpEnumUnderlyingType::SByte, "sbyte")]
    #[case(CSharpEnumUnderlyingType::Byte, "byte")]
    #[case(CSharpEnumUnderlyingType::Short, "short")]
    #[case(CSharpEnumUnderlyingType::UShort, "ushort")]
    #[case(CSharpEnumUnderlyingType::Int, "int")]
    #[case(CSharpEnumUnderlyingType::UInt, "uint")]
    #[case(CSharpEnumUnderlyingType::Long, "long")]
    #[case(CSharpEnumUnderlyingType::ULong, "ulong")]
    fn display_renders_as_csharp_keyword(
        #[case] ty: CSharpEnumUnderlyingType,
        #[case] expected: &str,
    ) {
        assert_eq!(ty.to_string(), expected);
    }
}
