use std::num::NonZeroUsize;

use boltffi_ast::{
    AdditionalBound, BaseTrait, BuiltinType, FnSig, FnTrait, FnTraitKind, MapKind, NamePart, Path,
    PathRoot, PathSegment, Primitive, RecordId, ReturnDef, TraitBounds, TraitId, TypeExpr,
};
use quote::ToTokens;

use crate::ScanError;
use crate::declared_types::DeclaredTypes;
use crate::unsupported::UnsupportedFeature;

pub struct Scanner<'a> {
    declared_types: &'a DeclaredTypes,
}

impl<'a> Scanner<'a> {
    pub fn new(declared_types: &'a DeclaredTypes) -> Self {
        Self { declared_types }
    }

    pub fn scan(&self, ty: &syn::Type) -> Result<TypeExpr, ScanError> {
        let unwrapped = unwrapped(ty);
        match unwrapped {
            syn::Type::FnPtr(fn_ptr) => self.fn_ptr(fn_ptr),
            syn::Type::ImplTrait(impl_trait) => self.impl_trait(impl_trait, ty),
            syn::Type::Slice(slice) => self.slice(slice),
            syn::Type::Path(type_path) => self.path(type_path, ty),
            syn::Type::TraitObject(trait_object) => self.dyn_trait(trait_object, ty),
            syn::Type::Tuple(tuple) => self.tuple(tuple),
            syn::Type::Array(array) => Err(ScanError::unsupported_rust_type(
                ty,
                format!("use `Vec<{}>`", crate::spelling::ty(&array.elem)),
            )),
            syn::Type::Never(_) => Err(ScanError::unsupported_rust_type(
                ty,
                "an exported function must return a value or `()`",
            )),
            syn::Type::Ptr(_) => Err(ScanError::unsupported_rust_type(
                ty,
                "pass an owned value, a reference, or `Vec<u8>`",
            )),
            syn::Type::Reference(reference) => Err(ScanError::unsupported_rust_type(
                ty,
                format!("use the owned type `{}`", owned(reference)),
            )),
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
            _ => Err(ScanError::unsupported_rust_type(
                source,
                format!(
                    "only `&str` and `&[u8]` can be returned by reference, so return `{}`",
                    owned(reference)
                ),
            )),
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
            StandardType::Option => {
                let argument = self.single_type_argument(segment, source)?;
                if let syn::Type::Reference(reference) = unwrapped(argument) {
                    return Err(ScanError::unsupported_rust_type(
                        source,
                        format!("use `Option<{}>`", owned(reference)),
                    ));
                }
                self.scan(argument).map(TypeExpr::option)
            }
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
        if let (StandardType::Result, [ok]) = (standard_type, type_arguments(segment).as_slice())
            && unsupported_standard_type(&type_path.path).is_none()
        {
            return Err(ScanError::unsupported_rust_type(
                source,
                format!(
                    "BoltFFI can't see through a `Result` alias, so write `Result<{}, E>`",
                    crate::spelling::ty(ok)
                ),
            ));
        }
        Ok(standard_type
            .accepts_path(&path_without_arguments(&type_path.path))
            .then_some(standard_type))
    }

    fn named(&self, type_path: &syn::TypePath, source: &syn::Type) -> Result<TypeExpr, ScanError> {
        if let Some(advice) = unsupported_standard_type(&type_path.path) {
            return Err(ScanError::unsupported_rust_type(source, advice));
        }
        if type_path
            .path
            .segments
            .iter()
            .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
        {
            let index = self.declared_types.defer_slot(&type_path.path);
            return Ok(TypeExpr::record(
                RecordId::new(format!("$slot:{index}")),
                ast_path_without_arguments(&type_path.path),
            ));
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
        if let Some(primitive) = primitive_path(&type_path.path) {
            return Ok(TypeExpr::Primitive(primitive));
        }
        let index = self.declared_types.defer_slot(&type_path.path);
        Ok(TypeExpr::record(
            RecordId::new(format!("$slot:{index}")),
            ast_path_without_arguments(&type_path.path),
        ))
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
        if is_standard_error_trait(&bound.path) {
            return Err(ScanError::unsupported_rust_type(
                source,
                "return `Result<T, String>` or a `#[error]` type",
            ));
        }
        if let Some(base) = self.base_trait(bound)? {
            return Ok(TraitBoundPart::Base(base));
        }
        if let Some(bound) = self.additional_bound(bound) {
            return Ok(TraitBoundPart::Additional(bound));
        }
        Err(ScanError::unsupported_type(source))
    }

    fn additional_bound(&self, bound: &syn::TraitBound) -> Option<AdditionalBound> {
        if crate::unsupported::has_bound_modifier(bound)
            || bound.lifetimes.is_some()
            || closure_bound(bound).is_some()
            || bound
                .path
                .segments
                .iter()
                .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
        {
            return None;
        }
        Some(AdditionalBound::AutoTrait(ast_path_without_arguments(
            &bound.path,
        )))
    }

    fn base_trait(&self, bound: &syn::TraitBound) -> Result<Option<BaseTrait>, ScanError> {
        if crate::unsupported::has_bound_modifier(bound) || bound.lifetimes.is_some() {
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
        let path = ast_path_without_arguments(&bound.path);
        if is_auto_trait_path(&bound.path) {
            return Ok(None);
        }
        let index = self.declared_types.defer_trait_slot(&bound.path);
        Ok(Some(BaseTrait::Named {
            id: TraitId::new(format!("$slot:{index}")),
            path,
        }))
    }

    fn fn_ptr(&self, fn_ptr: &syn::TypeFnPtr) -> Result<TypeExpr, ScanError> {
        if fn_ptr.lifetimes.is_some() {
            return Err(crate::unsupported::feature(
                UnsupportedFeature::HigherRankedFunctionPointer,
            )
            .at(fn_ptr));
        }
        if fn_ptr.unsafety.is_some() {
            return Err(
                crate::unsupported::feature(UnsupportedFeature::UnsafeFunctionPointer).at(fn_ptr),
            );
        }
        if fn_ptr.abi.is_some() {
            return Err(
                crate::unsupported::feature(UnsupportedFeature::ExternFunctionPointer).at(fn_ptr),
            );
        }
        if fn_ptr.variadic.is_some() {
            return Err(
                crate::unsupported::feature(UnsupportedFeature::VariadicFunctionPointer).at(fn_ptr),
            );
        }
        let parameters = fn_ptr
            .inputs
            .iter()
            .map(|argument| self.scan(&argument.ty))
            .collect::<Result<Vec<_>, _>>()?;
        let returns = self.scan_return(&fn_ptr.output)?;
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
            .map(|input| self.scan(&input.ty))
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
        let index = self.declared_types.defer_slot(&pool_path.path);
        Ok(TypeExpr::interned_string(
            ast_path_without_arguments(&type_path.path),
            format!("$slot:{index}"),
            ast_path_without_arguments(&pool_path.path),
            Vec::new(),
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
}

/// Resolves a type path to a primitive: a bare spelling like `i64`, or the
/// canonical qualified forms `std::primitive::i64` / `core::primitive::i64`.
/// Any other qualified path may name a user type that shares the leaf
/// spelling, so it does not resolve.
pub fn primitive_path(path: &syn::Path) -> Option<Primitive> {
    if path
        .segments
        .iter()
        .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
    {
        return None;
    }
    let primitive = Primitive::from_rust_name(&path.segments.last()?.ident.to_string())?;
    is_primitive_form(&segment_names(path)).then_some(primitive)
}

fn segment_names(path: &syn::Path) -> Vec<String> {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect()
}

/// Whether a path spells its leaf bare or as `std::primitive::` / `core::primitive::`.
fn is_primitive_form(names: &[String]) -> bool {
    names.len() == 1
        || (names.len() == 3
            && matches!(names[0].as_str(), "std" | "core")
            && names[1] == "primitive")
}

/// What to use instead of a standard-library type that has no boundary mapping.
fn unsupported_standard_type(path: &syn::Path) -> Option<&'static str> {
    let names = segment_names(path);
    let leaf = names.last()?.as_str();
    if is_primitive_form(&names) {
        return match leaf {
            "char" => Some("use `u32` for a code point or `String` for text"),
            "i128" | "u128" => Some("use `i64` or `u64`, or `String` for larger values"),
            _ => None,
        };
    }
    if !matches!(names[0].as_str(), "std" | "core" | "alloc") {
        return None;
    }
    match leaf {
        "HashSet" | "BTreeSet" | "VecDeque" | "LinkedList" | "BinaryHeap" => Some("use `Vec<T>`"),
        "PathBuf" | "Path" | "OsString" | "OsStr" | "CString" | "CStr" => Some("use `String`"),
        "Rc" | "Cell" | "RefCell" | "Mutex" | "RwLock" => {
            Some("pass the value itself, or keep shared state in an exported class")
        }
        "Result" => Some("use `Result<T, E>` with `String` or a `#[error]` type as `E`"),
        _ => None,
    }
}

fn is_standard_error_trait(path: &syn::Path) -> bool {
    let names = segment_names(path);
    names.len() == 3
        && matches!(names[0].as_str(), "std" | "core")
        && names[1..] == ["error", "Error"]
}

/// The owned type to use instead of a reference.
fn owned(reference: &syn::TypeReference) -> String {
    match unwrapped(&reference.elem) {
        syn::Type::Path(path) if path.path.is_ident("str") => "String".to_owned(),
        syn::Type::Slice(slice) => format!("Vec<{}>", crate::spelling::ty(&slice.elem)),
        elem @ syn::Type::TraitObject(_) => format!("Box<{}>", crate::spelling::ty(elem)),
        elem @ syn::Type::ImplTrait(_) => {
            let spelled = crate::spelling::ty(elem);
            format!("Box<dyn {}>", spelled.trim_start_matches("impl "))
        }
        elem => crate::spelling::ty(elem),
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

/// Whether a bound path names a well-known auto trait, which stays an
/// additional bound instead of becoming a deferred base-trait slot.
fn is_auto_trait_path(path: &syn::Path) -> bool {
    path.segments.last().is_some_and(|segment| {
        matches!(
            segment.ident.to_string().as_str(),
            "Send" | "Sync" | "Unpin"
        )
    })
}

fn ast_path_without_arguments(path: &syn::Path) -> Path {
    Path::new(
        path_root(path),
        path_segments_without_arguments(path).collect(),
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
            Self::Arc => &["Arc", "std::sync::Arc", "alloc::sync::Arc"],
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
    use boltffi_ast::{AdditionalBound, BaseTrait, BuiltinType, Primitive, TraitBounds};

    use super::*;

    use crate::declared_types::DeclaredTypes;

    fn ty(source: &str) -> syn::Type {
        syn::parse_str(source).expect("valid type")
    }

    #[test]
    fn references_to_trait_objects_suggest_a_box() {
        let owned_of = |source: &str| match ty(source) {
            syn::Type::Reference(reference) => owned(&reference),
            _ => panic!("reference type"),
        };

        assert_eq!(owned_of("&dyn Listener"), "Box<dyn Listener>");
        assert_eq!(owned_of("&impl Fn()"), "Box<dyn Fn()>");
        assert_eq!(owned_of("&[u8]"), "Vec<u8>");
        assert_eq!(owned_of("&str"), "String");
    }

    #[test]
    fn standard_types_without_a_mapping_get_advice_and_user_paths_do_not() {
        let advice = |source: &str| match ty(source) {
            syn::Type::Path(path) => unsupported_standard_type(&path.path),
            _ => panic!("path type"),
        };

        assert_eq!(advice("char"), advice("std::primitive::char"));
        assert!(advice("char").is_some());
        assert!(advice("std::collections::HashSet<String>").is_some());
        assert!(advice("std::io::Result<i32>").is_some());
        assert_eq!(advice("geometry::HashSet"), None);
        assert_eq!(advice("HashSet<String>"), None);
        assert_eq!(advice("std::string::String"), None);
    }

    fn scan(source: &str) -> Result<TypeExpr, ScanError> {
        Scanner::new(&DeclaredTypes::new()).scan(&ty(source))
    }

    fn impl_fn_with_bounds(function_trait: FnTrait, bounds: Vec<AdditionalBound>) -> TypeExpr {
        TypeExpr::ImplTrait(TraitBounds::new(
            BaseTrait::Function(Box::new(function_trait)),
            bounds,
        ))
    }

    #[test]
    fn resolves_primitives_from_bare_and_canonical_spellings_only() {
        assert_eq!(
            scan("std::primitive::i64"),
            Ok(TypeExpr::Primitive(Primitive::I64))
        );
        assert_eq!(
            scan("core::primitive::f32"),
            Ok(TypeExpr::Primitive(Primitive::F32))
        );
        assert!(
            !matches!(scan("mymod::i64"), Ok(TypeExpr::Primitive(_))),
            "a user type sharing a primitive leaf spelling must not resolve as that primitive"
        );
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
