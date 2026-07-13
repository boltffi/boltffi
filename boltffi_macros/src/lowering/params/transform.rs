use boltffi_ffi_rules::naming;
use boltffi_ffi_rules::primitive::Primitive;
use boltffi_ffi_rules::transport::{
    CallbackParamStyle, DirectBufferParamStrategy, ParamContract, ParamPassingStrategy,
    ParamValueStrategy, ScalarParamStrategy, WireParamStrategy,
};
use quote::quote;
use syn::Type;

use crate::index::class_types::{ClassParam, ClassTypeRegistry};
use crate::index::data_types::DataTypeCategory;
use crate::lowering::transport::{
    NamedTypeTransport, NamedTypeTransportClassifier, StandardContainer, TypeShapeExt,
};

pub(super) fn ptr_ident(base: &syn::Ident) -> syn::Ident {
    syn::Ident::new(
        &format!("{}{}", base, naming::param_ptr_suffix()),
        base.span(),
    )
}

pub(super) fn len_ident(base: &syn::Ident) -> syn::Ident {
    syn::Ident::new(
        &format!("{}{}", base, naming::param_len_suffix()),
        base.span(),
    )
}

pub(super) enum ParamTransform {
    PassThrough,
    StrRef,
    StringRef,
    OwnedString,
    Callback {
        params: Vec<syn::Type>,
        returns: Option<syn::Type>,
    },
    SliceRef(syn::Type),
    SliceMut(syn::Type),
    BoxedDynTrait(syn::Path),
    ArcDynTrait(syn::Path),
    OptionBoxedDynTrait(syn::Path),
    OptionArcDynTrait(syn::Path),
    ImplTrait(syn::Path),
    VecPrimitive(syn::Type),
    VecPassable(syn::Type),
    ClassHandle(ClassHandleParam),
    WireEncoded(WireEncodedParam),
    Passable(PassableParam),
}

pub(super) struct ClassifiedParamTransform {
    pub(super) contract: ParamContract,
    pub(super) transform: ParamTransform,
}

#[derive(Clone)]
pub(super) struct WireEncodedParam {
    pub(super) kind: WireEncodedParamKind,
    pub(super) decoded_type: syn::Type,
    pub(super) passing: WireEncodedPassing,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum WireEncodedParamKind {
    Required,
    Vec,
    Option,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum WireEncodedPassing {
    ByValue,
    SharedRef,
    MutableRef,
}

#[derive(Clone)]
pub(super) struct ClassHandleParam {
    pub(super) rust_type: syn::Type,
    pub(super) kind: ClassHandleParamKind,
    pub(super) nullable: bool,
}

#[derive(Clone)]
pub(super) struct PassableParam {
    pub(super) rust_type: syn::Type,
    pub(super) passing: PassablePassing,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PassablePassing {
    ByValue,
    SharedRef,
    MutableRef,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ClassHandleParamKind {
    SharedRef,
    MutableRef,
}

impl WireEncodedParam {
    pub(super) fn from_type(kind: WireEncodedParamKind, ty: &Type) -> Self {
        match ty {
            Type::Reference(reference) if reference.mutability.is_some() => Self {
                kind,
                decoded_type: (*reference.elem).clone(),
                passing: WireEncodedPassing::MutableRef,
            },
            Type::Reference(reference) => Self {
                kind,
                decoded_type: (*reference.elem).clone(),
                passing: WireEncodedPassing::SharedRef,
            },
            _ => Self {
                kind,
                decoded_type: ty.clone(),
                passing: WireEncodedPassing::ByValue,
            },
        }
    }

    pub(super) fn from_slice_ref(inner_type: &Type) -> Self {
        let decoded_type: Type = syn::parse_quote!(Vec<#inner_type>);
        Self {
            kind: WireEncodedParamKind::Vec,
            decoded_type,
            passing: WireEncodedPassing::SharedRef,
        }
    }
}

impl PassableParam {
    fn from_type(ty: &Type) -> Self {
        match ty {
            Type::Reference(reference) if reference.mutability.is_some() => Self {
                rust_type: (*reference.elem).clone(),
                passing: PassablePassing::MutableRef,
            },
            Type::Reference(reference) => Self {
                rust_type: (*reference.elem).clone(),
                passing: PassablePassing::SharedRef,
            },
            _ => Self {
                rust_type: ty.clone(),
                passing: PassablePassing::ByValue,
            },
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ParamTransformClassifier<'a> {
    class_types: &'a ClassTypeRegistry,
    named_type_transport_classifier: NamedTypeTransportClassifier<'a>,
}

struct ParamSyntax<'a> {
    ty: &'a Type,
    spelling: String,
}

enum ParsedParamShape {
    InlineClosure(ClosureSignature),
    ImplCallbackTrait(syn::Path),
    Slice(SliceShape),
    TraitObject(TraitObjectShape),
    Vec(syn::Type),
    Option(syn::Type),
}

struct ClosureSignature {
    params: Vec<syn::Type>,
    returns: Option<syn::Type>,
}

struct SliceShape {
    inner: syn::Type,
    is_mutable: bool,
}

struct TraitObjectShape {
    trait_path: syn::Path,
    ownership: TraitObjectOwnership,
    is_optional: bool,
}

#[derive(Clone, Copy)]
enum TraitObjectOwnership {
    Boxed,
    Shared,
}

impl<'a> ParamTransformClassifier<'a> {
    pub(super) fn new(
        class_types: &'a ClassTypeRegistry,
        named_type_transport_classifier: NamedTypeTransportClassifier<'a>,
    ) -> Self {
        Self {
            class_types,
            named_type_transport_classifier,
        }
    }

    pub(super) fn classify(&self, ty: &Type) -> ClassifiedParamTransform {
        let param_syntax = ParamSyntax::new(ty);

        if let Some(class_param) = self.class_types.class_param(ty) {
            let (rust_type, kind, nullable) = match class_param {
                ClassParam::SharedRef {
                    rust_type,
                    nullable,
                } => (rust_type, ClassHandleParamKind::SharedRef, nullable),
                ClassParam::MutableRef {
                    rust_type,
                    nullable,
                } => (rust_type, ClassHandleParamKind::MutableRef, nullable),
            };
            let passing_strategy = match kind {
                ClassHandleParamKind::SharedRef => ParamPassingStrategy::SharedRef,
                ClassHandleParamKind::MutableRef => ParamPassingStrategy::MutableRef,
            };
            return ClassifiedParamTransform {
                contract: ParamContract::new(
                    ParamValueStrategy::ObjectHandle { nullable },
                    passing_strategy,
                ),
                transform: ParamTransform::ClassHandle(ClassHandleParam {
                    rust_type,
                    kind,
                    nullable,
                }),
            };
        }

        if let Some(param_shape) = param_syntax.parse_shape() {
            return match param_shape {
                ParsedParamShape::InlineClosure(closure) => ClassifiedParamTransform {
                    contract: ParamContract::new(
                        ParamValueStrategy::CallbackHandle {
                            nullable: false,
                            style: CallbackParamStyle::InlineClosure,
                        },
                        Self::passing_strategy(ty),
                    ),
                    transform: ParamTransform::Callback {
                        params: closure.params,
                        returns: closure.returns,
                    },
                },
                ParsedParamShape::ImplCallbackTrait(trait_path) => ClassifiedParamTransform {
                    contract: ParamContract::new(
                        ParamValueStrategy::CallbackHandle {
                            nullable: false,
                            style: CallbackParamStyle::ImplTrait,
                        },
                        ParamPassingStrategy::ByValue,
                    ),
                    transform: ParamTransform::ImplTrait(trait_path),
                },
                ParsedParamShape::Slice(slice) => {
                    let passing_strategy = if slice.is_mutable {
                        ParamPassingStrategy::MutableRef
                    } else {
                        ParamPassingStrategy::SharedRef
                    };
                    if slice.is_mutable {
                        return ClassifiedParamTransform {
                            contract: ParamContract::new(
                                Self::direct_buffer_value_strategy(&slice.inner),
                                passing_strategy,
                            ),
                            transform: ParamTransform::SliceMut(slice.inner),
                        };
                    }
                    match self.direct_slice_value_strategy(&slice.inner) {
                        Some(value_strategy) => ClassifiedParamTransform {
                            contract: ParamContract::new(value_strategy, passing_strategy),
                            transform: ParamTransform::SliceRef(slice.inner),
                        },
                        None => ClassifiedParamTransform {
                            contract: ParamContract::new(
                                ParamValueStrategy::WireEncoded(WireParamStrategy::Vec),
                                passing_strategy,
                            ),
                            transform: ParamTransform::WireEncoded(
                                WireEncodedParam::from_slice_ref(&slice.inner),
                            ),
                        },
                    }
                }
                ParsedParamShape::TraitObject(trait_object) => {
                    let is_optional = trait_object.is_optional;
                    let style = trait_object.callback_style();
                    let transform = trait_object.into_transform();
                    ClassifiedParamTransform {
                        contract: ParamContract::new(
                            ParamValueStrategy::CallbackHandle {
                                nullable: is_optional,
                                style,
                            },
                            ParamPassingStrategy::ByValue,
                        ),
                        transform,
                    }
                }
                ParsedParamShape::Vec(inner_ty) => {
                    let inner_str = quote!(#inner_ty).to_string().replace(' ', "");
                    if is_primitive_vec_inner(&inner_str) {
                        ClassifiedParamTransform {
                            contract: ParamContract::new(
                                ParamValueStrategy::DirectBuffer(
                                    DirectBufferParamStrategy::ScalarElements,
                                ),
                                ParamPassingStrategy::ByValue,
                            ),
                            transform: ParamTransform::VecPrimitive(inner_ty),
                        }
                    } else if self
                        .named_type_transport_classifier
                        .supports_direct_vec_transport(&inner_ty)
                    {
                        ClassifiedParamTransform {
                            contract: ParamContract::new(
                                ParamValueStrategy::DirectBuffer(
                                    DirectBufferParamStrategy::CompositeElements,
                                ),
                                ParamPassingStrategy::ByValue,
                            ),
                            transform: ParamTransform::VecPassable(inner_ty),
                        }
                    } else {
                        ClassifiedParamTransform {
                            contract: ParamContract::new(
                                ParamValueStrategy::WireEncoded(WireParamStrategy::Vec),
                                ParamPassingStrategy::ByValue,
                            ),
                            transform: ParamTransform::WireEncoded(WireEncodedParam::from_type(
                                WireEncodedParamKind::Vec,
                                ty,
                            )),
                        }
                    }
                }
                ParsedParamShape::Option(inner_ty) => {
                    if let Some(trait_object) = TraitObjectShape::parse_optional(ty) {
                        let is_optional = trait_object.is_optional;
                        let style = trait_object.callback_style();
                        let transform = trait_object.into_transform();
                        ClassifiedParamTransform {
                            contract: ParamContract::new(
                                ParamValueStrategy::CallbackHandle {
                                    nullable: is_optional,
                                    style,
                                },
                                ParamPassingStrategy::ByValue,
                            ),
                            transform,
                        }
                    } else {
                        let _ = inner_ty;
                        ClassifiedParamTransform {
                            contract: ParamContract::new(
                                ParamValueStrategy::WireEncoded(WireParamStrategy::Option),
                                Self::passing_strategy(ty),
                            ),
                            transform: ParamTransform::WireEncoded(WireEncodedParam::from_type(
                                WireEncodedParamKind::Option,
                                ty,
                            )),
                        }
                    }
                }
            };
        }

        if param_syntax.is_raw_pointer() {
            return ClassifiedParamTransform {
                contract: ParamContract::new(
                    ParamValueStrategy::Scalar(ScalarParamStrategy::PrimitiveValue),
                    ParamPassingStrategy::ByValue,
                ),
                transform: ParamTransform::PassThrough,
            };
        }

        if param_syntax.is_extern_fn_pointer() {
            return ClassifiedParamTransform {
                contract: ParamContract::new(
                    ParamValueStrategy::Scalar(ScalarParamStrategy::PrimitiveValue),
                    ParamPassingStrategy::ByValue,
                ),
                transform: ParamTransform::PassThrough,
            };
        }

        if ty.is_generic_nominal_type() {
            return ClassifiedParamTransform {
                contract: ParamContract::new(
                    ParamValueStrategy::WireEncoded(WireParamStrategy::SingleValue),
                    Self::passing_strategy(ty),
                ),
                transform: ParamTransform::WireEncoded(WireEncodedParam::from_type(
                    WireEncodedParamKind::Required,
                    ty,
                )),
            };
        }

        if param_syntax.is_str_ref() {
            return ClassifiedParamTransform {
                contract: ParamContract::new(
                    ParamValueStrategy::Utf8String,
                    ParamPassingStrategy::SharedRef,
                ),
                transform: ParamTransform::StrRef,
            };
        }

        if param_syntax.is_string_ref() {
            return ClassifiedParamTransform {
                contract: ParamContract::new(
                    ParamValueStrategy::Utf8String,
                    ParamPassingStrategy::SharedRef,
                ),
                transform: ParamTransform::StringRef,
            };
        }

        if let Type::Reference(type_ref) = ty
            && let Some(category) = self
                .named_type_transport_classifier
                .named_type_category(&type_ref.elem)
        {
            return match category {
                DataTypeCategory::Scalar | DataTypeCategory::Blittable => {
                    ClassifiedParamTransform {
                        contract: ParamContract::new(
                            self.named_value_strategy(&type_ref.elem),
                            Self::passing_strategy(ty),
                        ),
                        transform: ParamTransform::Passable(PassableParam::from_type(ty)),
                    }
                }
                DataTypeCategory::WireEncoded | DataTypeCategory::NativeOpaque => {
                    ClassifiedParamTransform {
                        contract: ParamContract::new(
                            ParamValueStrategy::WireEncoded(WireParamStrategy::SingleValue),
                            Self::passing_strategy(ty),
                        ),
                        transform: ParamTransform::WireEncoded(WireEncodedParam::from_type(
                            WireEncodedParamKind::Required,
                            ty,
                        )),
                    }
                }
            };
        }

        if param_syntax.is_owned_string() {
            return ClassifiedParamTransform {
                contract: ParamContract::new(
                    ParamValueStrategy::Utf8String,
                    ParamPassingStrategy::ByValue,
                ),
                transform: ParamTransform::OwnedString,
            };
        }

        if ty.is_named_nominal_type() {
            return match self
                .named_type_transport_classifier
                .classify_named_type_transport(ty)
            {
                NamedTypeTransport::Passable => ClassifiedParamTransform {
                    contract: ParamContract::new(
                        self.named_value_strategy(ty),
                        Self::passing_strategy(ty),
                    ),
                    transform: ParamTransform::Passable(PassableParam::from_type(ty)),
                },
                NamedTypeTransport::WireEncoded => ClassifiedParamTransform {
                    contract: ParamContract::new(
                        ParamValueStrategy::WireEncoded(WireParamStrategy::SingleValue),
                        Self::passing_strategy(ty),
                    ),
                    transform: ParamTransform::WireEncoded(WireEncodedParam::from_type(
                        WireEncodedParamKind::Required,
                        ty,
                    )),
                },
            };
        }

        ClassifiedParamTransform {
            contract: ParamContract::new(
                ParamValueStrategy::Scalar(ScalarParamStrategy::PrimitiveValue),
                Self::passing_strategy(ty),
            ),
            transform: ParamTransform::PassThrough,
        }
    }

    fn passing_strategy(ty: &Type) -> ParamPassingStrategy {
        match ty {
            Type::Reference(reference) if reference.mutability.is_some() => {
                ParamPassingStrategy::MutableRef
            }
            Type::Reference(_) => ParamPassingStrategy::SharedRef,
            _ => ParamPassingStrategy::ByValue,
        }
    }

    fn direct_buffer_value_strategy(inner_ty: &Type) -> ParamValueStrategy {
        if inner_ty.is_primitive_type() {
            ParamValueStrategy::DirectBuffer(DirectBufferParamStrategy::ScalarElements)
        } else {
            ParamValueStrategy::DirectBuffer(DirectBufferParamStrategy::CompositeElements)
        }
    }

    fn direct_slice_value_strategy(&self, inner_ty: &Type) -> Option<ParamValueStrategy> {
        if inner_ty.is_primitive_type() {
            return Some(ParamValueStrategy::DirectBuffer(
                DirectBufferParamStrategy::ScalarElements,
            ));
        }

        matches!(
            self.named_type_transport_classifier
                .named_type_category(inner_ty),
            Some(DataTypeCategory::Blittable)
        )
        .then_some(ParamValueStrategy::DirectBuffer(
            DirectBufferParamStrategy::CompositeElements,
        ))
    }

    fn named_value_strategy(&self, ty: &Type) -> ParamValueStrategy {
        match self.named_type_transport_classifier.named_type_category(ty) {
            Some(DataTypeCategory::Scalar) => {
                ParamValueStrategy::Scalar(ScalarParamStrategy::CStyleEnumTag)
            }
            Some(DataTypeCategory::Blittable) => ParamValueStrategy::CompositeValue,
            Some(DataTypeCategory::WireEncoded | DataTypeCategory::NativeOpaque) | None => {
                ParamValueStrategy::WireEncoded(WireParamStrategy::SingleValue)
            }
        }
    }
}

impl<'a> ParamSyntax<'a> {
    fn new(ty: &'a Type) -> Self {
        Self {
            ty,
            spelling: quote!(#ty).to_string().replace(' ', ""),
        }
    }

    fn parse_shape(&self) -> Option<ParsedParamShape> {
        ClosureSignature::parse(self.ty)
            .map(ParsedParamShape::InlineClosure)
            .or_else(|| {
                TraitObjectShape::parse_impl_trait(self.ty).map(ParsedParamShape::ImplCallbackTrait)
            })
            .or_else(|| SliceShape::parse(self.ty).map(ParsedParamShape::Slice))
            .or_else(|| {
                TraitObjectShape::parse_required(self.ty).map(ParsedParamShape::TraitObject)
            })
            .or_else(|| match self.ty.type_descriptor().standard_container() {
                Some(StandardContainer::Vec(inner_type)) => {
                    Some(ParsedParamShape::Vec(inner_type.clone()))
                }
                Some(StandardContainer::Option(inner_type)) => {
                    Some(ParsedParamShape::Option(inner_type.clone()))
                }
                Some(StandardContainer::Result { .. }) | None => None,
            })
    }

    fn is_raw_pointer(&self) -> bool {
        self.spelling.starts_with("*const") || self.spelling.starts_with("*mut")
    }

    fn is_extern_fn_pointer(&self) -> bool {
        self.spelling.contains("extern") && self.spelling.contains("fn(")
    }

    fn is_str_ref(&self) -> bool {
        self.spelling == "&str"
            || (self.spelling.starts_with("&'") && self.spelling.ends_with("str"))
    }

    fn is_string_ref(&self) -> bool {
        let Type::Reference(reference) = self.ty else {
            return false;
        };

        matches!(
            reference.elem.as_ref(),
            Type::Path(type_path)
                if type_path
                    .path
                    .segments
                    .last()
                    .is_some_and(|segment| segment.ident == "String")
        )
    }

    fn is_owned_string(&self) -> bool {
        self.spelling == "String" || self.spelling == "std::string::String"
    }
}

impl ClosureSignature {
    fn parse(ty: &Type) -> Option<Self> {
        if let Type::BareFn(bare_fn) = ty {
            return Some(Self {
                params: bare_fn.inputs.iter().map(|arg| arg.ty.clone()).collect(),
                returns: match &bare_fn.output {
                    syn::ReturnType::Default => None,
                    syn::ReturnType::Type(_, output_ty) => Some((**output_ty).clone()),
                },
            });
        }

        let Type::ImplTrait(impl_trait) = ty else {
            return None;
        };

        impl_trait
            .bounds
            .iter()
            .filter_map(|bound| match bound {
                syn::TypeParamBound::Trait(trait_bound) => Some(&trait_bound.path),
                _ => None,
            })
            .filter_map(|path| path.segments.last())
            .filter_map(|segment| {
                matches!(
                    segment.ident.to_string().as_str(),
                    "Fn" | "FnMut" | "FnOnce"
                )
                .then_some(&segment.arguments)
            })
            .filter_map(|arguments| match arguments {
                syn::PathArguments::Parenthesized(parenthesized) => Some(parenthesized),
                _ => None,
            })
            .map(|arguments| Self {
                params: arguments.inputs.iter().cloned().collect(),
                returns: match &arguments.output {
                    syn::ReturnType::Default => None,
                    syn::ReturnType::Type(_, output_ty) => Some((**output_ty).clone()),
                },
            })
            .next()
    }
}

impl SliceShape {
    fn parse(ty: &Type) -> Option<Self> {
        let Type::Reference(reference) = ty else {
            return None;
        };
        let Type::Slice(slice) = reference.elem.as_ref() else {
            return None;
        };
        Some(Self {
            inner: (*slice.elem).clone(),
            is_mutable: reference.mutability.is_some(),
        })
    }
}

impl TraitObjectShape {
    fn parse_required(ty: &Type) -> Option<Self> {
        Self::parse_container(ty, false)
    }

    fn parse_optional(ty: &Type) -> Option<Self> {
        let Some(StandardContainer::Option(inner_type)) = ty.type_descriptor().standard_container()
        else {
            return None;
        };
        Self::parse_container(inner_type, true)
    }

    fn parse_container(ty: &Type, is_optional: bool) -> Option<Self> {
        Self::parse_trait_object(ty, "Box", TraitObjectOwnership::Boxed, is_optional).or_else(
            || Self::parse_trait_object(ty, "Arc", TraitObjectOwnership::Shared, is_optional),
        )
    }

    fn parse_trait_object(
        ty: &Type,
        container_name: &str,
        ownership: TraitObjectOwnership,
        is_optional: bool,
    ) -> Option<Self> {
        let Type::Path(type_path) = ty else {
            return None;
        };
        if type_path.qself.is_some() {
            return None;
        }
        let segment = type_path.path.segments.last()?;
        if segment.ident != container_name {
            return None;
        }
        let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
            return None;
        };
        let syn::GenericArgument::Type(Type::TraitObject(trait_object)) = arguments.args.first()?
        else {
            return None;
        };
        let syn::TypeParamBound::Trait(trait_bound) = trait_object.bounds.first()? else {
            return None;
        };
        Some(Self {
            trait_path: trait_bound.path.clone(),
            ownership,
            is_optional,
        })
    }

    fn parse_impl_trait(ty: &Type) -> Option<syn::Path> {
        let Type::ImplTrait(impl_trait) = ty else {
            return None;
        };

        impl_trait
            .bounds
            .iter()
            .filter_map(|bound| match bound {
                syn::TypeParamBound::Trait(trait_bound) => {
                    Some((trait_bound.modifier, &trait_bound.path))
                }
                _ => None,
            })
            .filter(|(modifier, path)| {
                let trait_name = path
                    .segments
                    .last()
                    .map(|segment| segment.ident.to_string())
                    .unwrap_or_default();
                !Self::is_non_callback_bound(*modifier, &trait_name)
            })
            .map(|(_, path)| path.clone())
            .next()
    }

    fn is_non_callback_bound(modifier: syn::TraitBoundModifier, name: &str) -> bool {
        if matches!(modifier, syn::TraitBoundModifier::Maybe(_)) && name == "Sized" {
            return true;
        }
        matches!(
            name,
            "Fn" | "FnMut"
                | "FnOnce"
                | "Send"
                | "Sync"
                | "Unpin"
                | "UnwindSafe"
                | "RefUnwindSafe"
                | "Sized"
                | "Copy"
                | "Clone"
                | "Default"
                | "Debug"
                | "Eq"
                | "PartialEq"
                | "Ord"
                | "PartialOrd"
                | "Hash"
        )
    }

    fn callback_style(&self) -> CallbackParamStyle {
        match self.ownership {
            TraitObjectOwnership::Boxed => CallbackParamStyle::BoxedDyn,
            TraitObjectOwnership::Shared => CallbackParamStyle::ArcDyn,
        }
    }

    fn into_transform(self) -> ParamTransform {
        match (self.ownership, self.is_optional) {
            (TraitObjectOwnership::Boxed, false) => ParamTransform::BoxedDynTrait(self.trait_path),
            (TraitObjectOwnership::Shared, false) => ParamTransform::ArcDynTrait(self.trait_path),
            (TraitObjectOwnership::Boxed, true) => {
                ParamTransform::OptionBoxedDynTrait(self.trait_path)
            }
            (TraitObjectOwnership::Shared, true) => {
                ParamTransform::OptionArcDynTrait(self.trait_path)
            }
        }
    }
}

pub(super) fn is_primitive_vec_inner(type_string: &str) -> bool {
    type_string.parse::<Primitive>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::class_types::ClassTypeRegistry;
    use crate::index::custom_types::CustomTypeRegistry;
    use crate::index::data_types::{DataTypeCategory, DataTypeRegistry};

    fn classifier(data_types: &'static DataTypeRegistry) -> ParamTransformClassifier<'static> {
        let class_types = Box::leak(Box::new(ClassTypeRegistry::default()));
        let custom_types = Box::leak(Box::new(CustomTypeRegistry::default()));
        ParamTransformClassifier::new(
            class_types,
            NamedTypeTransportClassifier::new(custom_types, data_types),
        )
    }

    fn classifier_with_classes(
        class_types: &'static ClassTypeRegistry,
    ) -> ParamTransformClassifier<'static> {
        let custom_types = Box::leak(Box::new(CustomTypeRegistry::default()));
        let data_types = Box::leak(Box::new(DataTypeRegistry::default()));
        ParamTransformClassifier::new(
            class_types,
            NamedTypeTransportClassifier::new(custom_types, data_types),
        )
    }

    fn empty_data_types() -> &'static DataTypeRegistry {
        Box::leak(Box::new(DataTypeRegistry::default()))
    }

    fn data_types(entries: &[(&str, DataTypeCategory)]) -> &'static DataTypeRegistry {
        Box::leak(Box::new(DataTypeRegistry::with_entries(entries)))
    }

    #[test]
    fn dependency_class_reference_lowers_to_object_handle() {
        let class_types = Box::leak(Box::new(ClassTypeRegistry::with_paths(&[&[
            "session_api",
            "Session",
        ]])));
        let ty: Type = syn::parse_quote!(&session_api::Session);
        let classified = classifier_with_classes(class_types).classify(&ty);

        assert_eq!(
            classified.contract.value_strategy(),
            ParamValueStrategy::ObjectHandle { nullable: false },
        );
        assert_eq!(
            classified.contract.passing_strategy(),
            ParamPassingStrategy::SharedRef,
        );
        assert!(matches!(
            classified.transform,
            ParamTransform::ClassHandle(ClassHandleParam {
                kind: ClassHandleParamKind::SharedRef,
                nullable: false,
                ..
            })
        ));
    }

    #[test]
    fn nullable_dependency_class_reference_lowers_to_nullable_object_handle() {
        let class_types = Box::leak(Box::new(ClassTypeRegistry::with_paths(&[&[
            "session_api",
            "Session",
        ]])));
        let ty: Type = syn::parse_quote!(Option<&mut session_api::Session>);
        let classified = classifier_with_classes(class_types).classify(&ty);

        assert_eq!(
            classified.contract.value_strategy(),
            ParamValueStrategy::ObjectHandle { nullable: true },
        );
        assert_eq!(
            classified.contract.passing_strategy(),
            ParamPassingStrategy::MutableRef,
        );
        assert!(matches!(
            classified.transform,
            ParamTransform::ClassHandle(ClassHandleParam {
                kind: ClassHandleParamKind::MutableRef,
                nullable: true,
                ..
            })
        ));
    }

    #[test]
    fn primitive_slice_stays_direct_buffer() {
        let ty: Type = syn::parse_quote!(&[i32]);
        let classified = classifier(empty_data_types()).classify(&ty);

        assert_eq!(
            classified.contract.value_strategy(),
            ParamValueStrategy::DirectBuffer(DirectBufferParamStrategy::ScalarElements),
        );
        assert_eq!(
            classified.contract.passing_strategy(),
            ParamPassingStrategy::SharedRef,
        );
        assert!(matches!(classified.transform, ParamTransform::SliceRef(_)));
    }

    #[test]
    fn blittable_record_slice_stays_direct_buffer() {
        let ty: Type = syn::parse_quote!(&[Point]);
        let classified =
            classifier(data_types(&[("Point", DataTypeCategory::Blittable)])).classify(&ty);

        assert_eq!(
            classified.contract.value_strategy(),
            ParamValueStrategy::DirectBuffer(DirectBufferParamStrategy::CompositeElements),
        );
        assert_eq!(
            classified.contract.passing_strategy(),
            ParamPassingStrategy::SharedRef,
        );
        assert!(matches!(classified.transform, ParamTransform::SliceRef(_)));
    }

    #[test]
    fn wire_encoded_record_slice_decodes_vec_storage() {
        let ty: Type = syn::parse_quote!(&[Person]);
        let classified =
            classifier(data_types(&[("Person", DataTypeCategory::WireEncoded)])).classify(&ty);

        assert_eq!(
            classified.contract.value_strategy(),
            ParamValueStrategy::WireEncoded(WireParamStrategy::Vec),
        );
        assert_eq!(
            classified.contract.passing_strategy(),
            ParamPassingStrategy::SharedRef,
        );
        let ParamTransform::WireEncoded(wire_param) = classified.transform else {
            panic!("expected wire encoded slice transform");
        };
        assert!(matches!(wire_param.kind, WireEncodedParamKind::Vec));
        assert!(matches!(wire_param.passing, WireEncodedPassing::SharedRef));
        let decoded_type = &wire_param.decoded_type;
        assert_eq!(
            quote!(#decoded_type).to_string().replace(' ', ""),
            "Vec<Person>",
        );
    }
}
