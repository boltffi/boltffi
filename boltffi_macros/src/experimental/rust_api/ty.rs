use boltffi_ast::{
    AdditionalBound, BaseTrait, BuiltinType, ConstExpr, FnSig, GenericArgument, Literal, MapKind,
    ParameterPassing, Path, PathRoot, Primitive, ReturnDef, TraitBounds, TypeExpr,
};
use boltffi_binding::Receive;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type, parse_str, parse2};

use crate::experimental::error::Error;

pub struct TypeTokens {
    ty: Type,
}

impl TypeTokens {
    pub fn new(type_expr: &TypeExpr) -> Result<Self, Error> {
        Ok(Self {
            ty: parse2(Self::tokens(type_expr)?).map_err(|_| {
                Error::SourceSyntaxMismatch("source type expression is not a Rust type")
            })?,
        })
    }

    pub fn parameter(passing: ParameterPassing, type_expr: &TypeExpr) -> Result<Self, Error> {
        let type_tokens = Self::tokens(type_expr)?;
        let tokens = match passing {
            ParameterPassing::Value => type_tokens,
            ParameterPassing::Ref => quote! { &#type_tokens },
            ParameterPassing::RefMut => quote! { &mut #type_tokens },
        };
        Ok(Self {
            ty: parse2(tokens).map_err(|_| {
                Error::SourceSyntaxMismatch("source parameter type is not Rust syntax")
            })?,
        })
    }

    pub fn into_type(self) -> Type {
        self.ty
    }
}

#[derive(Clone)]
pub struct DecodeTarget {
    parameter: Type,
    owned: Type,
    borrow: DecodeBorrow,
    source: TypeExpr,
}

impl DecodeTarget {
    pub fn new(
        passing: ParameterPassing,
        receive: Receive,
        type_expr: &TypeExpr,
    ) -> Result<Self, Error> {
        match (passing, receive) {
            (ParameterPassing::Value, Receive::ByValue) => {
                let ty = TypeTokens::new(type_expr)?.into_type();
                Ok(Self {
                    parameter: ty.clone(),
                    owned: ty,
                    borrow: DecodeBorrow::Owned,
                    source: type_expr.clone(),
                })
            }
            (ParameterPassing::Ref, Receive::ByRef) => Ok(Self::borrowed(
                TypeTokens::parameter(passing, type_expr)?.into_type(),
                type_expr,
                false,
            )?),
            (ParameterPassing::RefMut, Receive::ByMutRef) => Ok(Self::borrowed(
                TypeTokens::parameter(passing, type_expr)?.into_type(),
                type_expr,
                true,
            )?),
            _ => Err(Error::SourceSyntaxMismatch(
                "source parameter passing does not match binding receive mode",
            )),
        }
    }

    pub fn received(receive: Receive, type_expr: &TypeExpr) -> Result<Self, Error> {
        let passing = match receive {
            Receive::ByValue => ParameterPassing::Value,
            Receive::ByRef => ParameterPassing::Ref,
            Receive::ByMutRef => ParameterPassing::RefMut,
            _ => {
                return Err(Error::UnsupportedExpansion(
                    "unknown encoded parameter receive mode",
                ));
            }
        };
        Self::new(passing, receive, type_expr)
    }

    pub fn by_value(type_expr: &TypeExpr) -> Result<Self, Error> {
        Self::new(ParameterPassing::Value, Receive::ByValue, type_expr)
    }

    pub fn parameter(&self) -> &Type {
        &self.parameter
    }

    pub fn owned(&self) -> &Type {
        &self.owned
    }

    pub const fn borrow(&self) -> DecodeBorrow {
        self.borrow
    }

    pub fn source(&self) -> &TypeExpr {
        &self.source
    }

    /// Returns true when the source type is a slice (`&mut [u8]`), as opposed to an owned
    /// collection (`&mut Vec<u8>`). Used to select direct mutable-slice ABI vs wire-decode.
    pub fn is_slice_source(&self) -> bool {
        matches!(&self.source, TypeExpr::Slice(_))
    }

    pub fn incoming_encoded_type(&self) -> IncomingEncodedType {
        IncomingEncodedType::new(&self.source)
    }

    fn borrowed(parameter: Type, type_expr: &TypeExpr, mutable: bool) -> Result<Self, Error> {
        let (owned, borrow) = match type_expr {
            TypeExpr::Str => (
                TypeTokens::new(&TypeExpr::String)?.into_type(),
                DecodeBorrow::Str { mutable },
            ),
            TypeExpr::Slice(element) => (
                TypeTokens::new(&TypeExpr::Vec(element.clone()))?.into_type(),
                DecodeBorrow::Slice { mutable },
            ),
            _ => (
                TypeTokens::new(type_expr)?.into_type(),
                DecodeBorrow::Value { mutable },
            ),
        };
        Ok(Self {
            parameter,
            owned,
            borrow,
            source: type_expr.clone(),
        })
    }
}

pub struct IncomingEncodedType {
    type_expr: TypeExpr,
}

impl IncomingEncodedType {
    pub fn new(type_expr: &TypeExpr) -> Self {
        Self {
            type_expr: type_expr.clone(),
        }
    }

    pub fn require_supported(self) -> Result<(), Error> {
        self.require_supported_type(&self.type_expr)
    }

    fn require_supported_type(&self, type_expr: &TypeExpr) -> Result<(), Error> {
        match type_expr {
            TypeExpr::Option(inner)
            | TypeExpr::Vec(inner)
            | TypeExpr::Slice(inner)
            | TypeExpr::Boxed(inner)
            | TypeExpr::Arc(inner) => self.require_supported_type(inner),
            TypeExpr::Result { ok, err } => {
                self.require_supported_type(ok)?;
                self.require_supported_type(err)
            }
            TypeExpr::Tuple(elements) => elements
                .iter()
                .try_for_each(|element| self.require_supported_type(element)),
            TypeExpr::Map { key, value, .. } => {
                MapKey::new(key).require_supported()?;
                self.require_supported_type(value)
            }
            _ => Ok(()),
        }
    }
}

struct MapKey {
    type_expr: TypeExpr,
}

impl MapKey {
    fn new(type_expr: &TypeExpr) -> Self {
        Self {
            type_expr: type_expr.clone(),
        }
    }

    fn require_supported(self) -> Result<(), Error> {
        self.require_key_type(&self.type_expr)
    }

    fn require_key_type(&self, type_expr: &TypeExpr) -> Result<(), Error> {
        match type_expr {
            TypeExpr::Primitive(Primitive::F32 | Primitive::F64) => Err(
                Error::UnsupportedExpansion("floating-point encoded map key"),
            ),
            TypeExpr::Custom { .. } => Err(Error::UnsupportedExpansion("custom encoded map key")),
            TypeExpr::Str | TypeExpr::Slice(_) => {
                Err(Error::UnsupportedExpansion("unsized encoded map key"))
            }
            TypeExpr::Option(inner)
            | TypeExpr::Vec(inner)
            | TypeExpr::Boxed(inner)
            | TypeExpr::Arc(inner) => self.require_key_type(inner),
            TypeExpr::Result { ok, err } => {
                self.require_key_type(ok)?;
                self.require_key_type(err)
            }
            TypeExpr::Tuple(elements) => elements
                .iter()
                .try_for_each(|element| self.require_key_type(element)),
            TypeExpr::Map { .. } => Err(Error::UnsupportedExpansion("nested encoded map key")),
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Copy)]
pub enum DecodeBorrow {
    Owned,
    Value { mutable: bool },
    Slice { mutable: bool },
    Str { mutable: bool },
}

impl DecodeBorrow {
    pub const fn mutable(self) -> bool {
        match self {
            Self::Owned => false,
            Self::Value { mutable } | Self::Slice { mutable } | Self::Str { mutable } => mutable,
        }
    }
}

impl TypeTokens {
    fn tokens(type_expr: &TypeExpr) -> Result<TokenStream, Error> {
        Ok(match type_expr {
            TypeExpr::Primitive(primitive) => {
                let primitive_type: Type = parse_str(primitive.rust_name()).map_err(|_| {
                    Error::SourceSyntaxMismatch("primitive type is not Rust syntax")
                })?;
                quote! { #primitive_type }
            }
            TypeExpr::Unit => quote! { () },
            TypeExpr::String => quote! { String },
            TypeExpr::Str => quote! { str },
            TypeExpr::Builtin(kind) => match kind {
                BuiltinType::Duration => quote! { ::std::time::Duration },
                BuiltinType::SystemTime => quote! { ::std::time::SystemTime },
                BuiltinType::Uuid => quote! { ::uuid::Uuid },
                BuiltinType::Url => quote! { ::url::Url },
            },
            TypeExpr::Record { path, .. }
            | TypeExpr::Enum { path, .. }
            | TypeExpr::Class { path, .. }
            | TypeExpr::Custom { path, .. } => Self::path_tokens(path)?,
            TypeExpr::InternedString { pool, .. } => {
                let pool = Self::path_tokens(pool)?;
                quote! { ::boltffi::InternedString<#pool> }
            }
            TypeExpr::Dyn(bound) => {
                let bound = Self::trait_bound_tokens(bound)?;
                quote! { dyn #bound }
            }
            TypeExpr::ImplTrait(bound) => {
                let bound = Self::trait_bound_tokens(bound)?;
                quote! { impl #bound }
            }
            TypeExpr::Boxed(inner) => {
                let inner = Self::tokens(inner)?;
                quote! { Box<#inner> }
            }
            TypeExpr::Arc(inner) => {
                let inner = Self::tokens(inner)?;
                quote! { ::std::sync::Arc<#inner> }
            }
            TypeExpr::FnPtr(signature) => Self::fn_pointer_tokens(signature)?,
            TypeExpr::Vec(element) => {
                let element = Self::tokens(element)?;
                quote! { Vec<#element> }
            }
            TypeExpr::Slice(element) => {
                let element = Self::tokens(element)?;
                quote! { [#element] }
            }
            TypeExpr::Option(inner) => {
                let inner = Self::tokens(inner)?;
                quote! { Option<#inner> }
            }
            TypeExpr::Result { ok, err } => {
                let ok = Self::tokens(ok)?;
                let err = Self::tokens(err)?;
                quote! { Result<#ok, #err> }
            }
            TypeExpr::Tuple(elements) => Self::tuple_tokens(elements)?,
            TypeExpr::Map { kind, key, value } => {
                let key = Self::tokens(key)?;
                let value = Self::tokens(value)?;
                match kind {
                    MapKind::Hash => quote! { ::std::collections::HashMap<#key, #value> },
                    MapKind::BTree => quote! { ::std::collections::BTreeMap<#key, #value> },
                }
            }
            TypeExpr::SelfType => quote! { Self },
            TypeExpr::Parameter(parameter) => {
                let ident: Ident = parse_str(&parameter.name).map_err(|_| {
                    Error::SourceSyntaxMismatch("type parameter is not an identifier")
                })?;
                quote! { #ident }
            }
        })
    }

    fn trait_bound_tokens(bounds: &TraitBounds) -> Result<TokenStream, Error> {
        let base = match &bounds.base {
            BaseTrait::Named { path, .. } => Self::path_tokens(path)?,
            BaseTrait::Function(function_trait) => {
                let name = function_trait.kind.as_ref();
                let trait_ident: Ident = parse_str(name).map_err(|_| {
                    Error::SourceSyntaxMismatch("closure trait name is not an identifier")
                })?;
                let parameters = function_trait
                    .signature
                    .parameters
                    .iter()
                    .map(Self::tokens)
                    .collect::<Result<Vec<_>, _>>()?;
                match &function_trait.signature.returns {
                    ReturnDef::Void => quote! { #trait_ident(#(#parameters),*) },
                    ReturnDef::Value(return_type) => {
                        let return_type = Self::tokens(return_type)?;
                        quote! { #trait_ident(#(#parameters),*) -> #return_type }
                    }
                }
            }
        };
        let additional = bounds
            .bounds
            .iter()
            .map(Self::additional_bound_tokens)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(quote! { #base #(+ #additional)* })
    }

    fn additional_bound_tokens(bound: &AdditionalBound) -> Result<TokenStream, Error> {
        match bound {
            AdditionalBound::AutoTrait(path) => Self::path_tokens(path),
            AdditionalBound::Lifetime(lifetime) => parse_str::<syn::Lifetime>(lifetime)
                .map(|lifetime| quote! { #lifetime })
                .map_err(|_| {
                    Error::SourceSyntaxMismatch("trait lifetime bound is not Rust syntax")
                }),
        }
    }

    fn fn_pointer_tokens(signature: &FnSig) -> Result<TokenStream, Error> {
        let parameters = signature
            .parameters
            .iter()
            .map(Self::tokens)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(match &signature.returns {
            ReturnDef::Void => quote! { fn(#(#parameters),*) },
            ReturnDef::Value(return_type) => {
                let return_type = Self::tokens(return_type)?;
                quote! { fn(#(#parameters),*) -> #return_type }
            }
        })
    }

    fn tuple_tokens(elements: &[TypeExpr]) -> Result<TokenStream, Error> {
        let elements = elements
            .iter()
            .map(Self::tokens)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(match elements.as_slice() {
            [] => quote! { () },
            [element] => quote! { (#element,) },
            _ => quote! { (#(#elements),*) },
        })
    }

    fn path_tokens(path: &Path) -> Result<TokenStream, Error> {
        let path = Self::path_string(path)?;
        parse_str::<Type>(&path)
            .map(|ty| quote!(#ty))
            .map_err(|_| Error::SourceSyntaxMismatch("source path is not a Rust type path"))
    }

    fn path_string(path: &Path) -> Result<String, Error> {
        let prefix = match path.root {
            PathRoot::Relative => String::new(),
            PathRoot::Crate => "crate::".to_owned(),
            PathRoot::Self_ => "self::".to_owned(),
            PathRoot::Super(count) => {
                std::iter::repeat_n("super", count.get())
                    .collect::<Vec<_>>()
                    .join("::")
                    + "::"
            }
            PathRoot::Absolute => "::".to_owned(),
        };
        let segments = path
            .segments
            .iter()
            .map(|segment| {
                let arguments = segment
                    .arguments
                    .iter()
                    .map(Self::generic_argument_string)
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(match arguments.is_empty() {
                    true => segment.name.as_str().to_owned(),
                    false => format!("{}<{}>", segment.name.as_str(), arguments.join(", ")),
                })
            })
            .collect::<Result<Vec<_>, Error>>()?
            .join("::");
        Ok(format!("{prefix}{segments}"))
    }

    fn generic_argument_string(argument: &GenericArgument) -> Result<String, Error> {
        match argument {
            GenericArgument::Type(type_expr) => Self::type_string(type_expr),
            GenericArgument::Const(value) => Self::const_expr_string(value),
            GenericArgument::AssociatedType { name, type_expr } => Ok(format!(
                "{} = {}",
                name.as_str(),
                Self::type_string(type_expr)?
            )),
        }
    }

    fn type_string(type_expr: &TypeExpr) -> Result<String, Error> {
        TypeTokens::new(type_expr).map(|tokens| {
            let ty = tokens.into_type();
            quote!(#ty).to_string()
        })
    }

    fn const_expr_string(value: &ConstExpr) -> Result<String, Error> {
        Ok(match value {
            ConstExpr::Raw(source) => source.clone(),
            ConstExpr::Path(path) => Self::path_string(path)?,
            ConstExpr::Literal(literal) => Self::literal_string(literal),
            ConstExpr::Array(values) => format!(
                "[{}]",
                values
                    .iter()
                    .map(Self::const_expr_string)
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ")
            ),
            ConstExpr::Tuple(values) => format!(
                "({})",
                values
                    .iter()
                    .map(Self::const_expr_string)
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ")
            ),
        })
    }

    fn literal_string(literal: &Literal) -> String {
        match literal {
            Literal::Bool(value) => value.to_string(),
            Literal::Integer(value) => value.source.clone(),
            Literal::Float(value) => value.source.clone(),
            Literal::String(value) => format!("{value:?}"),
            Literal::Bytes(value) => format!("{value:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{PackageInfo, Path, PathRoot, PathSegment, ReturnDef, TypeExpr};

    use super::TypeTokens;
    use crate::experimental::rust_api::crate_root::RootModuleTypes;

    #[test]
    fn renders_interned_string_with_typed_crate_rooted_pool_path() {
        let type_expr = TypeExpr::interned_string(
            Path::new(
                PathRoot::Relative,
                vec![
                    PathSegment::new("boltffi"),
                    PathSegment::new("InternedString"),
                ],
            ),
            "demo::pools::BrowserName",
            Path::new(
                PathRoot::Crate,
                vec![PathSegment::new("pools"), PathSegment::new("BrowserName")],
            ),
            vec!["Chrome".to_owned()],
        );

        let rendered = TypeTokens::tokens(&type_expr)
            .expect("typed pool path should render")
            .to_string();
        assert_eq!(
            rendered,
            ":: boltffi :: InternedString < crate :: pools :: BrowserName >"
        );
    }

    #[test]
    fn scan_root_and_render_preserves_nested_pool_path() {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                mod pools {
                    boltffi::interned_string_pool! {
                        pub BrowserName {
                            CHROME = "Chrome",
                        }
                    }
                }

                mod api {
                    use boltffi::InternedString;
                    use crate::pools::BrowserName;

                    #[export]
                    pub fn browser() -> InternedString<BrowserName> {
                        BrowserName::CHROME
                    }
                }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source should scan");
        let rooted = RootModuleTypes::new(&source.package).contract(&source);
        let ReturnDef::Value(return_type) = &rooted.functions[0].returns else {
            panic!("expected value return");
        };

        let rendered = TypeTokens::tokens(return_type)
            .expect("scanned interned string should render")
            .to_string();
        assert_eq!(
            rendered,
            ":: boltffi :: InternedString < crate :: pools :: BrowserName >"
        );
    }
}
