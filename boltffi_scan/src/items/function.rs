use boltffi_ast::{FunctionDef, FunctionId, ParameterDef};
use syn::spanned::Spanned;

use crate::attributes::Attributes;
use crate::declared_types::DeclaredTypes;
use crate::marked::Marked;
use crate::type_expr::Scanner;
use crate::{ModuleScope, ScanError, attributes, name};

use super::signature;

pub fn scan(
    marked: &Marked<'_, syn::ItemFn>,
    declared_types: &DeclaredTypes,
) -> Result<FunctionDef, ScanError> {
    build(marked.item(), marked.scope(), declared_types)
}

fn build(
    item: &syn::ItemFn,
    scope: &ModuleScope,
    declared_types: &DeclaredTypes,
) -> Result<FunctionDef, ScanError> {
    let ident = &item.sig.ident;
    signature::validate(&item.sig, format!("function {ident}"))?;
    let mut function = FunctionDef::new(
        FunctionId::new(scope.path().qualified(&ident.to_string())),
        name::source(ident),
    );
    let scanner = Scanner::new(declared_types, scope);
    let attrs = Attributes::new(&item.attrs, &scanner);
    function.source = attributes::source(&item.vis, scope, item.span());
    function.source_span = function.source.span.clone();
    function.execution = signature::execution(&item.sig);
    function.parameters = parameters(&item.sig, &scanner)?;
    function.returns = scanner.scan_export_return(&item.sig.output)?;
    function.doc = attrs.doc();
    function.deprecated = attrs.deprecated()?;
    function.user_attrs = attrs.user_attrs();
    Ok(function)
}

fn parameters(sig: &syn::Signature, scanner: &Scanner<'_>) -> Result<Vec<ParameterDef>, ScanError> {
    sig.inputs
        .iter()
        .map(|argument| match argument {
            syn::FnArg::Typed(typed) => signature::parameter(typed, scanner),
            syn::FnArg::Receiver(_) => Err(ScanError::ReceiverOnFreeFunction),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use boltffi_ast::{
        AdditionalBound, BaseTrait, CallableForm, CanonicalName, ExecutionKind, FnTraitKind,
        NamePart, ParameterPassing, Path, Primitive, RecordId, ReturnDef, Source, TypeExpr,
        Visibility,
    };

    fn parse(source: &str) -> syn::ItemFn {
        syn::parse_str(source).expect("valid function source")
    }

    fn scan(source: &str) -> Result<FunctionDef, ScanError> {
        super::build(
            &parse(source),
            &ModuleScope::root("demo"),
            &DeclaredTypes::new(),
        )
    }

    fn name(parts: &[&str]) -> CanonicalName {
        CanonicalName::new(parts.iter().copied().map(NamePart::new).collect())
    }

    fn value_return(return_def: &ReturnDef) -> &TypeExpr {
        match return_def {
            ReturnDef::Value(type_expr) => type_expr,
            ReturnDef::Void => panic!("expected value return"),
        }
    }

    #[test]
    fn scans_complete_primitive_free_function_contract() {
        let function = scan("pub fn add(a: i32, b: i32) -> i32 { a + b }").expect("scan");
        let mut expected = FunctionDef::new(FunctionId::new("demo::add"), name(&["add"]));
        expected.form = CallableForm::Function;
        expected.execution = ExecutionKind::Sync;
        expected.parameters = vec![
            ParameterDef::value(name(&["a"]), TypeExpr::Primitive(Primitive::I32)),
            ParameterDef::value(name(&["b"]), TypeExpr::Primitive(Primitive::I32)),
        ];
        expected.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::I32));
        expected.source = Source::new(Visibility::Public, None);

        assert_eq!(function, expected);
    }

    #[test]
    fn preserves_function_and_parameter_source_spelling() {
        let function = scan("pub fn HTTPRequest(r#type: i32) -> i32 { r#type }").expect("scan");

        assert_eq!(function.name.spelling(), "HTTPRequest");
        assert_eq!(function.name.canonical(), &name(&["http", "request"]));
        assert_eq!(function.parameters[0].name.spelling(), "r#type");
        assert_eq!(
            function.parameters[0].name.canonical(),
            &CanonicalName::single("type")
        );
    }

    #[test]
    fn explicit_and_implicit_unit_returns_are_void() {
        let explicit = scan("pub fn explicit() -> () {}").expect("scan");
        let implicit = scan("pub fn implicit() {}").expect("scan");

        assert_eq!(explicit.returns, ReturnDef::Void);
        assert_eq!(implicit.returns, ReturnDef::Void);
    }

    #[test]
    fn scans_parenthesized_parameter_and_return_types() {
        let function = scan("pub fn id(value: (i32)) -> (i32) { value }").expect("scan");

        assert_eq!(
            function.parameters[0].type_expr,
            TypeExpr::Primitive(Primitive::I32)
        );
        assert_eq!(
            function.returns,
            ReturnDef::value(TypeExpr::Primitive(Primitive::I32))
        );
    }

    #[test]
    fn scans_borrowed_string_return_as_string_slice() {
        let function = scan("pub fn label() -> &'static str { \"up\" }").expect("scan");

        assert_eq!(function.returns, ReturnDef::value(TypeExpr::Str));
    }

    #[test]
    fn scans_borrowed_byte_slice_return_as_slice() {
        let function = scan("pub fn bytes() -> &'static [u8] { b\"up\" }").expect("scan");

        assert_eq!(
            function.returns,
            ReturnDef::value(TypeExpr::slice(TypeExpr::Primitive(Primitive::U8)))
        );
    }

    #[test]
    fn async_function_records_only_execution_change() {
        let function = scan("pub async fn spin() {}").expect("scan");

        assert_eq!(function.execution, ExecutionKind::Async);
        assert_eq!(function.form, CallableForm::Function);
        assert!(function.parameters.is_empty());
        assert_eq!(function.returns, ReturnDef::Void);
    }

    #[test]
    fn closure_return_records_complete_signature_contract() {
        let function =
            scan("pub fn make_handler() -> impl Send + FnMut(u32, bool) -> i64 { todo!() }")
                .expect("scan");

        let ReturnDef::Value(TypeExpr::ImplTrait(bounds)) = function.returns else {
            panic!("expected closure return");
        };
        let BaseTrait::Function(function_trait) = bounds.base else {
            panic!("expected function trait base");
        };
        assert_eq!(function_trait.kind, FnTraitKind::FnMut);
        assert_eq!(
            bounds.bounds,
            vec![AdditionalBound::AutoTrait(Path::single("Send"))]
        );
        assert_eq!(
            function_trait
                .signature
                .parameters
                .iter()
                .collect::<Vec<_>>(),
            vec![
                &TypeExpr::Primitive(Primitive::U32),
                &TypeExpr::Primitive(Primitive::Bool)
            ]
        );
        assert_eq!(
            function_trait.signature.returns,
            ReturnDef::value(TypeExpr::Primitive(Primitive::I64))
        );
    }

    #[test]
    fn closure_return_without_arrow_records_void_invoke_return() {
        let function = scan("pub fn make_handler() -> impl FnOnce(u32) { todo!() }").expect("scan");

        let ReturnDef::Value(TypeExpr::ImplTrait(bounds)) = function.returns else {
            panic!("expected closure return");
        };
        let BaseTrait::Function(function_trait) = bounds.base else {
            panic!("expected function trait base");
        };

        assert_eq!(function_trait.kind, FnTraitKind::FnOnce);
        assert_eq!(
            function_trait
                .signature
                .parameters
                .iter()
                .collect::<Vec<_>>(),
            vec![&TypeExpr::Primitive(Primitive::U32)]
        );
        assert_eq!(function_trait.signature.returns, ReturnDef::Void);
    }

    #[test]
    fn non_primitive_parameter_is_rejected_without_registration() {
        let error = scan("pub fn make(point: Point) {}").expect_err("unregistered type rejects");

        assert!(matches!(
            error,
            ScanError::UnsupportedType { spelling } if spelling == "Point"
        ));
    }

    #[test]
    fn resolves_record_typed_parameter_and_return_against_registry() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::Point"));
        let function = super::build(
            &parse("pub fn translate(point: Point, dx: f64) -> Point { point }"),
            &ModuleScope::root("demo"),
            &declared_types,
        )
        .expect("scan");

        assert_eq!(
            function.parameters[0].type_expr,
            TypeExpr::record(RecordId::new("demo::Point"), Path::single("Point"))
        );
        assert_eq!(
            value_return(&function.returns),
            &TypeExpr::record(RecordId::new("demo::Point"), Path::single("Point"))
        );
    }

    #[test]
    fn scans_restricted_function_visibility_without_touching_parameters() {
        let function = scan("pub(crate) fn add(a: i32) -> i32 { a }").expect("scan");

        assert_eq!(
            function.source.visibility,
            Visibility::Restricted("crate".to_owned())
        );
        assert_eq!(function.parameters[0].source.visibility, Visibility::Public);
        assert_eq!(function.parameters[0].passing, ParameterPassing::Value);
    }

    #[test]
    fn scans_multi_word_function_and_parameter_names_as_parts() {
        let function = scan("pub fn make_handler(user_id: i32) -> i32 { user_id }").expect("scan");

        assert_eq!(function.id, FunctionId::new("demo::make_handler"));
        assert_eq!(function.name.canonical(), &name(&["make", "handler"]));
        assert_eq!(
            function.parameters[0].name.canonical(),
            &name(&["user", "id"])
        );
    }

    #[test]
    fn rejects_receiver_on_free_function() {
        let error = scan("pub fn distance(&self) -> f64 { 0.0 }").expect_err("receiver rejected");

        assert_eq!(error, ScanError::ReceiverOnFreeFunction);
    }

    #[test]
    fn rejects_non_binding_parameter_pattern_before_type_scanning() {
        let error =
            scan("pub fn sum((x, y): (i32, i32)) -> i32 { x + y }").expect_err("pattern rejected");

        assert_eq!(error, ScanError::UnnamedParameter);
    }

    #[test]
    fn rejects_generic_function_before_erasing_type_parameters() {
        let error = scan("pub fn make<T>() -> i32 { 0 }").expect_err("generic rejected");

        assert_eq!(
            error,
            ScanError::UnsupportedGenerics {
                item: "function make".to_owned()
            }
        );
    }

    #[test]
    fn rejects_unsafe_function_before_erasing_unsafety() {
        let error = scan("pub unsafe fn free_handle(handle: i32) {}").expect_err("unsafe rejected");

        assert_eq!(
            error,
            ScanError::UnsupportedUnsafe {
                item: "function free_handle".to_owned()
            }
        );
    }

    #[test]
    fn rejects_extern_function_before_erasing_abi() {
        let error = scan("pub extern \"C\" fn add(value: i32) -> i32 { value }")
            .expect_err("extern rejected");

        assert_eq!(
            error,
            ScanError::UnsupportedExternAbi {
                item: "function add".to_owned()
            }
        );
    }
}
