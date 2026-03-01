use std::fmt;

macro_rules! define_id {
    ($name:ident) => {
        #[derive(Clone, PartialEq, Eq, Hash, Debug)]
        pub struct $name(String);

        impl $name {
            pub fn new(name: impl Into<String>) -> Self {
                Self(name.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self::new(s)
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self::new(s)
            }
        }
    };
}

define_id!(RecordId);
define_id!(EnumId);
define_id!(FunctionId);
define_id!(MethodId);
define_id!(ClassId);
define_id!(CallbackId);
define_id!(CustomTypeId);
define_id!(BuiltinId);
define_id!(StreamId);
define_id!(AsyncIteratorId);
define_id!(FieldName);
define_id!(ParamName);
define_id!(VariantName);

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct QualifiedName(String);

impl QualifiedName {
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for QualifiedName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ConverterPath {
    pub into_ffi: QualifiedName,
    pub try_from_ffi: QualifiedName,
}
