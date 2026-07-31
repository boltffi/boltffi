use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Primitive {
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    ISize,
    USize,
    F32,
    F64,
}

impl Primitive {
    pub const fn rust_name(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::I8 => "i8",
            Self::U8 => "u8",
            Self::I16 => "i16",
            Self::U16 => "u16",
            Self::I32 => "i32",
            Self::U32 => "u32",
            Self::I64 => "i64",
            Self::U64 => "u64",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::ISize => "isize",
            Self::USize => "usize",
        }
    }

    pub const fn c_type_name(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::I8 => "int8_t",
            Self::U8 => "uint8_t",
            Self::I16 => "int16_t",
            Self::U16 => "uint16_t",
            Self::I32 => "int32_t",
            Self::U32 => "uint32_t",
            Self::I64 => "int64_t",
            Self::U64 => "uint64_t",
            Self::F32 => "float",
            Self::F64 => "double",
            Self::ISize => "intptr_t",
            Self::USize => "uintptr_t",
        }
    }

    pub const fn ffi_buf_type(self) -> &'static str {
        match self {
            Self::Bool => "FfiBuf_bool",
            Self::I8 => "FfiBuf_i8",
            Self::U8 => "FfiBuf_u8",
            Self::I16 => "FfiBuf_i16",
            Self::U16 => "FfiBuf_u16",
            Self::I32 => "FfiBuf_i32",
            Self::U32 => "FfiBuf_u32",
            Self::I64 => "FfiBuf_i64",
            Self::U64 => "FfiBuf_u64",
            Self::F32 => "FfiBuf_f32",
            Self::F64 => "FfiBuf_f64",
            Self::ISize => "FfiBuf_isize",
            Self::USize => "FfiBuf_usize",
        }
    }

    pub const fn jni_array_type(self) -> &'static str {
        match self {
            Self::Bool => "jbooleanArray",
            Self::I8 | Self::U8 => "jbyteArray",
            Self::I16 | Self::U16 => "jshortArray",
            Self::I32 | Self::U32 => "jintArray",
            Self::I64 | Self::U64 | Self::ISize | Self::USize => "jlongArray",
            Self::F32 => "jfloatArray",
            Self::F64 => "jdoubleArray",
        }
    }

    pub const fn default_value(self) -> &'static str {
        match self {
            Self::Bool => "false",
            Self::F32 | Self::F64 => "0.0",
            _ => "0",
        }
    }

    pub const fn type_id(self) -> &'static str {
        match self {
            Self::Bool => "Bool",
            Self::I8 => "I8",
            Self::U8 => "U8",
            Self::I16 => "I16",
            Self::U16 => "U16",
            Self::I32 => "I32",
            Self::U32 => "U32",
            Self::I64 => "I64",
            Self::U64 => "U64",
            Self::F32 => "F32",
            Self::F64 => "F64",
            Self::ISize => "ISize",
            Self::USize => "USize",
        }
    }

    pub const fn size_bytes(self) -> Option<usize> {
        match self {
            Self::Bool | Self::I8 | Self::U8 => Some(1),
            Self::I16 | Self::U16 => Some(2),
            Self::I32 | Self::U32 | Self::F32 => Some(4),
            Self::I64 | Self::U64 | Self::F64 => Some(8),
            Self::ISize | Self::USize => None,
        }
    }

    pub const fn wire_size_bytes(self) -> usize {
        match self {
            Self::Bool | Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 | Self::ISize | Self::USize => 8,
        }
    }

    pub const fn alignment(self) -> Option<usize> {
        self.size_bytes()
    }

    pub const fn is_signed(self) -> bool {
        matches!(
            self,
            Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::ISize
        )
    }

    pub const fn is_unsigned(self) -> bool {
        matches!(
            self,
            Self::U8 | Self::U16 | Self::U32 | Self::U64 | Self::USize
        )
    }

    pub const fn is_integer(self) -> bool {
        !matches!(self, Self::F32 | Self::F64 | Self::Bool)
    }

    pub const fn is_float(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    pub const fn is_platform_sized(self) -> bool {
        matches!(self, Self::ISize | Self::USize)
    }

    pub const fn fits_in_32_bits(self) -> bool {
        matches!(
            self,
            Self::Bool
                | Self::I8
                | Self::U8
                | Self::I16
                | Self::U16
                | Self::I32
                | Self::U32
                | Self::F32
        )
    }
}

impl FromStr for Primitive {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "bool" => Ok(Self::Bool),
            "i8" => Ok(Self::I8),
            "u8" => Ok(Self::U8),
            "i16" => Ok(Self::I16),
            "u16" => Ok(Self::U16),
            "i32" => Ok(Self::I32),
            "u32" => Ok(Self::U32),
            "i64" => Ok(Self::I64),
            "u64" => Ok(Self::U64),
            "f32" => Ok(Self::F32),
            "f64" => Ok(Self::F64),
            "isize" => Ok(Self::ISize),
            "usize" => Ok(Self::USize),
            _ => Err(()),
        }
    }
}
