use std::num::NonZeroUsize;

use boltffi_ast::{
    ConstExpr, CustomRemoteGenericArgument, CustomRemotePath, CustomRemotePathSegment,
    CustomRemoteType, CustomTypeConverter, CustomTypeConverters, CustomTypeDef, CustomTypeId,
    NamePart, Path, PathRoot, PathSegment,
};
use syn::parse::Parse;

use crate::declared_types::DeclaredTypes;
use crate::type_expr::Scanner;
use crate::{ScanError, name};

pub(crate) fn scan_macro_tokens(
    tokens: proc_macro2::TokenStream,
    declared_types: &DeclaredTypes,
) -> Result<CustomTypeDef, ScanError> {
    let spec = syn::parse2::<ParsedSpec>(tokens)
        .map_err(invalid_custom_type)
        .and_then(Spec::from_parsed)?;
    build_from_spec(&spec, declared_types)
}

pub(crate) fn scan_trait_impl_item(
    item: &syn::ItemImpl,
    declared_types: &DeclaredTypes,
) -> Result<CustomTypeDef, ScanError> {
    let spec = Spec::parse_trait_impl(item)?;
    build_from_spec(&spec, declared_types)
}

fn build_from_spec(
    spec: &Spec,
    declared_types: &DeclaredTypes,
) -> Result<CustomTypeDef, ScanError> {
    Ok(CustomTypeDef::new(
        CustomTypeId::new(name::self_id(&spec.name)),
        name::source(&spec.name),
        spec.remote_type.clone(),
        Scanner::new(declared_types).scan(&spec.repr)?,
        spec.error.clone(),
        spec.converters(),
    ))
}

struct Spec {
    name: syn::Ident,
    remote_type: CustomRemoteType,
    repr: syn::Type,
    error: Option<CustomRemoteType>,
    converters: ConverterSource,
}

enum ConverterSource {
    Macro,
    Trait { receiver: Path },
}

impl Spec {
    fn parse_trait_impl(item: &syn::ItemImpl) -> Result<Self, ScanError> {
        let (trait_path, _) = item.trait_.as_ref().ok_or_else(|| {
            invalid_custom_type_message("custom_ffi marker requires a trait impl".to_owned())
        })?;
        if !is_custom_ffi_trait(trait_path) {
            return Err(invalid_custom_type_message(format!(
                "custom_ffi marker requires CustomFfiConvertible, found `{}`",
                quote::quote!(#trait_path).to_string().replace(' ', "")
            )));
        }
        let self_type = item.self_ty.as_ref();
        let name = impl_target_name(self_type)?;
        let receiver = impl_target_path(self_type)?;
        let remote_type = RemoteType::scan(self_type)?;
        let repr = associated_type(item, "FfiRepr")?;
        let error = Some(RemoteType::scan(&associated_type(item, "Error")?)?);
        Ok(Self {
            name,
            remote_type,
            repr,
            error,
            converters: ConverterSource::Trait { receiver },
        })
    }

    fn from_parsed(parsed: ParsedSpec) -> Result<Self, ScanError> {
        let remote_type = RemoteType::scan(&parsed.remote)?;
        Ok(Self {
            name: parsed.name,
            remote_type,
            repr: parsed.repr,
            error: parsed.error.map(|ty| RemoteType::scan(&ty)).transpose()?,
            converters: ConverterSource::Macro,
        })
    }

    fn converters(&self) -> CustomTypeConverters {
        match &self.converters {
            ConverterSource::Macro => CustomTypeConverters::new(
                CustomTypeConverter::path(self.macro_helper("into_ffi")),
                CustomTypeConverter::path(self.macro_helper("try_from_ffi")),
            ),
            ConverterSource::Trait { receiver } => CustomTypeConverters::new(
                trait_converter(receiver.clone(), "into_ffi"),
                trait_converter(receiver.clone(), "try_from_ffi"),
            ),
        }
    }

    fn macro_helper(&self, role: &str) -> Path {
        let helper = format!(
            "__boltffi_custom_type_{}_{}",
            name::canonical(&self.name),
            role
        );
        Path::new(
            PathRoot::Crate,
            vec![PathSegment::new("$self"), PathSegment::new(helper)],
        )
    }
}

fn is_custom_ffi_trait(path: &syn::Path) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == "CustomFfiConvertible")
}

fn impl_target_name(ty: &syn::Type) -> Result<syn::Ident, ScanError> {
    let syn::Type::Path(path) = ty else {
        return Err(invalid_custom_type_message(format!(
            "custom_ffi target is not a path `{}`",
            crate::spelling::ty(ty)
        )));
    };
    if path.qself.is_some() {
        return Err(invalid_custom_type_message(format!(
            "custom_ffi target cannot use qualified self type `{}`",
            crate::spelling::ty(ty)
        )));
    }
    path.path
        .segments
        .last()
        .map(|segment| segment.ident.clone())
        .ok_or_else(|| invalid_custom_type_message("custom_ffi target path is empty".to_owned()))
}

fn impl_target_path(ty: &syn::Type) -> Result<Path, ScanError> {
    let syn::Type::Path(path) = ty else {
        return Err(invalid_custom_type_message(format!(
            "custom_ffi target is not a path `{}`",
            crate::spelling::ty(ty)
        )));
    };
    if path.qself.is_some() {
        return Err(invalid_custom_type_message(format!(
            "custom_ffi target cannot use qualified self type `{}`",
            crate::spelling::ty(ty)
        )));
    }
    let (root, segments) = PathParts::split_root(&path.path)?;
    segments
        .into_iter()
        .map(|segment| match segment.arguments {
            syn::PathArguments::None => Ok(PathSegment::new(segment.ident.to_string())),
            _ => Err(invalid_custom_type_message(format!(
                "custom_ffi target cannot use generic arguments `{}`",
                crate::spelling::ty(ty)
            ))),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|segments| Path::new(root, segments))
}

fn associated_type(item: &syn::ItemImpl, name: &str) -> Result<syn::Type, ScanError> {
    item.items
        .iter()
        .find_map(|item| match item {
            syn::ImplItem::Type(ty) if ty.ident == name => Some(ty.ty.clone()),
            _ => None,
        })
        .ok_or_else(|| {
            invalid_custom_type_message(format!(
                "custom_ffi impl is missing associated type `{name}`"
            ))
        })
}

fn trait_converter(receiver: Path, method: &str) -> CustomTypeConverter {
    CustomTypeConverter::trait_method(receiver, NamePart::new(method))
}

impl Parse for ParsedSpec {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<syn::Visibility>()?;
        let name = input.parse()?;
        input.parse::<syn::Token![,]>()?;

        let mut fields = ParsedFields::default();
        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            input.parse::<syn::Token![=]>()?;
            fields.parse_field(key, input)?;

            if input.peek(syn::Token![,]) {
                input.parse::<syn::Token![,]>()?;
            }
        }

        fields.finish(name)
    }
}

struct ParsedSpec {
    name: syn::Ident,
    remote: syn::Type,
    repr: syn::Type,
    error: Option<syn::Type>,
}

#[derive(Default)]
struct ParsedFields {
    remote: Option<syn::Type>,
    repr: Option<syn::Type>,
    error: Option<syn::Type>,
    into_ffi: Option<syn::Expr>,
    try_from_ffi: Option<syn::Expr>,
}

impl ParsedFields {
    fn parse_field(
        &mut self,
        key: syn::Ident,
        input: syn::parse::ParseStream<'_>,
    ) -> syn::Result<()> {
        match key.to_string().as_str() {
            "remote" => Self::set(&mut self.remote, "remote", input.parse()?, input),
            "repr" => Self::set(&mut self.repr, "repr", input.parse()?, input),
            "error" => Self::set(&mut self.error, "error", input.parse()?, input),
            "into_ffi" => Self::set(&mut self.into_ffi, "into_ffi", input.parse()?, input),
            "try_from_ffi" => Self::set(
                &mut self.try_from_ffi,
                "try_from_ffi",
                input.parse()?,
                input,
            ),
            other => Err(syn::Error::new(
                key.span(),
                format!("unknown custom_type! key `{other}`"),
            )),
        }
    }

    fn finish(self, name: syn::Ident) -> syn::Result<ParsedSpec> {
        let span = name.span();
        self.into_ffi
            .ok_or_else(|| syn::Error::new(span, "missing `into_ffi = ...`"))?;
        self.try_from_ffi
            .ok_or_else(|| syn::Error::new(span, "missing `try_from_ffi = ...`"))?;
        Ok(ParsedSpec {
            name,
            remote: self
                .remote
                .ok_or_else(|| syn::Error::new(span, "missing `remote = ...`"))?,
            repr: self
                .repr
                .ok_or_else(|| syn::Error::new(span, "missing `repr = ...`"))?,
            error: self.error,
        })
    }

    fn set<T>(
        slot: &mut Option<T>,
        key: &str,
        value: T,
        input: syn::parse::ParseStream<'_>,
    ) -> syn::Result<()> {
        if slot.replace(value).is_some() {
            Err(input.error(format!("duplicate custom_type! key `{key}`")))
        } else {
            Ok(())
        }
    }
}

struct RemoteType;

impl RemoteType {
    fn scan(ty: &syn::Type) -> Result<CustomRemoteType, ScanError> {
        match ty {
            syn::Type::Group(group) => Self::scan(group.elem.as_ref()),
            syn::Type::Paren(paren) => Self::scan(paren.elem.as_ref()),
            syn::Type::Path(path) if path.qself.is_none() => {
                Ok(CustomRemoteType::Path(RemotePath::scan(&path.path)?))
            }
            syn::Type::Tuple(tuple) => tuple
                .elems
                .iter()
                .map(Self::scan)
                .collect::<Result<Vec<_>, _>>()
                .map(CustomRemoteType::Tuple),
            _ => Err(invalid_custom_type_message(format!(
                "unsupported custom remote type `{}`",
                crate::spelling::ty(ty)
            ))),
        }
    }
}

struct RemotePath;

impl RemotePath {
    fn scan(path: &syn::Path) -> Result<CustomRemotePath, ScanError> {
        let (root, segments) = PathParts::split_root(path)?;
        if segments.is_empty() {
            return Err(invalid_custom_type_message(format!(
                "empty custom path `{}`",
                quote::quote!(#path).to_string().replace(' ', "")
            )));
        }
        segments
            .into_iter()
            .map(Self::segment)
            .collect::<Result<Vec<_>, _>>()
            .map(|segments| CustomRemotePath::new(root, segments))
    }

    fn segment(segment: &syn::PathSegment) -> Result<CustomRemotePathSegment, ScanError> {
        match &segment.arguments {
            syn::PathArguments::None => Ok(CustomRemotePathSegment::new(segment.ident.to_string())),
            syn::PathArguments::AngleBracketed(arguments) => arguments
                .args
                .iter()
                .map(Self::generic_argument)
                .collect::<Result<Vec<_>, _>>()
                .map(|arguments| {
                    CustomRemotePathSegment::with_arguments(segment.ident.to_string(), arguments)
                }),
            arguments => Err(invalid_custom_type_message(format!(
                "unsupported custom remote path arguments `{}`",
                quote::quote!(#arguments).to_string().replace(' ', "")
            ))),
        }
    }

    fn generic_argument(
        argument: &syn::GenericArgument,
    ) -> Result<CustomRemoteGenericArgument, ScanError> {
        match argument {
            syn::GenericArgument::Type(ty) => RemoteType::scan(ty)
                .map(Box::new)
                .map(CustomRemoteGenericArgument::Type),
            syn::GenericArgument::Const(expr) => Ok(CustomRemoteGenericArgument::Const(
                ConstExpr::Raw(quote::quote!(#expr).to_string().replace(' ', "")),
            )),
            syn::GenericArgument::AssocType(assoc) => {
                RemoteType::scan(&assoc.ty).map(|ty| CustomRemoteGenericArgument::AssociatedType {
                    name: NamePart::new(assoc.ident.to_string()),
                    ty: Box::new(ty),
                })
            }
            other => Err(invalid_custom_type_message(format!(
                "unsupported custom remote generic argument `{}`",
                quote::quote!(#other).to_string().replace(' ', "")
            ))),
        }
    }
}

struct PathParts;

impl PathParts {
    fn split_root(path: &syn::Path) -> Result<(PathRoot, Vec<&syn::PathSegment>), ScanError> {
        let segments = path.segments.iter().collect::<Vec<_>>();
        let Some(first) = segments.first() else {
            return Err(invalid_custom_type_message("empty custom path"));
        };
        if path.leading_colon.is_some() {
            return Ok((PathRoot::Absolute, segments));
        }
        match first.ident.to_string().as_str() {
            "crate" => Ok((PathRoot::Crate, segments.into_iter().skip(1).collect())),
            "self" => Ok((PathRoot::Self_, segments.into_iter().skip(1).collect())),
            "super" => {
                let levels = segments
                    .iter()
                    .take_while(|segment| segment.ident == "super")
                    .count();
                Ok((
                    PathRoot::Super(
                        NonZeroUsize::new(levels)
                            .expect("super path has at least one parent segment"),
                    ),
                    segments.into_iter().skip(levels).collect(),
                ))
            }
            _ => Ok((PathRoot::Relative, segments)),
        }
    }
}

fn invalid_custom_type(error: syn::Error) -> ScanError {
    invalid_custom_type_message(error.to_string()).at_span(error.span())
}

fn invalid_custom_type_message(message: impl Into<String>) -> ScanError {
    ScanError::InvalidCustomType {
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use boltffi_ast::{CanonicalName, Primitive, Source, TypeExpr, Visibility};

    fn item(source: &str) -> syn::ItemMacro {
        syn::parse_str(source).expect("custom type macro")
    }

    fn scan(source: &str) -> Result<CustomTypeDef, ScanError> {
        scan_macro_tokens(item(source).mac.tokens, &DeclaredTypes::new())
    }

    fn rust_path(name: impl Into<NamePart>) -> CustomRemoteType {
        CustomRemoteType::single_path(name)
    }

    fn date_time_utc() -> CustomRemoteType {
        CustomRemoteType::path(CustomRemotePath::new(
            PathRoot::Relative,
            vec![CustomRemotePathSegment::with_arguments(
                "DateTime",
                vec![CustomRemoteGenericArgument::Type(Box::new(rust_path(
                    "Utc",
                )))],
            )],
        ))
    }

    fn converter(role: &str) -> CustomTypeConverter {
        CustomTypeConverter::path(Path::new(
            PathRoot::Crate,
            vec![
                PathSegment::new("$self"),
                PathSegment::new(format!("__boltffi_custom_type_utc_date_time_{role}")),
            ],
        ))
    }

    #[test]
    fn scans_complete_custom_type_macro() {
        let custom = scan(
            "custom_type!(pub UtcDateTime, remote = DateTime<Utc>, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis);",
        )
        .expect("scan");

        assert_eq!(custom.id, CustomTypeId::new("$self::UtcDateTime"));
        assert_eq!(
            custom.name.canonical(),
            &CanonicalName::new(vec!["utc".into(), "date".into(), "time".into()])
        );
        assert_eq!(custom.remote, date_time_utc());
        assert_eq!(custom.repr, TypeExpr::Primitive(Primitive::I64));
        assert_eq!(custom.error, None);
        assert_eq!(custom.converters.into_ffi, converter("into_ffi"));
        assert_eq!(custom.converters.try_from_ffi, converter("try_from_ffi"));
        assert_eq!(custom.source, Source::new(Visibility::Public, None));
    }

    #[test]
    fn scans_error_type() {
        let custom = scan(
            "custom_type!(UtcDateTime, remote = DateTime<Utc>, repr = i64, error = ConvertError, into_ffi = to_millis, try_from_ffi = from_millis);",
        )
        .expect("scan");

        assert_eq!(custom.error, Some(rust_path("ConvertError")));
    }

    #[test]
    fn rejects_missing_required_fields() {
        assert!(matches!(
            crate::error::unlocated(scan("custom_type!(UtcDateTime, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis);")),
            Err(ScanError::InvalidCustomType { message }) if message.contains("missing `remote = ...`")
        ));
        assert!(matches!(
            crate::error::unlocated(scan("custom_type!(UtcDateTime, remote = DateTime<Utc>, repr = i64, into_ffi = to_millis);")),
            Err(ScanError::InvalidCustomType { message }) if message.contains("missing `try_from_ffi = ...`")
        ));
    }

    #[test]
    fn rejects_unknown_and_duplicate_keys() {
        assert!(matches!(
            crate::error::unlocated(scan("custom_type!(UtcDateTime, remote = DateTime<Utc>, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis, typo = true);")),
            Err(ScanError::InvalidCustomType { message }) if message.contains("unknown custom_type! key `typo`")
        ));
        assert!(matches!(
            crate::error::unlocated(scan("custom_type!(UtcDateTime, remote = DateTime<Utc>, remote = Timestamp, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis);")),
            Err(ScanError::InvalidCustomType { message }) if message.contains("duplicate custom_type! key `remote`")
        ));
    }

    #[test]
    fn custom_type_macro_uses_generated_converter_helpers() {
        let custom = scan(
            "custom_type!(UtcDateTime, remote = DateTime<Utc>, repr = i64, into_ffi = |dt: &DateTime<Utc>| dt.timestamp_millis(), try_from_ffi = |millis: i64| { DateTime::from_timestamp_millis(millis).ok_or(ConvertError) });",
        )
        .expect("scan");

        assert_eq!(custom.converters.into_ffi, converter("into_ffi"));
        assert_eq!(custom.converters.try_from_ffi, converter("try_from_ffi"));
    }

    #[test]
    fn custom_type_macro_converter_helpers_ignore_source_qualifiers() {
        let custom = scan(
            "custom_type!(UtcDateTime, remote = DateTime<Utc>, repr = i64, into_ffi = crate::time::to_millis, try_from_ffi = super::from_millis);",
        )
        .expect("scan");

        assert_eq!(custom.converters.into_ffi, converter("into_ffi"));
        assert_eq!(custom.converters.try_from_ffi, converter("try_from_ffi"));
    }
}
