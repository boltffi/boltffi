use crate::ScanError;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UnsupportedFeature {
    TupleStruct,
    UnitStruct,
    NonLiteralEnumDiscriminant,
    UnsafeFunctionPointer,
    ExternFunctionPointer,
    VariadicFunctionPointer,
    HigherRankedFunctionPointer,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UnsupportedInfo {
    pub error: &'static str,
    pub example: &'static str,
    pub message: &'static str,
}

impl UnsupportedFeature {
    pub const ALL: &'static [Self] = &[
        Self::TupleStruct,
        Self::UnitStruct,
        Self::NonLiteralEnumDiscriminant,
        Self::UnsafeFunctionPointer,
        Self::ExternFunctionPointer,
        Self::VariadicFunctionPointer,
        Self::HigherRankedFunctionPointer,
    ];

    pub const fn info(self) -> UnsupportedInfo {
        match self {
            Self::TupleStruct => UnsupportedInfo {
                error: "tuple_struct",
                example: "#[data] pub struct Point(f64, f64);",
                message: "tuple structs are not represented by the record AST",
            },
            Self::UnitStruct => UnsupportedInfo {
                error: "unit_struct",
                example: "#[data] pub struct Marker;",
                message: "unit structs are not represented by the record AST",
            },
            Self::NonLiteralEnumDiscriminant => UnsupportedInfo {
                error: "non_literal_enum_discriminant",
                example: "#[data] pub enum Status { Ok = BASE + 1 }",
                message: "enum discriminants must be integer literals",
            },
            Self::UnsafeFunctionPointer => UnsupportedInfo {
                error: "unsafe_function_pointer",
                example: "unsafe fn(u32)",
                message: "unsafe function pointer types cannot be represented as closure types",
            },
            Self::ExternFunctionPointer => UnsupportedInfo {
                error: "extern_function_pointer",
                example: "extern \"C\" fn(u32)",
                message: "extern function pointer ABI cannot be represented as a closure type",
            },
            Self::VariadicFunctionPointer => UnsupportedInfo {
                error: "variadic_function_pointer",
                example: "fn(u32, ...)",
                message: "variadic function pointer types cannot be represented as closure types",
            },
            Self::HigherRankedFunctionPointer => UnsupportedInfo {
                error: "higher_ranked_function_pointer",
                example: "for<'a> fn(&'a str)",
                message: "higher-ranked function pointer lifetimes cannot be represented as closure types",
            },
        }
    }
}

pub(super) fn feature(feature: UnsupportedFeature) -> ScanError {
    ScanError::UnsupportedFeature { feature }
}

pub fn generics(generics: &syn::Generics, item: &str) -> Result<(), ScanError> {
    if !generics.params.is_empty() || generics.where_clause.is_some() {
        return Err(ScanError::UnsupportedGenerics {
            item: item.to_owned(),
        });
    }
    Ok(())
}

pub fn safety(safety: &syn::Safety, item: &str) -> Result<(), ScanError> {
    match safety {
        syn::Safety::Unsafe(token) => unsafety(Some(token), item),
        _ => Ok(()),
    }
}

pub fn unsafety(unsafety: Option<&syn::token::Unsafe>, item: &str) -> Result<(), ScanError> {
    if unsafety.is_some() {
        return Err(ScanError::UnsupportedUnsafe {
            item: item.to_owned(),
        });
    }
    Ok(())
}

pub fn extern_abi(abi: Option<&syn::Abi>, item: &str) -> Result<(), ScanError> {
    if abi.is_some() {
        return Err(ScanError::UnsupportedExternAbi {
            item: item.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn catalog_entries_are_complete_enough_to_report() {
        assert!(UnsupportedFeature::ALL.iter().all(|feature| {
            let info = feature.info();
            !info.error.is_empty() && !info.example.is_empty() && !info.message.is_empty()
        }));
    }

    #[test]
    fn catalog_error_keys_are_unique() {
        let keys = UnsupportedFeature::ALL
            .iter()
            .map(|feature| feature.info().error)
            .collect::<HashSet<_>>();

        assert_eq!(keys.len(), UnsupportedFeature::ALL.len());
    }

    #[test]
    fn catalog_records_the_current_known_scanner_gaps() {
        assert_eq!(
            UnsupportedFeature::TupleStruct.info(),
            UnsupportedInfo {
                error: "tuple_struct",
                example: "#[data] pub struct Point(f64, f64);",
                message: "tuple structs are not represented by the record AST",
            }
        );
        assert_eq!(
            UnsupportedFeature::NonLiteralEnumDiscriminant.info(),
            UnsupportedInfo {
                error: "non_literal_enum_discriminant",
                example: "#[data] pub enum Status { Ok = BASE + 1 }",
                message: "enum discriminants must be integer literals",
            }
        );
    }
}
