use boltffi_ast::PackageInfo;
use boltffi_binding::{LowerErrorKind, Native, UnsupportedType, lower};
use boltffi_scan::{ScanError, UnsupportedFeature, scan_file};

#[derive(Clone, Copy)]
enum ExpectedError {
    UnsupportedFeature(UnsupportedFeature),
    UnsupportedGenerics(&'static str),
    UnsupportedUnsafe(&'static str),
    UnsupportedExternAbi(&'static str),
    LowerUnsupportedType(UnsupportedType),
}

struct Fixture {
    name: &'static str,
    source: &'static str,
    expected: ExpectedError,
}

impl Fixture {
    fn assert(self) {
        let file = syn::parse_str(self.source).expect("fixture must parse as Rust syntax");
        let scanned = scan_file(file, PackageInfo::new("demo", None));
        match self.expected {
            ExpectedError::LowerUnsupportedType(expected) => {
                let contract = scanned.expect("fixture should scan before lowering rejects");
                let error = lower::<Native>(&contract).expect_err("fixture should reject in lower");
                assert!(
                    matches!(error.kind(), LowerErrorKind::UnsupportedType(actual) if actual == &expected),
                    "{} expected lower unsupported type {:?}, got {:?}",
                    self.name,
                    expected,
                    error.kind()
                );
            }
            expected => {
                let error = scanned.expect_err("fixture should reject while scanning");
                expected.assert_scan_error(self.name, error);
            }
        }
    }
}

impl ExpectedError {
    fn assert_scan_error(self, name: &str, error: ScanError) {
        let error = error.unlocated().clone();
        match self {
            Self::UnsupportedFeature(expected) => assert_eq!(
                error,
                ScanError::UnsupportedFeature { feature: expected },
                "{name}"
            ),
            Self::UnsupportedGenerics(expected) => assert_eq!(
                error,
                ScanError::UnsupportedGenerics {
                    item: expected.to_owned()
                },
                "{name}"
            ),
            Self::UnsupportedUnsafe(expected) => assert_eq!(
                error,
                ScanError::UnsupportedUnsafe {
                    item: expected.to_owned()
                },
                "{name}"
            ),
            Self::UnsupportedExternAbi(expected) => assert_eq!(
                error,
                ScanError::UnsupportedExternAbi {
                    item: expected.to_owned()
                },
                "{name}"
            ),
            Self::LowerUnsupportedType(_) => unreachable!("lower errors are asserted separately"),
        }
    }
}

#[test]
fn negative_fixtures_document_unsupported_scanner_pipeline_shapes() {
    [
        Fixture {
            name: "tuple struct",
            source: include_str!("../fixtures/negative_scanner/tuple_struct.rs"),
            expected: ExpectedError::UnsupportedFeature(UnsupportedFeature::TupleStruct),
        },
        Fixture {
            name: "unit struct",
            source: include_str!("../fixtures/negative_scanner/unit_struct.rs"),
            expected: ExpectedError::UnsupportedFeature(UnsupportedFeature::UnitStruct),
        },
        Fixture {
            name: "non literal enum discriminant",
            source: include_str!("../fixtures/negative_scanner/non_literal_enum_discriminant.rs"),
            expected: ExpectedError::UnsupportedFeature(
                UnsupportedFeature::NonLiteralEnumDiscriminant,
            ),
        },
        Fixture {
            name: "generic function",
            source: include_str!("../fixtures/negative_scanner/generic_function.rs"),
            expected: ExpectedError::UnsupportedGenerics("function make"),
        },
        Fixture {
            name: "unsafe function",
            source: include_str!("../fixtures/negative_scanner/unsafe_function.rs"),
            expected: ExpectedError::UnsupportedUnsafe("function free_handle"),
        },
        Fixture {
            name: "extern function",
            source: include_str!("../fixtures/negative_scanner/extern_function.rs"),
            expected: ExpectedError::UnsupportedExternAbi("function add"),
        },
        Fixture {
            name: "unsafe function pointer",
            source: include_str!("../fixtures/negative_scanner/unsafe_function_pointer.rs"),
            expected: ExpectedError::UnsupportedFeature(UnsupportedFeature::UnsafeFunctionPointer),
        },
        Fixture {
            name: "extern function pointer",
            source: include_str!("../fixtures/negative_scanner/extern_function_pointer.rs"),
            expected: ExpectedError::UnsupportedFeature(UnsupportedFeature::ExternFunctionPointer),
        },
        Fixture {
            name: "variadic function pointer",
            source: include_str!("../fixtures/negative_scanner/variadic_function_pointer.rs"),
            expected: ExpectedError::UnsupportedFeature(
                UnsupportedFeature::VariadicFunctionPointer,
            ),
        },
        Fixture {
            name: "higher ranked function pointer",
            source: include_str!("../fixtures/negative_scanner/higher_ranked_function_pointer.rs"),
            expected: ExpectedError::UnsupportedFeature(
                UnsupportedFeature::HigherRankedFunctionPointer,
            ),
        },
        Fixture {
            name: "closure record field",
            source: include_str!("../fixtures/negative_scanner/closure_record_field.rs"),
            expected: ExpectedError::LowerUnsupportedType(UnsupportedType::ClosureInValuePosition),
        },
    ]
    .into_iter()
    .for_each(Fixture::assert);
}
