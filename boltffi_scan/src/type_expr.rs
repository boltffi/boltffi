use std::num::NonZeroUsize;

use boltffi_ast::{
    AdditionalBound, BaseTrait, BuiltinType, ConstExpr, CustomTypeId, FnSig, FnTrait, FnTraitKind,
    GenericArgument, MapKind, NamePart, Path, PathRoot, PathSegment, Primitive, ReturnDef,
    TraitBounds, TypeExpr,
};
use quote::ToTokens;

use crate::declared_types::{DeclaredType, DeclaredTypes, SourceType};
use crate::unsupported::UnsupportedFeature;
use crate::{ModuleScope, ScanError};

pub struct Scanner<'a> {
    declared_types: &'a DeclaredTypes,
    scope: &'a ModuleScope,
}

impl<'a> Scanner<'a> {
    pub fn new(declared_types: &'a DeclaredTypes, scope: &'a ModuleScope) -> Self {
        Self {
            declared_types,
            scope,
        }
    }

    pub fn scope(&self) -> &'a ModuleScope {
        self.scope
    }

    pub fn scan(&self, ty: &syn::Type) -> Result<TypeExpr, ScanError> {
        let unwrapped = unwrapped(ty);
        if let Some(custom) = self.custom_remote(unwrapped)? {
            return Ok(TypeExpr::custom(
                custom.clone(),
                self.path_for_custom_remote(unwrapped, custom.as_str()),
            ));
        }
        match unwrapped {
            syn::Type::BareFn(bare_fn) => self.bare_fn(bare_fn),
            syn::Type::ImplTrait(impl_trait) => self.impl_trait(impl_trait, ty),
            syn::Type::Slice(slice) => self.slice(slice),
            syn::Type::Path(type_path) => self.path(type_path, ty),
            syn::Type::TraitObject(trait_object) => self.dyn_trait(trait_object, ty),
            syn::Type::Tuple(tuple) => self.tuple(tuple),
            _ => Err(ScanError::unsupported_type(ty)),
        }
    }

    pub fn scan_return(&self, output: &syn::ReturnType) -> Result<ReturnDef, ScanError> {
        match output {
            syn::ReturnType::Default => Ok(ReturnDef::Void),
            syn::ReturnType::Type(_, ty) if is_unit(ty) => Ok(ReturnDef::Void),
            syn::ReturnType::Type(_, ty) => Ok(ReturnDef::Value(self.scan(ty)?)),
        }
    }

    pub fn scan_export_return(&self, output: &syn::ReturnType) -> Result<ReturnDef, ScanError> {
        match output {
            syn::ReturnType::Default => Ok(ReturnDef::Void),
            syn::ReturnType::Type(_, ty) if is_unit(ty) => Ok(ReturnDef::Void),
            syn::ReturnType::Type(_, ty) => Ok(ReturnDef::Value(self.scan_export_return_type(ty)?)),
        }
    }

    fn scan_export_return_type(&self, ty: &syn::Type) -> Result<TypeExpr, ScanError> {
        match unwrapped(ty) {
            syn::Type::Reference(reference) if reference.mutability.is_none() => {
                self.borrowed_export_return(reference, ty)
            }
            _ => self.scan(ty),
        }
    }

    fn borrowed_export_return(
        &self,
        reference: &syn::TypeReference,
        source: &syn::Type,
    ) -> Result<TypeExpr, ScanError> {
        match self.scan(&reference.elem)? {
            TypeExpr::Str => Ok(TypeExpr::Str),
            TypeExpr::Slice(element)
                if matches!(element.as_ref(), TypeExpr::Primitive(Primitive::U8)) =>
            {
                Ok(TypeExpr::Slice(element))
            }
            _ => Err(ScanError::unsupported_type(source)),
        }
    }

    fn path(&self, type_path: &syn::TypePath, source: &syn::Type) -> Result<TypeExpr, ScanError> {
        if type_path.qself.is_some() {
            return Err(ScanError::unsupported_type(source));
        }
        if let Some(standard_type) = self.standard_type(type_path, source)? {
            return self.standard_path(standard_type, type_path, source);
        }
        self.named(type_path, source)
    }

    fn standard_path(
        &self,
        standard_type: StandardType,
        type_path: &syn::TypePath,
        source: &syn::Type,
    ) -> Result<TypeExpr, ScanError> {
        let segment = type_path
            .path
            .segments
            .last()
            .ok_or_else(|| ScanError::unsupported_type(source))?;
        match standard_type {
            StandardType::String => Ok(TypeExpr::String),
            StandardType::InternedString => self.interned_string(type_path, segment, source),
            StandardType::Vec => self
                .single_type_argument(segment, source)
                .and_then(|argument| self.scan(argument))
                .map(TypeExpr::vec),
            StandardType::Option => self
                .single_type_argument(segment, source)
                .and_then(|argument| self.scan(argument))
                .map(TypeExpr::option),
            StandardType::Result => {
                let (ok, err) = self.two_type_arguments(segment, source)?;
                Ok(TypeExpr::result(self.scan(ok)?, self.scan(err)?))
            }
            StandardType::HashMap => {
                let (key, value) = self.two_type_arguments(segment, source)?;
                Ok(TypeExpr::map(
                    MapKind::Hash,
                    self.scan(key)?,
                    self.scan(value)?,
                ))
            }
            StandardType::BTreeMap => {
                let (key, value) = self.two_type_arguments(segment, source)?;
                Ok(TypeExpr::map(
                    MapKind::BTree,
                    self.scan(key)?,
                    self.scan(value)?,
                ))
            }
            StandardType::Box => self
                .single_type_argument(segment, source)
                .and_then(|argument| self.scan(argument))
                .map(TypeExpr::boxed),
            StandardType::Arc => self
                .single_type_argument(segment, source)
                .and_then(|argument| self.scan(argument))
                .map(TypeExpr::arc),
            StandardType::Builtin(kind) => Ok(TypeExpr::builtin(kind)),
        }
    }

    fn standard_type(
        &self,
        type_path: &syn::TypePath,
        source: &syn::Type,
    ) -> Result<Option<StandardType>, ScanError> {
        let Some(segment) = type_path.path.segments.last() else {
            return Ok(None);
        };
        let Some(standard_type) = StandardType::from_leaf(&segment.ident.to_string()) else {
            return Ok(None);
        };
        match self
            .declared_types
            .resolve_type_in_scope(self.scope, &type_path.path)?
        {
            SourceType::Declared(_) if matches!(standard_type, StandardType::Builtin(_)) => {
                Ok(None)
            }
            SourceType::Declared(DeclaredType::Record(_))
            | SourceType::Declared(DeclaredType::Enum(_))
            | SourceType::Declared(DeclaredType::Trait(_))
            | SourceType::Declared(DeclaredType::Class(_))
            | SourceType::Declared(DeclaredType::InternedStringPool(_))
            | SourceType::Unregistered => Err(ScanError::unsupported_type(source)),
            SourceType::Declared(DeclaredType::Custom(_)) => Ok(None),
            SourceType::External(path) => {
                Ok(standard_type.accepts_path(&path).then_some(standard_type))
            }
            SourceType::Unknown => Ok(standard_type
                .accepts_path(&path_without_arguments(&type_path.path))
                .then_some(standard_type)),
        }
    }

    fn named(&self, type_path: &syn::TypePath, source: &syn::Type) -> Result<TypeExpr, ScanError> {
        if type_path
            .path
            .segments
            .iter()
            .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
        {
            return Err(ScanError::unsupported_type(source));
        }
        let Some(segment) = type_path.path.segments.last() else {
            return Err(ScanError::unsupported_type(source));
        };
        let name = segment.ident.to_string();
        if name == "Self" {
            return Ok(TypeExpr::SelfType);
        }
        if name == "str" {
            return Ok(TypeExpr::Str);
        }
        if let Some(primitive) = Primitive::from_rust_name(&name) {
            return Ok(TypeExpr::Primitive(primitive));
        }
        let path = ast_path(&type_path.path, self)?;
        match self
            .declared_types
            .resolve_type_in_scope(self.scope, &type_path.path)?
        {
            SourceType::Declared(DeclaredType::Record(id)) => {
                Ok(TypeExpr::record(id.clone(), path))
            }
            SourceType::Declared(DeclaredType::Enum(id)) => {
                Ok(TypeExpr::enumeration(id.clone(), path))
            }
            SourceType::Declared(DeclaredType::Class(id)) => Ok(TypeExpr::class(id.clone(), path)),
            SourceType::Declared(DeclaredType::Custom(id)) => {
                Ok(TypeExpr::custom(id.clone(), path))
            }
            SourceType::Declared(DeclaredType::Trait(_))
            | SourceType::Declared(DeclaredType::InternedStringPool(_))
            | SourceType::Unregistered
            | SourceType::External(_)
            | SourceType::Unknown => Err(ScanError::unsupported_type(source)),
        }
    }

    fn dyn_trait(
        &self,
        trait_object: &syn::TypeTraitObject,
        source: &syn::Type,
    ) -> Result<TypeExpr, ScanError> {
        self.trait_bounds(&trait_object.bounds, source)
            .map(TypeExpr::Dyn)
    }

    fn impl_trait(
        &self,
        impl_trait: &syn::TypeImplTrait,
        source: &syn::Type,
    ) -> Result<TypeExpr, ScanError> {
        self.trait_bounds(&impl_trait.bounds, source)
            .map(TypeExpr::ImplTrait)
    }

    fn trait_bounds(
        &self,
        bounds: &syn::punctuated::Punctuated<syn::TypeParamBound, syn::Token![+]>,
        source: &syn::Type,
    ) -> Result<TraitBounds, ScanError> {
        let parts = bounds
            .iter()
            .map(|bound| self.trait_bound_part(bound, source))
            .collect::<Result<Vec<_>, _>>()?;
        let (base, bounds) = parts.into_iter().try_fold(
            (None, Vec::new()),
            |(base, mut bounds), part| match part {
                TraitBoundPart::Base(next) if base.is_none() => Ok((Some(next), bounds)),
                TraitBoundPart::Base(_) => Err(ScanError::unsupported_type(source)),
                TraitBoundPart::Additional(bound) => {
                    bounds.push(bound);
                    Ok((base, bounds))
                }
            },
        )?;
        base.map(|base| TraitBounds::new(base, bounds))
            .ok_or_else(|| ScanError::unsupported_type(source))
    }

    fn trait_bound_part(
        &self,
        bound: &syn::TypeParamBound,
        source: &syn::Type,
    ) -> Result<TraitBoundPart, ScanError> {
        match bound {
            syn::TypeParamBound::Trait(bound) => self.trait_path_bound(bound, source),
            syn::TypeParamBound::Lifetime(lifetime) => Ok(TraitBoundPart::Additional(
                AdditionalBound::Lifetime(lifetime.to_token_stream().to_string()),
            )),
            _ => Err(ScanError::unsupported_type(source)),
        }
    }

    fn trait_path_bound(
        &self,
        bound: &syn::TraitBound,
        source: &syn::Type,
    ) -> Result<TraitBoundPart, ScanError> {
        if let Some(base) = self.base_trait(bound)? {
            return Ok(TraitBoundPart::Base(base));
        }
        if let Some(bound) = self.additional_bound(bound)? {
            return Ok(TraitBoundPart::Additional(bound));
        }
        Err(ScanError::unsupported_type(source))
    }

    fn additional_bound(
        &self,
        bound: &syn::TraitBound,
    ) -> Result<Option<AdditionalBound>, ScanError> {
        if !matches!(bound.modifier, syn::TraitBoundModifier::None)
            || bound.lifetimes.is_some()
            || closure_bound(bound).is_some()
            || bound
                .path
                .segments
                .iter()
                .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
        {
            return Ok(None);
        }
        match self
            .declared_types
            .resolve_type_in_scope(self.scope, &bound.path)?
        {
            SourceType::Declared(_) | SourceType::Unregistered => Ok(None),
            SourceType::External(_) | SourceType::Unknown => Ok(Some(AdditionalBound::AutoTrait(
                ast_path(&bound.path, self)?,
            ))),
        }
    }

    fn base_trait(&self, bound: &syn::TraitBound) -> Result<Option<BaseTrait>, ScanError> {
        if !matches!(bound.modifier, syn::TraitBoundModifier::None) || bound.lifetimes.is_some() {
            return Ok(None);
        }
        if let Some((kind, arguments)) = closure_bound(bound) {
            return self
                .fn_trait(kind, arguments)
                .map(Box::new)
                .map(BaseTrait::Function)
                .map(Some);
        }
        if bound
            .path
            .segments
            .iter()
            .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
        {
            return Ok(None);
        }
        let path = ast_path(&bound.path, self)?;
        match self
            .declared_types
            .resolve_type_in_scope(self.scope, &bound.path)?
        {
            SourceType::Declared(DeclaredType::Trait(id)) => Ok(Some(BaseTrait::Named {
                id: id.clone(),
                path,
            })),
            SourceType::Declared(_)
            | SourceType::Unregistered
            | SourceType::External(_)
            | SourceType::Unknown => Ok(None),
        }
    }

    fn bare_fn(&self, bare_fn: &syn::TypeBareFn) -> Result<TypeExpr, ScanError> {
        if bare_fn.lifetimes.is_some() {
            return Err(crate::unsupported::feature(
                UnsupportedFeature::HigherRankedFunctionPointer,
            ));
        }
        if bare_fn.unsafety.is_some() {
            return Err(crate::unsupported::feature(
                UnsupportedFeature::UnsafeFunctionPointer,
            ));
        }
        if bare_fn.abi.is_some() {
            return Err(crate::unsupported::feature(
                UnsupportedFeature::ExternFunctionPointer,
            ));
        }
        if bare_fn.variadic.is_some() {
            return Err(crate::unsupported::feature(
                UnsupportedFeature::VariadicFunctionPointer,
            ));
        }
        let parameters = bare_fn
            .inputs
            .iter()
            .map(|argument| self.scan(&argument.ty))
            .collect::<Result<Vec<_>, _>>()?;
        let returns = self.scan_return(&bare_fn.output)?;
        Ok(TypeExpr::fn_ptr(FnSig::new(parameters, returns)))
    }

    fn fn_trait(
        &self,
        kind: FnTraitKind,
        arguments: &syn::ParenthesizedGenericArguments,
    ) -> Result<FnTrait, ScanError> {
        let parameters = arguments
            .inputs
            .iter()
            .map(|input| self.scan(input))
            .collect::<Result<Vec<_>, _>>()?;
        let returns = self.scan_return(&arguments.output)?;
        Ok(FnTrait::new(kind, FnSig::new(parameters, returns)))
    }

    fn slice(&self, slice: &syn::TypeSlice) -> Result<TypeExpr, ScanError> {
        self.scan(&slice.elem).map(TypeExpr::slice)
    }

    fn tuple(&self, tuple: &syn::TypeTuple) -> Result<TypeExpr, ScanError> {
        match tuple.elems.len() {
            0 => Ok(TypeExpr::Unit),
            _ => tuple
                .elems
                .iter()
                .map(|element| self.scan(element))
                .collect::<Result<Vec<_>, _>>()
                .map(TypeExpr::tuple),
        }
    }

    fn interned_string(
        &self,
        type_path: &syn::TypePath,
        segment: &syn::PathSegment,
        source: &syn::Type,
    ) -> Result<TypeExpr, ScanError> {
        let pool = self.single_type_argument(segment, source)?;
        let syn::Type::Path(pool_path) = unwrapped(pool) else {
            return Err(ScanError::unsupported_type(source));
        };
        if pool_path.qself.is_some()
            || pool_path
                .path
                .segments
                .iter()
                .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
        {
            return Err(ScanError::unsupported_type(source));
        }
        let (pool_canonical_path, static_values) = self
            .declared_types
            .resolve_interned_string_pool_entry(self.scope, &pool_path.path)?
            .ok_or_else(|| ScanError::unsupported_type(source))?;
        Ok(TypeExpr::interned_string(
            interned_string_base_path(&type_path.path),
            pool_canonical_path,
            ast_path_without_arguments(&pool_path.path),
            static_values.to_vec(),
        ))
    }

    fn single_type_argument<'segment>(
        &self,
        segment: &'segment syn::PathSegment,
        source: &syn::Type,
    ) -> Result<&'segment syn::Type, ScanError> {
        match type_arguments(segment).as_slice() {
            [argument] => Ok(argument),
            _ => Err(ScanError::unsupported_type(source)),
        }
    }

    fn two_type_arguments<'segment>(
        &self,
        segment: &'segment syn::PathSegment,
        source: &syn::Type,
    ) -> Result<(&'segment syn::Type, &'segment syn::Type), ScanError> {
        match type_arguments(segment).as_slice() {
            [first, second] => Ok((first, second)),
            _ => Err(ScanError::unsupported_type(source)),
        }
    }

    fn path_for_custom_remote(&self, ty: &syn::Type, fallback: &str) -> Path {
        match unwrapped(ty) {
            syn::Type::Path(type_path) => ast_path(&type_path.path, self)
                .unwrap_or_else(|_| ast_path_without_arguments(&type_path.path)),
            _ => Path::single(fallback),
        }
    }

    fn can_resolve_custom_remote(&self, ty: &syn::Type) -> Result<bool, ScanError> {
        let syn::Type::Path(type_path) = ty else {
            return Ok(true);
        };
        match self
            .declared_types
            .resolve_type_in_scope(self.scope, &type_path.path)?
        {
            SourceType::Declared(_) => Ok(false),
            SourceType::Unregistered => self
                .declared_types
                .resolves_type_alias(self.scope, &type_path.path),
            SourceType::External(_) | SourceType::Unknown => Ok(true),
        }
    }

    fn custom_remote(&self, ty: &syn::Type) -> Result<Option<&CustomTypeId>, ScanError> {
        if self.can_resolve_custom_remote(ty)? {
            self.declared_types.resolve_custom_remote(self.scope, ty)
        } else {
            Ok(None)
        }
    }
}

pub fn unwrapped(ty: &syn::Type) -> &syn::Type {
    match ty {
        syn::Type::Paren(paren) => unwrapped(&paren.elem),
        syn::Type::Group(group) => unwrapped(&group.elem),
        _ => ty,
    }
}

fn is_unit(ty: &syn::Type) -> bool {
    matches!(unwrapped(ty), syn::Type::Tuple(tuple) if tuple.elems.is_empty())
}

fn ast_path(path: &syn::Path, scanner: &Scanner<'_>) -> Result<Path, ScanError> {
    Ok(Path::new(
        path_root(path),
        path_segments(path, scanner)?.collect(),
    ))
}

fn ast_path_without_arguments(path: &syn::Path) -> Path {
    Path::new(
        path_root(path),
        path_segments_without_arguments(path).collect(),
    )
}

/// Returns the bare path to `InternedString` without pool generic arguments.
fn interned_string_base_path(type_path: &syn::Path) -> Path {
    Path::new(
        path_root(type_path),
        path_segments_without_arguments(type_path).collect(),
    )
}

fn path_root(path: &syn::Path) -> PathRoot {
    if path.leading_colon.is_some() {
        return PathRoot::Absolute;
    }
    let leading_supers = path
        .segments
        .iter()
        .take_while(|segment| segment.ident == "super")
        .count();
    match path
        .segments
        .first()
        .map(|segment| segment.ident.to_string())
    {
        Some(first) if first == "crate" => PathRoot::Crate,
        Some(first) if first == "self" => PathRoot::Self_,
        Some(_) | None if leading_supers == 0 => PathRoot::Relative,
        _ => NonZeroUsize::new(leading_supers)
            .map(PathRoot::Super)
            .unwrap_or(PathRoot::Relative),
    }
}

fn path_segments<'path>(
    path: &'path syn::Path,
    scanner: &'path Scanner<'_>,
) -> Result<impl Iterator<Item = PathSegment> + 'path, ScanError> {
    let skipped_root_segments = match path_root(path) {
        PathRoot::Relative | PathRoot::Absolute => 0,
        PathRoot::Crate | PathRoot::Self_ => 1,
        PathRoot::Super(count) => count.get(),
    };
    path.segments
        .iter()
        .skip(skipped_root_segments)
        .map(|segment| ast_segment(segment, scanner))
        .collect::<Result<Vec<_>, _>>()
        .map(IntoIterator::into_iter)
}

fn path_segments_without_arguments(path: &syn::Path) -> impl Iterator<Item = PathSegment> + '_ {
    let skipped_root_segments = match path_root(path) {
        PathRoot::Relative | PathRoot::Absolute => 0,
        PathRoot::Crate | PathRoot::Self_ => 1,
        PathRoot::Super(count) => count.get(),
    };
    path.segments
        .iter()
        .skip(skipped_root_segments)
        .map(|segment| PathSegment::new(NamePart::new(segment.ident.to_string())))
}

fn ast_segment(
    segment: &syn::PathSegment,
    scanner: &Scanner<'_>,
) -> Result<PathSegment, ScanError> {
    let arguments = match &segment.arguments {
        syn::PathArguments::None | syn::PathArguments::Parenthesized(_) => Vec::new(),
        syn::PathArguments::AngleBracketed(arguments) => arguments
            .args
            .iter()
            .map(|argument| ast_generic_argument(argument, scanner))
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok(PathSegment::with_arguments(
        NamePart::new(segment.ident.to_string()),
        arguments,
    ))
}

fn ast_generic_argument(
    argument: &syn::GenericArgument,
    scanner: &Scanner<'_>,
) -> Result<GenericArgument, ScanError> {
    match argument {
        syn::GenericArgument::Type(ty) => scanner.scan(ty).map(GenericArgument::Type),
        syn::GenericArgument::Const(expr) => Ok(GenericArgument::Const(ConstExpr::Raw(
            expr.to_token_stream().to_string(),
        ))),
        syn::GenericArgument::AssocType(associated) => {
            scanner
                .scan(&associated.ty)
                .map(|type_expr| GenericArgument::AssociatedType {
                    name: NamePart::new(associated.ident.to_string()),
                    type_expr,
                })
        }
        _ => Ok(GenericArgument::Const(ConstExpr::Raw(
            argument.to_token_stream().to_string(),
        ))),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StandardType {
    String,
    InternedString,
    Vec,
    Option,
    Result,
    HashMap,
    BTreeMap,
    Box,
    Arc,
    Builtin(BuiltinType),
}

impl StandardType {
    fn from_leaf(leaf: &str) -> Option<Self> {
        Some(match leaf {
            "String" => Self::String,
            "InternedString" => Self::InternedString,
            "Vec" => Self::Vec,
            "Option" => Self::Option,
            "Result" => Self::Result,
            "HashMap" => Self::HashMap,
            "BTreeMap" => Self::BTreeMap,
            "Box" => Self::Box,
            "Arc" => Self::Arc,
            "Duration" => Self::Builtin(BuiltinType::Duration),
            "SystemTime" => Self::Builtin(BuiltinType::SystemTime),
            "Uuid" => Self::Builtin(BuiltinType::Uuid),
            "Url" => Self::Builtin(BuiltinType::Url),
            _ => return None,
        })
    }

    fn accepts_path(self, path: &str) -> bool {
        self.paths().contains(&path)
    }

    fn paths(self) -> &'static [&'static str] {
        match self {
            Self::String => &["String", "std::string::String", "alloc::string::String"],
            Self::InternedString => &[
                "InternedString",
                "boltffi::InternedString",
                "boltffi_core::InternedString",
            ],
            Self::Vec => &["Vec", "std::vec::Vec", "alloc::vec::Vec"],
            Self::Option => &["Option", "std::option::Option", "core::option::Option"],
            Self::Result => &["Result", "std::result::Result", "core::result::Result"],
            Self::HashMap => &["HashMap", "std::collections::HashMap"],
            Self::BTreeMap => &[
                "BTreeMap",
                "std::collections::BTreeMap",
                "alloc::collections::BTreeMap",
            ],
            Self::Box => &["Box", "std::boxed::Box", "alloc::boxed::Box"],
            Self::Arc => &["std::sync::Arc", "alloc::sync::Arc"],
            Self::Builtin(BuiltinType::Duration) => {
                &["Duration", "std::time::Duration", "core::time::Duration"]
            }
            Self::Builtin(BuiltinType::SystemTime) => &["SystemTime", "std::time::SystemTime"],
            Self::Builtin(BuiltinType::Uuid) => &["Uuid", "uuid::Uuid"],
            Self::Builtin(BuiltinType::Url) => &["Url", "url::Url"],
        }
    }
}

fn path_without_arguments(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn type_arguments(segment: &syn::PathSegment) -> Vec<&syn::Type> {
    match &segment.arguments {
        syn::PathArguments::AngleBracketed(bracketed) => bracketed
            .args
            .iter()
            .filter_map(|argument| match argument {
                syn::GenericArgument::Type(ty) => Some(ty),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn closure_bound(
    bound: &syn::TraitBound,
) -> Option<(FnTraitKind, &syn::ParenthesizedGenericArguments)> {
    let segment = bound.path.segments.last()?;
    let kind = closure_kind(&segment.ident.to_string())?;
    let syn::PathArguments::Parenthesized(arguments) = &segment.arguments else {
        return None;
    };
    Some((kind, arguments))
}

fn closure_kind(name: &str) -> Option<FnTraitKind> {
    Some(match name {
        "Fn" => FnTraitKind::Fn,
        "FnMut" => FnTraitKind::FnMut,
        "FnOnce" => FnTraitKind::FnOnce,
        _ => return None,
    })
}

enum TraitBoundPart {
    Base(BaseTrait),
    Additional(AdditionalBound),
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        AdditionalBound, BaseTrait, BuiltinType, ClassId, Primitive, RecordId, TraitBounds, TraitId,
    };

    use super::*;
    use crate::ModuleScope;
    use crate::declared_types::DeclaredTypes;
    use crate::path::ModulePath;

    fn ty(source: &str) -> syn::Type {
        syn::parse_str(source).expect("valid type")
    }

    fn item(source: &str) -> syn::Item {
        syn::parse_str(source).expect("valid item")
    }

    fn scan(source: &str) -> Result<TypeExpr, ScanError> {
        Scanner::new(&DeclaredTypes::new(), &ModuleScope::root("demo")).scan(&ty(source))
    }

    fn dyn_trait_with_bounds(id: &str, path: &str, bounds: Vec<AdditionalBound>) -> TypeExpr {
        TypeExpr::Dyn(TraitBounds::new(
            BaseTrait::Named {
                id: TraitId::new(id),
                path: Path::single(path),
            },
            bounds,
        ))
    }

    fn impl_fn_with_bounds(function_trait: FnTrait, bounds: Vec<AdditionalBound>) -> TypeExpr {
        TypeExpr::ImplTrait(TraitBounds::new(
            BaseTrait::Function(Box::new(function_trait)),
            bounds,
        ))
    }

    #[test]
    fn scans_primitives_and_standard_containers_without_binding_folding() {
        assert_eq!(scan("i32"), Ok(TypeExpr::Primitive(Primitive::I32)));
        assert_eq!(scan("String"), Ok(TypeExpr::String));
        assert_eq!(scan("str"), Ok(TypeExpr::Str));
        assert_eq!(
            scan("Vec<u8>"),
            Ok(TypeExpr::vec(TypeExpr::Primitive(Primitive::U8)))
        );
        assert_eq!(
            scan("[u8]"),
            Ok(TypeExpr::slice(TypeExpr::Primitive(Primitive::U8)))
        );
        assert_eq!(
            scan("Option<Vec<u8>>"),
            Ok(TypeExpr::option(TypeExpr::vec(TypeExpr::Primitive(
                Primitive::U8
            ))))
        );
    }

    #[test]
    fn scans_nested_interned_string_with_canonical_pool_identity_and_source_path() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_test_interned_string_pool(
            "demo::pools::BrowserName",
            vec!["Chrome".to_owned()],
        );
        let items = [item("use crate::pools::BrowserName;")];
        let module = ModuleScope::new(ModulePath::root("demo").child("api"), &items);
        let scanner = Scanner::new(&declared_types, &module);

        let scanned = scanner
            .scan(&ty("boltffi::InternedString<BrowserName>"))
            .expect("declared pool should resolve");
        let TypeExpr::InternedString {
            path,
            pool_id,
            pool,
            static_values,
        } = scanned
        else {
            panic!("expected interned string type");
        };
        assert_eq!(
            path,
            Path::new(
                PathRoot::Relative,
                vec![
                    PathSegment::new("boltffi"),
                    PathSegment::new("InternedString")
                ],
            )
        );
        assert_eq!(pool_id, "demo::pools::BrowserName");
        assert_eq!(pool, Path::single("BrowserName"));
        assert_eq!(static_values, vec!["Chrome"]);
    }

    #[test]
    fn scans_legacy_builtin_value_types() {
        assert_eq!(
            scan("Duration"),
            Ok(TypeExpr::builtin(BuiltinType::Duration))
        );
        assert_eq!(
            scan("std::time::Duration"),
            Ok(TypeExpr::builtin(BuiltinType::Duration))
        );
        assert_eq!(
            scan("SystemTime"),
            Ok(TypeExpr::builtin(BuiltinType::SystemTime))
        );
        assert_eq!(scan("Uuid"), Ok(TypeExpr::builtin(BuiltinType::Uuid)));
        assert_eq!(scan("url::Url"), Ok(TypeExpr::builtin(BuiltinType::Url)));
    }

    #[test]
    fn declared_types_named_like_builtins_win_over_builtin_leaf_names() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::Duration"));
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);

        assert_eq!(
            scanner.scan(&ty("Duration")),
            Ok(TypeExpr::record(
                RecordId::new("demo::Duration"),
                Path::single("Duration")
            ))
        );
    }

    #[test]
    fn external_imports_do_not_fall_back_to_builtin_leaf_names() {
        let items = [item("use time::Duration;")];
        let module = ModuleScope::new(ModulePath::root("demo"), &items);
        let declared_types = DeclaredTypes::new();
        let scanner = Scanner::new(&declared_types, &module);

        assert_eq!(
            scanner.scan(&ty("Duration")),
            Err(ScanError::unsupported_type(&ty("Duration")))
        );
    }

    #[test]
    fn scans_result_tuple_and_maps_as_source_shapes() {
        assert_eq!(
            scan("Result<(u32,), String>"),
            Ok(TypeExpr::result(
                TypeExpr::tuple(vec![TypeExpr::Primitive(Primitive::U32)]),
                TypeExpr::String
            ))
        );
        assert_eq!(
            scan("HashMap<String, i32>"),
            Ok(TypeExpr::map(
                MapKind::Hash,
                TypeExpr::String,
                TypeExpr::Primitive(Primitive::I32)
            ))
        );
    }

    #[test]
    fn scans_declared_types_with_source_paths() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::Point"));
        declared_types.register_class(ClassId::new("demo::Engine"));
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);

        assert_eq!(
            scanner.scan(&ty("Point")),
            Ok(TypeExpr::record(
                RecordId::new("demo::Point"),
                Path::single("Point")
            ))
        );
        assert_eq!(
            scanner.scan(&ty("Option<Engine>")),
            Ok(TypeExpr::option(TypeExpr::class(
                ClassId::new("demo::Engine"),
                Path::single("Engine")
            )))
        );
    }

    #[test]
    fn scans_callback_traits_only_behind_dyn_or_impl_trait() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_trait(TraitId::new("demo::Listener"));
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);

        assert_eq!(
            scanner.scan(&ty("Box<dyn Listener>")),
            Ok(TypeExpr::boxed(TypeExpr::dyn_trait(
                TraitId::new("demo::Listener"),
                Path::single("Listener")
            )))
        );
        assert_eq!(
            scanner.scan(&ty("Box<dyn Listener + Send>")),
            Ok(TypeExpr::boxed(dyn_trait_with_bounds(
                "demo::Listener",
                "Listener",
                vec![AdditionalBound::AutoTrait(Path::single("Send"))]
            )))
        );
        assert_eq!(
            scanner.scan(&ty("impl Listener")),
            Ok(TypeExpr::impl_trait(
                TraitId::new("demo::Listener"),
                Path::single("Listener")
            ))
        );
        assert_eq!(
            scanner.scan(&ty("Listener")),
            Err(ScanError::unsupported_type(&ty("Listener")))
        );
    }

    #[test]
    fn scans_closure_source_forms() {
        assert_eq!(
            scan("fn(u32) -> String"),
            Ok(TypeExpr::fn_ptr(FnSig::new(
                vec![TypeExpr::Primitive(Primitive::U32)],
                ReturnDef::Value(TypeExpr::String)
            )))
        );
        assert_eq!(
            scan("impl FnMut(u32) -> String"),
            Ok(TypeExpr::impl_fn(FnTrait::new(
                FnTraitKind::FnMut,
                FnSig::new(
                    vec![TypeExpr::Primitive(Primitive::U32)],
                    ReturnDef::Value(TypeExpr::String)
                )
            )))
        );
        assert_eq!(
            scan("impl FnMut(u32) -> String + Send + 'static"),
            Ok(impl_fn_with_bounds(
                FnTrait::new(
                    FnTraitKind::FnMut,
                    FnSig::new(
                        vec![TypeExpr::Primitive(Primitive::U32)],
                        ReturnDef::Value(TypeExpr::String)
                    )
                ),
                vec![
                    AdditionalBound::AutoTrait(Path::single("Send")),
                    AdditionalBound::Lifetime("'static".to_owned())
                ]
            ))
        );
        assert_eq!(
            scan("Box<dyn FnOnce(u32)>"),
            Ok(TypeExpr::boxed(TypeExpr::dyn_fn(FnTrait::new(
                FnTraitKind::FnOnce,
                FnSig::new(vec![TypeExpr::Primitive(Primitive::U32)], ReturnDef::Void)
            ))))
        );
    }

    #[test]
    fn rejects_borrowed_string_closure_returns() {
        assert!(scan("fn() -> &'static str").is_err());
        assert!(scan("impl Fn() -> &'static str").is_err());
        assert!(scan("Box<dyn Fn() -> &'static str>").is_err());
    }
}
