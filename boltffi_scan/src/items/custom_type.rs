use std::num::NonZeroUsize;

use boltffi_ast::{
    ConstExpr, CustomRemoteGenericArgument, CustomRemotePath, CustomRemotePathSegment,
    CustomRemoteType, CustomTypeConverter, CustomTypeConverters, CustomTypeDef, CustomTypeId,
    Literal, NamePart, Path, PathRoot, PathSegment,
};
use syn::parse::Parse;

use crate::attributes::Attributes;
use crate::declared_types::DeclaredTypes;
use crate::marked::{MarkedCustom, MarkedCustomItem};
use crate::path::{ImportLookup, ModulePath, ModuleScope};
use crate::type_expr::Scanner;
use crate::{ScanError, attributes, name};

pub fn scan(
    marked: &MarkedCustom<'_>,
    declared_types: &DeclaredTypes,
) -> Result<CustomTypeDef, ScanError> {
    let spec = Spec::parse(marked)?;
    let custom_name = name::source(spec.name());
    let custom_id = CustomTypeId::new(marked.module().qualified(&spec.name().to_string()));
    let converters = spec.converters(marked.module());
    let scanner = Scanner::new(declared_types, marked.scope());
    let attrs = Attributes::new(marked.attrs(), &scanner);
    let mut custom = CustomTypeDef::new(
        custom_id,
        custom_name,
        spec.remote_type().clone(),
        scanner.scan(spec.repr())?,
        spec.error().cloned(),
        converters,
    );
    custom.source = spec.source(marked);
    custom.source_span = custom.source.span.clone();
    custom.doc = attrs.doc();
    custom.deprecated = attrs.deprecated()?;
    custom.user_attrs = attrs.user_attrs();
    Ok(custom)
}

pub struct Spec {
    visibility: Option<syn::Visibility>,
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
    pub fn parse(marked: &MarkedCustom<'_>) -> Result<Self, ScanError> {
        match marked.item() {
            MarkedCustomItem::Macro(item) => Self::parse_macro(item),
            MarkedCustomItem::TraitImpl(item) => Self::parse_trait_impl(item),
        }
    }

    fn parse_macro(item: &syn::ItemMacro) -> Result<Self, ScanError> {
        syn::parse2::<ParsedSpec>(item.mac.tokens.clone())
            .map_err(invalid_custom_type)
            .and_then(Self::from_parsed)
    }

    fn parse_trait_impl(item: &syn::ItemImpl) -> Result<Self, ScanError> {
        let (_, trait_path, _) = item.trait_.as_ref().ok_or_else(|| {
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
            visibility: None,
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
            visibility: Some(parsed.visibility),
            name: parsed.name,
            remote_type,
            repr: parsed.repr,
            error: parsed.error.map(|ty| RemoteType::scan(&ty)).transpose()?,
            converters: ConverterSource::Macro,
        })
    }

    pub fn name(&self) -> &syn::Ident {
        &self.name
    }

    pub fn remote_type(&self) -> &CustomRemoteType {
        &self.remote_type
    }

    /// Whether the custom declaration proves a Rust type exists at its
    /// declared path.
    ///
    /// A `custom_ffi` trait impl targets a real type in its module, even one
    /// a macro like `new_key_type!` generated, so the path can join the
    /// source namespace and re-export chains can verify against it. A
    /// `custom_type!` macro only names an FFI-side alias.
    pub fn declares_source_type(&self) -> bool {
        matches!(self.converters, ConverterSource::Trait { .. })
    }

    fn repr(&self) -> &syn::Type {
        &self.repr
    }

    fn converters(&self, module: &ModulePath) -> CustomTypeConverters {
        match &self.converters {
            ConverterSource::Macro => CustomTypeConverters::new(
                CustomTypeConverter::path(self.macro_helper(module, "into_ffi")),
                CustomTypeConverter::path(self.macro_helper(module, "try_from_ffi")),
            ),
            ConverterSource::Trait { receiver } => CustomTypeConverters::new(
                trait_converter(receiver.clone(), "into_ffi"),
                trait_converter(receiver.clone(), "try_from_ffi"),
            ),
        }
    }

    fn error(&self) -> Option<&CustomRemoteType> {
        self.error.as_ref()
    }

    fn source(&self, marked: &MarkedCustom<'_>) -> boltffi_ast::Source {
        match &self.visibility {
            Some(visibility) => attributes::source(visibility, marked.scope(), marked.span()),
            None => attributes::public_source(marked.scope(), marked.span()),
        }
    }

    fn macro_helper(&self, module: &ModulePath, role: &str) -> Path {
        let helper = format!(
            "__boltffi_custom_type_{}_{}",
            name::symbol_segment(&self.name.to_string()),
            role
        );
        let segments = module
            .segments()
            .iter()
            .skip(1)
            .map(|segment| PathSegment::new(segment.as_str()))
            .chain(std::iter::once(PathSegment::new(helper)))
            .collect();
        Path::new(PathRoot::Crate, segments)
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
        let visibility = input.parse()?;
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

        fields.finish(visibility, name)
    }
}

struct ParsedSpec {
    visibility: syn::Visibility,
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
            other => Err(input.error(format!("unknown custom_type! key `{other}`"))),
        }
    }

    fn finish(self, visibility: syn::Visibility, name: syn::Ident) -> syn::Result<ParsedSpec> {
        let span = name.span();
        self.into_ffi
            .ok_or_else(|| syn::Error::new(span, "missing `into_ffi = ...`"))?;
        self.try_from_ffi
            .ok_or_else(|| syn::Error::new(span, "missing `try_from_ffi = ...`"))?;
        Ok(ParsedSpec {
            visibility,
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

pub struct RemoteType;

impl RemoteType {
    pub fn scan(ty: &syn::Type) -> Result<CustomRemoteType, ScanError> {
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

pub struct RemoteIdentity {
    exact: Vec<String>,
    shape: Option<String>,
    ambiguous: bool,
}

impl RemoteIdentity {
    pub fn registered(scope: &ModuleScope, remote: &CustomRemoteType) -> Self {
        let exact = Self::remote_exact(scope, remote);
        Self {
            exact: exact.keys,
            shape: Self::registration_shape_fallback(remote),
            ambiguous: exact.ambiguous,
        }
    }

    pub fn query(scope: &ModuleScope, remote: &CustomRemoteType) -> Self {
        let exact = Self::remote_exact(scope, remote);
        Self {
            exact: exact.keys,
            shape: Self::query_shape_fallback(scope, remote, exact.ambiguous),
            ambiguous: exact.ambiguous,
        }
    }

    pub fn exact(&self) -> &[String] {
        &self.exact
    }

    pub fn shape(&self) -> Option<&str> {
        self.shape.as_deref()
    }

    pub const fn ambiguous(&self) -> bool {
        self.ambiguous
    }

    fn remote_exact(scope: &ModuleScope, remote: &CustomRemoteType) -> ExactRemote {
        match remote {
            CustomRemoteType::Path(path) => Self::remote_path_exact(scope, path),
            CustomRemoteType::Tuple(elements) => Self::join_exact(
                elements
                    .iter()
                    .map(|element| Self::remote_exact(scope, element)),
                |elements| format!("({})", elements.join(",")),
            ),
        }
    }

    fn remote_shape(remote: &CustomRemoteType) -> String {
        match remote {
            CustomRemoteType::Path(path) => Self::remote_path_shape(path),
            CustomRemoteType::Tuple(elements) => {
                let elements = elements
                    .iter()
                    .map(Self::remote_shape)
                    .collect::<Vec<_>>()
                    .join(",");
                format!("({elements})")
            }
        }
    }

    fn registration_shape_fallback(remote: &CustomRemoteType) -> Option<String> {
        if Self::allows_registration_shape_fallback(remote) {
            Some(Self::remote_shape(remote))
        } else {
            None
        }
    }

    fn query_shape_fallback(
        scope: &ModuleScope,
        remote: &CustomRemoteType,
        ambiguous: bool,
    ) -> Option<String> {
        if !ambiguous
            && !Self::remote_uses_scope_import(scope, remote)
            && Self::allows_query_shape_fallback(remote)
        {
            Some(Self::remote_shape(remote))
        } else {
            None
        }
    }

    fn allows_registration_shape_fallback(remote: &CustomRemoteType) -> bool {
        match remote {
            CustomRemoteType::Path(path) => !matches!(
                path.root,
                PathRoot::Relative
                    if path.segments.len() == 1
                        && path.segments[0].arguments.is_empty()
            ),
            CustomRemoteType::Tuple(elements) => elements
                .iter()
                .any(Self::allows_registration_shape_fallback),
        }
    }

    fn allows_query_shape_fallback(remote: &CustomRemoteType) -> bool {
        match remote {
            CustomRemoteType::Path(path) => {
                matches!(path.root, PathRoot::Relative)
                    && path.segments.len() == 1
                    && path.segments[0]
                        .arguments
                        .iter()
                        .all(Self::allows_query_generic_argument_shape_fallback)
            }
            CustomRemoteType::Tuple(elements) => {
                elements.iter().all(Self::allows_query_shape_fallback)
            }
        }
    }

    fn remote_path_exact(scope: &ModuleScope, path: &CustomRemotePath) -> ExactRemote {
        match path.root {
            PathRoot::Relative if path.segments.len() == 1 => {
                let segment = &path.segments[0];
                Self::relative_first_segment_exact(scope, segment)
            }
            PathRoot::Relative => Self::relative_remote_path_exact(scope, path),
            PathRoot::Crate => Self::join_exact(
                path.segments
                    .iter()
                    .map(|segment| Self::remote_path_segment_exact(scope, segment)),
                |segments| {
                    scope
                        .path()
                        .segments()
                        .first()
                        .cloned()
                        .into_iter()
                        .chain(segments)
                        .collect::<Vec<_>>()
                        .join("::")
                },
            ),
            PathRoot::Self_ => Self::join_exact(
                path.segments
                    .iter()
                    .map(|segment| Self::remote_path_segment_exact(scope, segment)),
                |segments| {
                    scope
                        .path()
                        .segments()
                        .iter()
                        .cloned()
                        .chain(segments)
                        .collect::<Vec<_>>()
                        .join("::")
                },
            ),
            PathRoot::Super(levels) => Self::join_exact(
                path.segments
                    .iter()
                    .map(|segment| Self::remote_path_segment_exact(scope, segment)),
                |segments| {
                    scope
                        .path()
                        .segments()
                        .iter()
                        .take(
                            scope
                                .path()
                                .segments()
                                .len()
                                .saturating_sub(levels.get())
                                .max(1),
                        )
                        .cloned()
                        .chain(segments)
                        .collect::<Vec<_>>()
                        .join("::")
                },
            ),
            PathRoot::Absolute => Self::join_exact(
                path.segments
                    .iter()
                    .map(|segment| Self::remote_path_segment_exact(scope, segment)),
                |segments| format!("::{}", segments.join("::")),
            ),
        }
    }

    fn relative_remote_path_exact(scope: &ModuleScope, path: &CustomRemotePath) -> ExactRemote {
        let Some(first) = path.segments.first() else {
            return ExactRemote::none();
        };
        match scope.imported(first.name.as_str()) {
            ImportLookup::Unique(imported) => {
                let first = Self::prefix_path_exact(scope, imported.to_vec(), first);
                Self::join_exact(
                    std::iter::once(first).chain(
                        path.segments
                            .iter()
                            .skip(1)
                            .map(|segment| Self::remote_path_segment_exact(scope, segment)),
                    ),
                    |segments| segments.join("::"),
                )
            }
            ImportLookup::Ambiguous => ExactRemote::ambiguous(),
            ImportLookup::None => Self::join_exact(
                path.segments
                    .iter()
                    .map(|segment| Self::remote_path_segment_exact(scope, segment)),
                |segments| segments.join("::"),
            ),
        }
    }

    fn relative_first_segment_exact(
        scope: &ModuleScope,
        segment: &CustomRemotePathSegment,
    ) -> ExactRemote {
        match scope.imported(segment.name.as_str()) {
            ImportLookup::Unique(imported) => {
                Self::prefix_path_exact(scope, imported.to_vec(), segment)
            }
            ImportLookup::Ambiguous => ExactRemote::ambiguous(),
            ImportLookup::None => ExactRemote::merge(
                std::iter::once({
                    let mut prefix = scope.path().segments().to_vec();
                    prefix.push(segment.name.as_str().to_owned());
                    Self::prefix_path_exact(scope, prefix, segment)
                })
                .chain(
                    scope
                        .glob_candidates_for_segments(&[segment.name.as_str().to_owned()])
                        .into_iter()
                        .map(|candidate| {
                            Self::prefix_path_exact(
                                scope,
                                candidate.split("::").map(ToOwned::to_owned).collect(),
                                segment,
                            )
                        }),
                ),
            ),
        }
    }

    fn join_exact(
        mut elements: impl Iterator<Item = ExactRemote>,
        render: impl Fn(Vec<String>) -> String,
    ) -> ExactRemote {
        let resolved = elements.try_fold(
            (vec![Vec::<String>::new()], false),
            |(sets, ambiguous), element| {
                if element.ambiguous {
                    return Some((sets, true));
                }
                if element.keys.is_empty() {
                    return None;
                }
                let next = sets
                    .iter()
                    .flat_map(|set| {
                        element.keys.iter().map(|key| {
                            let mut set = set.clone();
                            set.push(key.clone());
                            set
                        })
                    })
                    .collect::<Vec<_>>();
                Some((next, ambiguous))
            },
        );
        match resolved {
            Some((sets, false)) => ExactRemote::keys(sets.into_iter().map(render).collect()),
            Some((_, true)) => ExactRemote::ambiguous(),
            None => ExactRemote::none(),
        }
    }

    fn prefix_path_exact(
        scope: &ModuleScope,
        prefix: Vec<String>,
        segment: &CustomRemotePathSegment,
    ) -> ExactRemote {
        if segment.arguments.is_empty() {
            return ExactRemote::key(prefix.join("::"));
        }
        let Some(last_index) = prefix.len().checked_sub(1) else {
            return ExactRemote::none();
        };
        Self::join_exact(
            segment
                .arguments
                .iter()
                .map(|argument| Self::remote_generic_argument_exact(scope, argument)),
            |arguments| {
                let mut prefix = prefix.clone();
                let last = prefix
                    .get_mut(last_index)
                    .expect("last index came from non-empty path");
                let base = last.clone();
                *last = format!("{base}<{}>", arguments.join(","));
                prefix.join("::")
            },
        )
    }

    fn remote_uses_scope_import(scope: &ModuleScope, remote: &CustomRemoteType) -> bool {
        match remote {
            CustomRemoteType::Path(path) => Self::remote_path_uses_scope_import(scope, path),
            CustomRemoteType::Tuple(elements) => elements
                .iter()
                .any(|element| Self::remote_uses_scope_import(scope, element)),
        }
    }

    fn remote_path_uses_scope_import(scope: &ModuleScope, path: &CustomRemotePath) -> bool {
        let arguments_use_import = path
            .segments
            .iter()
            .flat_map(|segment| segment.arguments.iter())
            .any(|argument| Self::generic_argument_uses_scope_import(scope, argument));
        match path.root {
            PathRoot::Relative => {
                arguments_use_import
                    || path.segments.first().is_some_and(|segment| {
                        !matches!(scope.imported(segment.name.as_str()), ImportLookup::None)
                            || scope.has_glob_imports()
                    })
            }
            PathRoot::Crate | PathRoot::Self_ | PathRoot::Super(_) | PathRoot::Absolute => {
                arguments_use_import
            }
        }
    }

    fn generic_argument_uses_scope_import(
        scope: &ModuleScope,
        argument: &CustomRemoteGenericArgument,
    ) -> bool {
        match argument {
            CustomRemoteGenericArgument::Type(remote) => {
                Self::remote_uses_scope_import(scope, remote)
            }
            CustomRemoteGenericArgument::Const(expr) => {
                Self::const_expr_uses_scope_import(scope, expr)
            }
            CustomRemoteGenericArgument::AssociatedType { ty, .. } => {
                Self::remote_uses_scope_import(scope, ty)
            }
        }
    }

    fn const_expr_uses_scope_import(scope: &ModuleScope, expr: &ConstExpr) -> bool {
        match expr {
            ConstExpr::Path(path) => Self::path_expr_uses_scope_import(scope, path),
            ConstExpr::Array(elements) | ConstExpr::Tuple(elements) => elements
                .iter()
                .any(|element| Self::const_expr_uses_scope_import(scope, element)),
            ConstExpr::Literal(_) | ConstExpr::Raw(_) => false,
        }
    }

    fn path_expr_uses_scope_import(scope: &ModuleScope, path: &Path) -> bool {
        matches!(path.root, PathRoot::Relative)
            && path.segments.first().is_some_and(|segment| {
                !matches!(scope.imported(segment.name.as_str()), ImportLookup::None)
                    || scope.has_glob_imports()
            })
    }

    fn remote_path_shape(path: &CustomRemotePath) -> String {
        path.last()
            .map(Self::remote_path_segment_shape)
            .unwrap_or_default()
    }

    fn remote_path_segment_exact(
        scope: &ModuleScope,
        segment: &CustomRemotePathSegment,
    ) -> ExactRemote {
        if segment.arguments.is_empty() {
            return ExactRemote::key(segment.name.as_str().to_owned());
        }
        Self::join_exact(
            segment
                .arguments
                .iter()
                .map(|argument| Self::remote_generic_argument_exact(scope, argument)),
            |arguments| format!("{}<{}>", segment.name.as_str(), arguments.join(",")),
        )
    }

    fn remote_path_segment_shape(segment: &CustomRemotePathSegment) -> String {
        if segment.arguments.is_empty() {
            return segment.name.as_str().to_owned();
        }
        let arguments = segment
            .arguments
            .iter()
            .filter_map(Self::remote_generic_argument_shape)
            .collect::<Vec<_>>()
            .join(",");
        if arguments.is_empty() {
            segment.name.as_str().to_owned()
        } else {
            format!("{}<{arguments}>", segment.name.as_str())
        }
    }

    fn remote_generic_argument_exact(
        scope: &ModuleScope,
        argument: &CustomRemoteGenericArgument,
    ) -> ExactRemote {
        match argument {
            CustomRemoteGenericArgument::Type(remote) => Self::remote_exact(scope, remote),
            CustomRemoteGenericArgument::Const(expr) => ExactRemote::key(Self::const_expr(expr)),
            CustomRemoteGenericArgument::AssociatedType { name, ty } => {
                Self::remote_exact(scope, ty).map(|ty| format!("{}={}", name.as_str(), ty))
            }
        }
    }

    fn remote_generic_argument_shape(argument: &CustomRemoteGenericArgument) -> Option<String> {
        match argument {
            CustomRemoteGenericArgument::Type(remote) => Some(Self::remote_shape(remote)),
            CustomRemoteGenericArgument::Const(expr) => Some(Self::const_expr_shape(expr)),
            CustomRemoteGenericArgument::AssociatedType { name, ty } => {
                Some(format!("{}={}", name.as_str(), Self::remote_shape(ty)))
            }
        }
    }

    fn allows_query_generic_argument_shape_fallback(
        argument: &CustomRemoteGenericArgument,
    ) -> bool {
        match argument {
            CustomRemoteGenericArgument::Type(remote) => Self::allows_query_shape_fallback(remote),
            CustomRemoteGenericArgument::Const(expr) => {
                Self::allows_query_const_expr_shape_fallback(expr)
            }
            CustomRemoteGenericArgument::AssociatedType { ty, .. } => {
                Self::allows_query_shape_fallback(ty)
            }
        }
    }

    fn const_expr(expr: &ConstExpr) -> String {
        match expr {
            ConstExpr::Literal(literal) => Self::literal(literal),
            ConstExpr::Path(path) => Self::path_expr(path),
            ConstExpr::Array(elements) => {
                let elements = elements
                    .iter()
                    .map(Self::const_expr)
                    .collect::<Vec<_>>()
                    .join(",");
                format!("[{elements}]")
            }
            ConstExpr::Tuple(elements) => {
                let elements = elements
                    .iter()
                    .map(Self::const_expr)
                    .collect::<Vec<_>>()
                    .join(",");
                format!("({elements})")
            }
            ConstExpr::Raw(source) => source.clone(),
        }
    }

    fn const_expr_shape(expr: &ConstExpr) -> String {
        match expr {
            ConstExpr::Literal(literal) => Self::literal(literal),
            ConstExpr::Path(path) => Self::path_expr_shape(path),
            ConstExpr::Array(elements) => {
                let elements = elements
                    .iter()
                    .map(Self::const_expr_shape)
                    .collect::<Vec<_>>()
                    .join(",");
                format!("[{elements}]")
            }
            ConstExpr::Tuple(elements) => {
                let elements = elements
                    .iter()
                    .map(Self::const_expr_shape)
                    .collect::<Vec<_>>()
                    .join(",");
                format!("({elements})")
            }
            ConstExpr::Raw(source) => source.clone(),
        }
    }

    fn allows_query_const_expr_shape_fallback(expr: &ConstExpr) -> bool {
        match expr {
            ConstExpr::Literal(_) | ConstExpr::Raw(_) => true,
            ConstExpr::Path(path) => {
                matches!(path.root, PathRoot::Relative)
                    && path.segments.len() == 1
                    && path.segments[0].arguments.is_empty()
            }
            ConstExpr::Array(elements) | ConstExpr::Tuple(elements) => elements
                .iter()
                .all(Self::allows_query_const_expr_shape_fallback),
        }
    }

    fn literal(literal: &Literal) -> String {
        match literal {
            Literal::Bool(value) => value.to_string(),
            Literal::Integer(value) => value.source.clone(),
            Literal::Float(value) => value.source.clone(),
            Literal::String(value) => format!("{value:?}"),
            Literal::Bytes(value) => format!("{value:?}"),
        }
    }

    fn path_expr(path: &Path) -> String {
        let root = match path.root {
            PathRoot::Relative => String::new(),
            PathRoot::Crate => "crate::".to_owned(),
            PathRoot::Self_ => "self::".to_owned(),
            PathRoot::Super(levels) => "super::".repeat(levels.get()),
            PathRoot::Absolute => "::".to_owned(),
        };
        let segments = path
            .segments
            .iter()
            .map(Self::path_segment_expr)
            .collect::<Vec<_>>()
            .join("::");
        format!("{root}{segments}")
    }

    fn path_segment_expr(segment: &PathSegment) -> String {
        if segment.arguments.is_empty() {
            return segment.name.as_str().to_owned();
        }
        let arguments = segment
            .arguments
            .iter()
            .map(|argument| format!("{argument:?}"))
            .collect::<Vec<_>>()
            .join(",");
        format!("{}<{arguments}>", segment.name.as_str())
    }

    fn path_expr_shape(path: &Path) -> String {
        path.last()
            .map(Self::path_segment_expr_shape)
            .unwrap_or_default()
    }

    fn path_segment_expr_shape(segment: &PathSegment) -> String {
        if segment.arguments.is_empty() {
            return segment.name.as_str().to_owned();
        }
        let arguments = segment
            .arguments
            .iter()
            .map(|argument| format!("{argument:?}"))
            .collect::<Vec<_>>()
            .join(",");
        format!("{}<{arguments}>", segment.name.as_str())
    }
}

struct ExactRemote {
    keys: Vec<String>,
    ambiguous: bool,
}

impl ExactRemote {
    fn key(key: impl Into<String>) -> Self {
        Self {
            keys: vec![key.into()],
            ambiguous: false,
        }
    }

    fn keys(mut keys: Vec<String>) -> Self {
        keys.sort();
        keys.dedup();
        Self {
            keys,
            ambiguous: false,
        }
    }

    const fn none() -> Self {
        Self {
            keys: Vec::new(),
            ambiguous: false,
        }
    }

    const fn ambiguous() -> Self {
        Self {
            keys: Vec::new(),
            ambiguous: true,
        }
    }

    fn merge(elements: impl Iterator<Item = Self>) -> Self {
        let elements = elements.collect::<Vec<_>>();
        if elements.iter().any(|element| element.ambiguous) {
            return Self::ambiguous();
        }
        Self::keys(
            elements
                .into_iter()
                .flat_map(|element| element.keys)
                .collect(),
        )
    }

    fn map(self, transform: impl Fn(String) -> String) -> Self {
        Self {
            keys: self.keys.into_iter().map(transform).collect(),
            ambiguous: self.ambiguous,
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
    invalid_custom_type_message(error.to_string())
}

fn invalid_custom_type_message(message: impl Into<String>) -> ScanError {
    ScanError::InvalidCustomType {
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModuleScope;
    use boltffi_ast::{CanonicalName, Primitive, Source, TypeExpr, Visibility};

    fn item(source: &str) -> syn::ItemMacro {
        syn::parse_str(source).expect("custom type macro")
    }

    fn scan(source: &str) -> Result<CustomTypeDef, ScanError> {
        let module = ModuleScope::root("demo");
        let item = item(source);
        let marked = MarkedCustom::new(&module, &item);
        let declared_types = DeclaredTypes::new();
        super::scan(&marked, &declared_types)
    }

    fn rust_path(name: impl Into<NamePart>) -> CustomRemoteType {
        CustomRemoteType::single_path(name)
    }

    fn remote(source: &str) -> CustomRemoteType {
        RemoteType::scan(&syn::parse_str(source).expect("remote type")).expect("remote")
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
            vec![PathSegment::new(format!(
                "__boltffi_custom_type_utc_date_time_{role}"
            ))],
        ))
    }

    #[test]
    fn scans_complete_custom_type_macro() {
        let custom = scan(
            "custom_type!(pub UtcDateTime, remote = DateTime<Utc>, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis);",
        )
        .expect("scan");

        assert_eq!(custom.id, CustomTypeId::new("demo::UtcDateTime"));
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
            scan("custom_type!(UtcDateTime, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis);"),
            Err(ScanError::InvalidCustomType { message }) if message.contains("missing `remote = ...`")
        ));
        assert!(matches!(
            scan("custom_type!(UtcDateTime, remote = DateTime<Utc>, repr = i64, into_ffi = to_millis);"),
            Err(ScanError::InvalidCustomType { message }) if message.contains("missing `try_from_ffi = ...`")
        ));
    }

    #[test]
    fn rejects_unknown_and_duplicate_keys() {
        assert!(matches!(
            scan("custom_type!(UtcDateTime, remote = DateTime<Utc>, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis, typo = true);"),
            Err(ScanError::InvalidCustomType { message }) if message.contains("unknown custom_type! key `typo`")
        ));
        assert!(matches!(
            scan("custom_type!(UtcDateTime, remote = DateTime<Utc>, remote = Timestamp, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis);"),
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

    #[test]
    fn remote_identity_shape_preserves_const_arguments() {
        let module = ModuleScope::root("demo");
        let registered = RemoteIdentity::registered(&module, &remote("fixed::Array<4>"));
        let matching_query = RemoteIdentity::query(&module, &remote("Array<4>"));
        let non_matching_query = RemoteIdentity::query(&module, &remote("Array<8>"));

        assert_eq!(registered.shape(), Some("Array<4>"));
        assert_eq!(matching_query.shape(), Some("Array<4>"));
        assert_eq!(non_matching_query.shape(), Some("Array<8>"));
    }
}
