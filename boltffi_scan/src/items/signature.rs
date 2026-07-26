use boltffi_ast::{
    ExecutionKind, MethodDef, MethodId, ParameterDef, ParameterPassing, Receiver, ReturnDef,
    Source, SourceName,
};
use syn::spanned::Spanned;

use crate::attributes::Attributes;
use crate::type_expr::Scanner;
use crate::{ScanError, attributes, name, unsupported};

pub(super) fn validate(signature: &syn::Signature, item: String) -> Result<(), ScanError> {
    unsupported::generics(&signature.generics, &item)?;
    unsupported::unsafety(signature.unsafety.as_ref(), &item)?;
    unsupported::extern_abi(signature.abi.as_ref(), &item)?;
    Ok(())
}

pub(super) fn execution(signature: &syn::Signature) -> ExecutionKind {
    match signature.asyncness {
        Some(_) => ExecutionKind::Async,
        None => ExecutionKind::Sync,
    }
}

pub(super) fn method(
    signature: &syn::Signature,
    attrs: &[syn::Attribute],
    source: Source,
    parent: &str,
    scanner: &Scanner<'_>,
    returns: MethodReturns,
) -> Result<MethodDef, ScanError> {
    let ident = &signature.ident;
    validate(signature, format!("method {parent}::{ident}"))?;
    let mut method = MethodDef::new(
        MethodId::new(format!("{parent}::{ident}")),
        name::source(ident),
        receiver(signature),
    );
    let metadata = Attributes::new(attrs, scanner);
    method.execution = execution(signature);
    method.parameters = parameters(signature, scanner)?;
    method.returns = returns.scan(scanner, &signature.output)?;
    method.source = source;
    method.source_span = method.source.span.clone();
    method.doc = metadata.doc();
    method.deprecated = metadata.deprecated()?;
    method.user_attrs = metadata.user_attrs();
    Ok(method)
}

#[derive(Clone, Copy)]
pub(super) enum MethodReturns {
    Export,
    Trait,
}

impl MethodReturns {
    fn scan(self, scanner: &Scanner<'_>, output: &syn::ReturnType) -> Result<ReturnDef, ScanError> {
        match self {
            Self::Export => scanner.scan_export_return(output),
            Self::Trait => scanner.scan_return(output),
        }
    }
}

pub(super) fn parameter(
    typed: &syn::PatType,
    scanner: &Scanner<'_>,
) -> Result<ParameterDef, ScanError> {
    let binding_name = parameter_name(&typed.pat)?;
    let (source_type, passing) = parameter_type(&typed.ty);
    let mut parameter = ParameterDef::value(binding_name, scanner.scan(source_type)?);
    let metadata = Attributes::new(&typed.attrs, scanner);
    parameter.passing = passing;
    parameter.source = attributes::public_source(scanner.scope(), typed.span());
    parameter.doc = metadata.doc();
    parameter.default = metadata.default()?;
    parameter.user_attrs = metadata.user_attrs();
    Ok(parameter)
}

fn receiver(signature: &syn::Signature) -> Receiver {
    match signature.inputs.first() {
        Some(syn::FnArg::Receiver(receiver)) => {
            match (receiver.reference.is_some(), receiver.mutability.is_some()) {
                (true, true) => Receiver::Mutable,
                (true, false) => Receiver::Shared,
                (false, _) => Receiver::Owned,
            }
        }
        _ => Receiver::None,
    }
}

fn parameters(
    signature: &syn::Signature,
    scanner: &Scanner<'_>,
) -> Result<Vec<ParameterDef>, ScanError> {
    signature
        .inputs
        .iter()
        .filter_map(|argument| match argument {
            syn::FnArg::Typed(typed) => Some(parameter(typed, scanner)),
            syn::FnArg::Receiver(_) => None,
        })
        .collect()
}

fn parameter_type(ty: &syn::Type) -> (&syn::Type, ParameterPassing) {
    match ty {
        syn::Type::Reference(reference) => {
            let passing = match reference.mutability {
                Some(_) => ParameterPassing::RefMut,
                None => ParameterPassing::Ref,
            };
            (&reference.elem, passing)
        }
        _ => (ty, ParameterPassing::Value),
    }
}

fn parameter_name(pat: &syn::Pat) -> Result<SourceName, ScanError> {
    match pat {
        syn::Pat::Ident(binding) => Ok(name::source(&binding.ident)),
        _ => Err(ScanError::UnnamedParameter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModuleScope;
    use crate::declared_types::DeclaredTypes;
    use boltffi_ast::{Primitive, TypeExpr};

    fn parameter(source: &str) -> ParameterDef {
        let typed = syn::parse_str::<syn::PatType>(source).expect("parameter");
        let declared_types = DeclaredTypes::new();
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);
        super::parameter(&typed, &scanner).expect("scan")
    }

    #[test]
    fn records_value_parameter_passing() {
        let parameter = parameter("value: i32");

        assert_eq!(parameter.type_expr, TypeExpr::Primitive(Primitive::I32));
        assert_eq!(parameter.passing, ParameterPassing::Value);
    }

    #[test]
    fn records_shared_reference_parameter_passing() {
        let parameter = parameter("value: &i32");

        assert_eq!(parameter.type_expr, TypeExpr::Primitive(Primitive::I32));
        assert_eq!(parameter.passing, ParameterPassing::Ref);
    }

    #[test]
    fn records_mutable_reference_parameter_passing() {
        let parameter = parameter("value: &mut i32");

        assert_eq!(parameter.type_expr, TypeExpr::Primitive(Primitive::I32));
        assert_eq!(parameter.passing, ParameterPassing::RefMut);
    }

    #[test]
    fn preserves_container_shape_after_reference_passing_is_recorded() {
        let parameter = parameter("value: &Vec<u8>");

        assert_eq!(
            parameter.type_expr,
            TypeExpr::vec(TypeExpr::Primitive(Primitive::U8))
        );
        assert_eq!(parameter.passing, ParameterPassing::Ref);
    }

    #[test]
    fn records_result_parameters_as_type_expr() {
        let parameter = parameter("value: Result<u32, String>");

        assert_eq!(
            parameter.type_expr,
            TypeExpr::result(TypeExpr::Primitive(Primitive::U32), TypeExpr::String)
        );
    }

    #[test]
    fn rejects_signature_shapes_not_preserved_by_ast() {
        let generic = syn::parse_str::<syn::Signature>("fn make<T>()").expect("generic signature");
        let unsafe_signature =
            syn::parse_str::<syn::Signature>("unsafe fn free()").expect("unsafe signature");
        let extern_signature =
            syn::parse_str::<syn::Signature>("extern \"C\" fn add()").expect("extern signature");

        assert_eq!(
            validate(&generic, "function make".to_owned()),
            Err(ScanError::UnsupportedGenerics {
                item: "function make".to_owned()
            })
        );
        assert_eq!(
            validate(&unsafe_signature, "function free".to_owned()),
            Err(ScanError::UnsupportedUnsafe {
                item: "function free".to_owned()
            })
        );
        assert_eq!(
            validate(&extern_signature, "function add".to_owned()),
            Err(ScanError::UnsupportedExternAbi {
                item: "function add".to_owned()
            })
        );
    }
}
