use syn::Type;

use crate::index::custom_types::{CustomTypeRegistry, contains_custom_types};
use crate::index::data_types::{DataTypeCategory, DataTypeRegistry};

mod type_shape;

pub(crate) use type_shape::{RustTypeShape, StandardContainer, TypeDescriptor, TypeShapeExt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NamedTypeTransport {
    Passable,
    WireEncoded,
}

#[derive(Clone, Copy)]
pub(crate) struct NamedTypeTransportClassifier<'a> {
    custom_types: &'a CustomTypeRegistry,
    data_types: &'a DataTypeRegistry,
}

impl<'a> NamedTypeTransportClassifier<'a> {
    pub(crate) fn new(
        custom_types: &'a CustomTypeRegistry,
        data_types: &'a DataTypeRegistry,
    ) -> Self {
        Self {
            custom_types,
            data_types,
        }
    }

    pub(crate) fn classify_named_type_transport(&self, ty: &Type) -> NamedTypeTransport {
        if contains_custom_types(ty, self.custom_types) {
            return NamedTypeTransport::WireEncoded;
        }

        match self.data_types.category_for(ty) {
            Some(DataTypeCategory::Scalar | DataTypeCategory::Blittable) => {
                NamedTypeTransport::Passable
            }
            Some(DataTypeCategory::WireEncoded | DataTypeCategory::NativeOpaque) | None => {
                NamedTypeTransport::WireEncoded
            }
        }
    }

    pub(crate) fn supports_direct_vec_transport(&self, ty: &Type) -> bool {
        if ty.is_primitive_type() {
            return true;
        }

        if ty.is_string_like_type() || contains_custom_types(ty, self.custom_types) {
            return false;
        }

        self.data_types
            .category_for(ty)
            .is_some_and(DataTypeCategory::supports_direct_vec)
    }

    pub(crate) fn named_type_category(&self, ty: &Type) -> Option<DataTypeCategory> {
        if contains_custom_types(ty, self.custom_types) {
            return None;
        }

        self.data_types.category_for(ty)
    }
}
