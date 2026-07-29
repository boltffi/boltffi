use boltffi_ffi_rules::primitive::Primitive;
use syn::Type;

use crate::index::data_types::DataTypeCategory;
use crate::lowering::transport::{
    NamedTypeTransport, RustTypeShape, StandardContainer, TypeDescriptor,
};

use super::model::{
    EncodedReturnStrategy, ReturnLoweringContext, ScalarReturnStrategy, ValueReturnStrategy,
};

pub(super) fn option_primitive_uses_scalar_encoding(primitive: Primitive) -> bool {
    !matches!(primitive, Primitive::I64 | Primitive::U64)
}

/// Whether `Option<inner_type>` can cross as a scalar because `inner_type` is a
/// C-style enum whose discriminants fit in the `f64` slot wasm uses.
pub(super) fn option_scalar_enum_uses_scalar_encoding(
    inner_type: &Type,
    return_lowering: &ReturnLoweringContext<'_>,
) -> bool {
    matches!(
        return_lowering.data_types().category_for(inner_type),
        Some(DataTypeCategory::Scalar(repr)) if repr.fits_in_f64()
    )
}

#[derive(Clone, Copy)]
pub struct ReturnTypeDescriptor<'a> {
    type_descriptor: TypeDescriptor<'a>,
}

impl<'a> ReturnTypeDescriptor<'a> {
    pub fn parse(rust_type: &'a Type) -> Self {
        Self {
            type_descriptor: TypeDescriptor::new(rust_type),
        }
    }

    pub fn option_primitive(&self) -> Option<Primitive> {
        self.option_inner_type()
            .and_then(|inner_type| TypeDescriptor::new(inner_type).primitive())
    }

    pub fn option_inner_type(&self) -> Option<&'a Type> {
        match self.type_descriptor.standard_container() {
            Some(StandardContainer::Option(inner_type)) => Some(inner_type),
            _ => None,
        }
    }

    pub fn option_encoded_return_strategy(&self) -> EncodedReturnStrategy {
        let Some(primitive) = self.option_primitive() else {
            return EncodedReturnStrategy::WireEncoded;
        };

        if option_primitive_uses_scalar_encoding(primitive) {
            EncodedReturnStrategy::OptionScalar
        } else {
            EncodedReturnStrategy::WireEncoded
        }
    }

    pub fn is_primitive(&self) -> bool {
        matches!(
            self.type_descriptor.shape(),
            RustTypeShape::Unit | RustTypeShape::Primitive(_)
        )
    }
}

pub fn classify_value_return_strategy(
    rust_type: &Type,
    return_lowering: &ReturnLoweringContext<'_>,
) -> ValueReturnStrategy {
    if return_lowering.object_handle_return(rust_type).is_some() {
        return ValueReturnStrategy::ObjectHandle;
    }

    let return_type = ReturnTypeDescriptor::parse(rust_type);

    if return_lowering.class_types().is_class_type(rust_type) {
        return ValueReturnStrategy::ObjectHandle;
    }

    match return_type.type_descriptor.shape() {
        RustTypeShape::Unit => ValueReturnStrategy::Void,
        RustTypeShape::Utf8String => ValueReturnStrategy::Buffer(EncodedReturnStrategy::Utf8String),
        RustTypeShape::Primitive(_) => {
            ValueReturnStrategy::Scalar(ScalarReturnStrategy::PrimitiveValue)
        }
        RustTypeShape::StandardContainer(StandardContainer::Vec(inner_type)) => {
            let buffer_strategy = if return_lowering
                .named_type_transport_classifier()
                .supports_direct_vec_transport(inner_type)
            {
                EncodedReturnStrategy::DirectVec
            } else {
                EncodedReturnStrategy::WireEncoded
            };
            ValueReturnStrategy::Buffer(buffer_strategy)
        }
        RustTypeShape::StandardContainer(StandardContainer::Result { ok, err }) => {
            if ReturnTypeDescriptor::parse(ok).is_primitive()
                && ReturnTypeDescriptor::parse(err).is_primitive()
            {
                ValueReturnStrategy::Buffer(EncodedReturnStrategy::ResultScalar)
            } else {
                ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded)
            }
        }
        RustTypeShape::StandardContainer(StandardContainer::Option(inner_type)) => {
            if ReturnTypeDescriptor::parse(inner_type).is_primitive() {
                ValueReturnStrategy::Buffer(return_type.option_encoded_return_strategy())
            } else if option_scalar_enum_uses_scalar_encoding(inner_type, return_lowering) {
                // A C-style enum crosses as its discriminant, so it can ride the
                // same scalar slot as an optional primitive instead of a buffer.
                ValueReturnStrategy::Buffer(EncodedReturnStrategy::OptionScalar)
            } else {
                ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded)
            }
        }
        RustTypeShape::NamedNominal | RustTypeShape::GenericNominal | RustTypeShape::Other => {
            match return_lowering
                .named_type_transport_classifier()
                .classify_named_type_transport(rust_type)
            {
                NamedTypeTransport::WireEncoded => {
                    ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded)
                }
                NamedTypeTransport::Passable => {
                    match return_lowering.data_types().category_for(rust_type) {
                        Some(DataTypeCategory::Scalar(_)) => {
                            ValueReturnStrategy::Scalar(ScalarReturnStrategy::CStyleEnumTag)
                        }
                        Some(DataTypeCategory::Blittable) => ValueReturnStrategy::CompositeValue,
                        Some(DataTypeCategory::WireEncoded) | None => unreachable!(
                            "passable return transport requires scalar or blittable data type"
                        ),
                    }
                }
            }
        }
    }
}
