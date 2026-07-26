pub use boltffi_ffi_rules::transport::{
    DirectBufferReturnMethod, EncodedReturnStrategy, ErrorReturnStrategy, ReturnContract,
    ReturnInvocationContext, ReturnPlatform, ScalarReturnStrategy, ValueReturnMethod,
    ValueReturnStrategy,
};
use syn::{ReturnType, Type};

use crate::index::class_types::ClassTypeRegistry;
use crate::index::custom_types::CustomTypeRegistry;
use crate::index::data_types::DataTypeRegistry;
use crate::index::type_paths::TypePathKey;
use crate::lowering::transport::{NamedTypeTransportClassifier, StandardContainer, TypeDescriptor};

use super::classify::classify_value_return_strategy;

#[derive(Clone)]
pub struct ResolvedReturn {
    rust_type: syn::Type,
    return_contract: ReturnContract,
    object_handle: Option<ObjectHandleReturn>,
}

impl ResolvedReturn {
    pub fn new(rust_type: syn::Type, return_contract: ReturnContract) -> Self {
        Self {
            rust_type,
            return_contract,
            object_handle: None,
        }
    }

    pub fn with_object_handle(
        rust_type: syn::Type,
        return_contract: ReturnContract,
        object_handle: ObjectHandleReturn,
    ) -> Self {
        Self {
            rust_type,
            return_contract,
            object_handle: Some(object_handle),
        }
    }

    pub fn rust_type(&self) -> &syn::Type {
        &self.rust_type
    }

    pub fn value_return_strategy(&self) -> ValueReturnStrategy {
        self.return_contract.value_strategy()
    }

    pub fn encoded_return_strategy(&self) -> Option<EncodedReturnStrategy> {
        match self.return_contract.value_strategy() {
            ValueReturnStrategy::Buffer(strategy) => Some(strategy),
            _ => None,
        }
    }

    pub fn is_unit(&self) -> bool {
        matches!(
            self.return_contract.value_strategy(),
            ValueReturnStrategy::Void
        )
    }

    pub fn is_primitive_scalar(&self) -> bool {
        matches!(
            self.return_contract.value_strategy(),
            ValueReturnStrategy::Scalar(ScalarReturnStrategy::PrimitiveValue)
        )
    }

    pub fn is_passable_value(&self) -> bool {
        matches!(
            self.return_contract.value_strategy(),
            ValueReturnStrategy::Scalar(ScalarReturnStrategy::CStyleEnumTag)
                | ValueReturnStrategy::CompositeValue
        )
    }

    pub fn is_object_handle(&self) -> bool {
        matches!(
            self.return_contract.value_strategy(),
            ValueReturnStrategy::ObjectHandle
        )
    }

    pub fn object_handle(&self) -> Option<&ObjectHandleReturn> {
        self.object_handle.as_ref()
    }

    pub fn value_return_method(
        &self,
        context: ReturnInvocationContext,
        platform: ReturnPlatform,
    ) -> ValueReturnMethod {
        self.return_contract.value_return_method(context, platform)
    }

    pub fn direct_buffer_return_method(
        &self,
        context: ReturnInvocationContext,
        platform: ReturnPlatform,
    ) -> Option<DirectBufferReturnMethod> {
        self.return_contract
            .direct_buffer_return_method(context, platform)
    }
}

#[derive(Clone, Copy)]
pub struct ReturnLoweringContext<'a> {
    class_types: &'a ClassTypeRegistry,
    custom_types: &'a CustomTypeRegistry,
    data_types: &'a DataTypeRegistry,
    self_type: Option<&'a Type>,
}

#[derive(Clone)]
pub struct ObjectHandleReturn {
    pointee: Type,
    nullable: bool,
    /// `Result<Class, E>` / `Result<Self, E>`: the Ok payload becomes a
    /// handle and the Err payload crosses via the last-error channel.
    fallible: bool,
}

impl ObjectHandleReturn {
    fn required(pointee: Type) -> Self {
        Self {
            pointee,
            nullable: false,
            fallible: false,
        }
    }

    fn nullable(pointee: Type) -> Self {
        Self {
            pointee,
            nullable: true,
            fallible: false,
        }
    }

    fn fallible(pointee: Type) -> Self {
        Self {
            pointee,
            nullable: false,
            fallible: true,
        }
    }

    pub fn pointee(&self) -> &Type {
        &self.pointee
    }

    pub fn is_nullable(&self) -> bool {
        self.nullable
    }

    pub fn is_fallible(&self) -> bool {
        self.fallible
    }
}

impl<'a> ReturnLoweringContext<'a> {
    pub fn new(
        custom_types: &'a CustomTypeRegistry,
        data_types: &'a DataTypeRegistry,
        class_types: &'a ClassTypeRegistry,
    ) -> Self {
        Self {
            class_types,
            custom_types,
            data_types,
            self_type: None,
        }
    }

    pub fn with_self_type<'b>(&'b self, self_type: &'b Type) -> ReturnLoweringContext<'b>
    where
        'a: 'b,
    {
        ReturnLoweringContext {
            custom_types: self.custom_types,
            data_types: self.data_types,
            class_types: self.class_types,
            self_type: Some(self_type),
        }
    }

    pub fn class_types(&self) -> &'a ClassTypeRegistry {
        self.class_types
    }

    pub fn custom_types(&self) -> &'a CustomTypeRegistry {
        self.custom_types
    }

    pub fn data_types(&self) -> &'a DataTypeRegistry {
        self.data_types
    }

    pub(crate) fn named_type_transport_classifier(&self) -> NamedTypeTransportClassifier<'a> {
        NamedTypeTransportClassifier::new(self.custom_types, self.data_types)
    }

    pub(crate) fn object_handle_return(&self, rust_type: &Type) -> Option<ObjectHandleReturn> {
        match TypeDescriptor::new(rust_type).standard_container() {
            Some(StandardContainer::Option(inner_type)) => self
                .object_handle_pointee(inner_type)
                .map(ObjectHandleReturn::nullable),
            Some(StandardContainer::Result { ok, .. }) => self
                .object_handle_pointee(ok)
                .map(ObjectHandleReturn::fallible),
            _ => self
                .object_handle_pointee(rust_type)
                .map(ObjectHandleReturn::required),
        }
    }

    fn object_handle_pointee(&self, rust_type: &Type) -> Option<Type> {
        if self.is_self_type(rust_type) {
            return self.self_type.cloned();
        }

        self.class_types
            .contains(rust_type)
            .then(|| rust_type.clone())
    }

    fn is_self_type(&self, rust_type: &Type) -> bool {
        TypePathKey::from_type(rust_type).is_some_and(|type_path_key| {
            type_path_key.is_single_segment()
                && type_path_key
                    .first_segment()
                    .is_some_and(|segment| segment == "Self")
        })
    }

    pub fn lower_output(&self, output: &ReturnType) -> ResolvedReturn {
        match output {
            ReturnType::Default => ResolvedReturn::new(
                syn::parse_quote!(()),
                ReturnContract::infallible(ValueReturnStrategy::Void),
            ),
            ReturnType::Type(_, rust_type) => self.lower_type(rust_type),
        }
    }

    pub fn lower_type(&self, rust_type: &Type) -> ResolvedReturn {
        let value_strategy = classify_value_return_strategy(rust_type, self);
        if matches!(value_strategy, ValueReturnStrategy::ObjectHandle)
            && let Some(object_handle) = self.object_handle_return(rust_type)
        {
            let error_strategy = if object_handle.is_fallible() {
                ErrorReturnStrategy::StatusCode
            } else {
                ErrorReturnStrategy::None
            };
            return ResolvedReturn::with_object_handle(
                rust_type.clone(),
                ReturnContract::new(value_strategy, error_strategy),
                object_handle,
            );
        }

        let return_contract = ReturnContract::new(value_strategy, ErrorReturnStrategy::None);
        ResolvedReturn::new(rust_type.clone(), return_contract)
    }
}

#[derive(Clone, Copy)]
pub struct WasmOptionScalarEncoding {
    pub(super) primitive: boltffi_ffi_rules::primitive::Primitive,
}
