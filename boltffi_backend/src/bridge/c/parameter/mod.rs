mod byte_slice;
mod callback_completion;
mod closure;
mod closure_return;
mod continuation;
mod direct_vector;
mod direct_writeback;
mod encoded_writeback;
mod group;

use crate::core::Result;

use boltffi_binding::ClosureSignature;

use super::{Identifier, Type};

pub use byte_slice::ByteSliceParameter;
pub use callback_completion::CallbackCompletionParameter;
pub use closure::ClosureParameter;
pub use closure_return::ClosureReturnParameter;
pub use continuation::ContinuationParameter;
pub use direct_vector::{DirectVectorElementAbi, DirectVectorParameter};
pub use direct_writeback::DirectWritebackParameter;
pub use encoded_writeback::EncodedWritebackParameter;
pub use group::ParameterGroup;

/// A C function parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Parameter {
    name: Identifier,
    ty: Type,
    role: ParameterRole,
}

/// Position of a C ABI parameter in a function declaration.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct ParameterIndex {
    index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ParameterRole {
    Value,
    BytePointer(Identifier),
    ByteLength(Identifier),
    DirectVectorPointer {
        name: Identifier,
        element: DirectVectorElementAbi,
    },
    DirectVectorLength(Identifier),
    EncodedWriteback(Identifier),
    SuccessOut,
    CompletionStatusOut,
    CallbackCompletionCallback(Identifier),
    CallbackCompletionContext(Identifier),
    ContinuationData(Identifier),
    ContinuationCallback(Identifier),
    ClosureCall {
        name: Identifier,
        signature: ClosureSignature,
        parameters: Vec<Parameter>,
    },
    ClosureContext(Identifier),
    ClosureRelease(Identifier),
    ClosureReturn {
        name: Identifier,
        signature: ClosureSignature,
        call_type: Type,
        parameters: Vec<Parameter>,
    },
}

impl Parameter {
    /// Creates a value C ABI parameter.
    pub fn new(name: impl Into<String>, ty: Type) -> Result<Self> {
        Self::with_role(name, ty, ParameterRole::Value)
    }

    /// Creates the pointer half of a borrowed byte-slice C ABI parameter group.
    pub fn byte_pointer(name: &str) -> Result<Self> {
        Self::byte_pointer_with_type(name, Type::ConstPointer(Box::new(Type::Uint8)))
    }

    /// Creates the pointer half of a mutable byte-slice C ABI parameter group.
    pub fn mutable_byte_pointer(name: &str) -> Result<Self> {
        Self::byte_pointer_with_type(name, Type::MutPointer(Box::new(Type::Uint8)))
    }

    fn byte_pointer_with_type(name: &str, ty: Type) -> Result<Self> {
        Self::with_role(
            format!("{name}_ptr"),
            ty,
            ParameterRole::BytePointer(Identifier::escape(name)?),
        )
    }

    /// Creates the length half of a borrowed byte-slice C ABI parameter group.
    pub fn byte_length(name: &str) -> Result<Self> {
        Self::with_role(
            format!("{name}_len"),
            Type::PointerWidth,
            ParameterRole::ByteLength(Identifier::escape(name)?),
        )
    }

    /// Creates the pointer half of a borrowed direct-vector C ABI parameter group.
    pub fn direct_vector_pointer(name: &str, element: DirectVectorElementAbi) -> Result<Self> {
        Self::with_role(
            format!("{name}_ptr"),
            element.pointer_type(),
            ParameterRole::DirectVectorPointer {
                name: Identifier::escape(name)?,
                element,
            },
        )
    }

    /// Creates the length half of a borrowed direct-vector C ABI parameter group.
    pub fn direct_vector_length(name: &str, element: &DirectVectorElementAbi) -> Result<Self> {
        Self::with_role(
            element.length_name(name),
            Type::PointerWidth,
            ParameterRole::DirectVectorLength(Identifier::escape(name)?),
        )
    }

    /// Creates caller-owned storage for a fallible success value.
    pub fn success_out(name: &str, ty: Type) -> Result<Self> {
        Self::with_role(name, ty, ParameterRole::SuccessOut)
    }

    /// Creates caller-owned storage for an encoded mutation result.
    pub fn encoded_writeback(name: &str) -> Result<Self> {
        Self::with_role(
            format!("{name}_out"),
            Type::MutPointer(Box::new(Type::Buffer)),
            ParameterRole::EncodedWriteback(Identifier::escape(name)?),
        )
    }

    /// Creates caller-owned status storage.
    pub fn completion_status_out(name: &str) -> Result<Self> {
        Self::with_role(
            name,
            Type::MutPointer(Box::new(Type::Status)),
            ParameterRole::CompletionStatusOut,
        )
    }

    /// Creates the callback function pointer in an async callback completion group.
    pub fn callback_completion(name: &str, ty: Type) -> Result<Self> {
        Self::with_role(
            name,
            ty,
            ParameterRole::CallbackCompletionCallback(Identifier::escape(name)?),
        )
    }

    /// Creates the context pointer in an async callback completion group.
    pub fn callback_completion_context(name: &str) -> Result<Self> {
        Self::with_role(
            format!("{name}_context"),
            Type::MutPointer(Box::new(Type::Void)),
            ParameterRole::CallbackCompletionContext(Identifier::escape(name)?),
        )
    }

    /// Creates the data half of a poll continuation C ABI parameter group.
    pub fn continuation_data(name: &str) -> Result<Self> {
        Self::with_role(
            format!("{name}_data"),
            Type::Uint64,
            ParameterRole::ContinuationData(Identifier::escape(name)?),
        )
    }

    /// Creates the function pointer half of a poll continuation C ABI parameter group.
    pub fn continuation_callback(name: &str, result: Type) -> Result<Self> {
        Self::with_role(
            name,
            Type::FunctionPointer {
                returns: Box::new(Type::Void),
                params: vec![Type::Uint64, result],
            },
            ParameterRole::ContinuationCallback(Identifier::escape(name)?),
        )
    }

    /// Creates the call function pointer in a closure C ABI parameter group.
    pub fn closure_call(
        name: &str,
        signature: &ClosureSignature,
        ty: Type,
        parameters: Vec<Parameter>,
    ) -> Result<Self> {
        Self::with_role(
            format!("{name}_call"),
            ty,
            ParameterRole::ClosureCall {
                name: Identifier::escape(name)?,
                signature: signature.clone(),
                parameters,
            },
        )
    }

    /// Creates the context pointer in a closure C ABI parameter group.
    pub fn closure_context(name: &str) -> Result<Self> {
        Self::with_role(
            format!("{name}_context"),
            Type::MutPointer(Box::new(Type::Void)),
            ParameterRole::ClosureContext(Identifier::escape(name)?),
        )
    }

    /// Creates the release function pointer in a closure C ABI parameter group.
    pub fn closure_release(name: &str) -> Result<Self> {
        Self::with_role(
            format!("{name}_release"),
            Type::FunctionPointer {
                returns: Box::new(Type::Void),
                params: vec![Type::MutPointer(Box::new(Type::Void))],
            },
            ParameterRole::ClosureRelease(Identifier::escape(name)?),
        )
    }

    /// Creates the out-pointer for a closure return C ABI parameter group.
    pub fn closure_return(
        name: &str,
        signature: &ClosureSignature,
        call_type: Type,
        parameters: Vec<Parameter>,
    ) -> Result<Self> {
        Self::with_role(
            name,
            Type::MutPointer(Box::new(Type::Void)),
            ParameterRole::ClosureReturn {
                name: Identifier::escape(name)?,
                signature: signature.clone(),
                call_type,
                parameters,
            },
        )
    }

    /// Returns the parameter name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the parameter type.
    pub fn ty(&self) -> &Type {
        &self.ty
    }

    fn with_role(name: impl Into<String>, ty: Type, role: ParameterRole) -> Result<Self> {
        Ok(Self {
            name: Identifier::escape(name)?,
            ty,
            role,
        })
    }
}

impl ParameterIndex {
    /// Returns the zero-based C ABI parameter position.
    pub const fn position(self) -> usize {
        self.index
    }

    const fn new(index: usize) -> Self {
        Self { index }
    }
}

impl ParameterRole {
    fn is_byte_length(&self, expected: &Identifier) -> bool {
        matches!(self, Self::ByteLength(name) if name == expected)
    }

    fn is_direct_vector_length(&self, expected: &Identifier) -> bool {
        matches!(self, Self::DirectVectorLength(name) if name == expected)
    }

    fn is_encoded_writeback(&self, expected: &Identifier) -> bool {
        matches!(self, Self::EncodedWriteback(name) if name == expected)
    }

    fn is_callback_completion_context(&self, expected: &Identifier) -> bool {
        matches!(self, Self::CallbackCompletionContext(name) if name == expected)
    }

    fn is_continuation_callback(&self, expected: &Identifier) -> bool {
        matches!(self, Self::ContinuationCallback(name) if name == expected)
    }

    fn is_closure_context(&self, expected: &Identifier) -> bool {
        matches!(self, Self::ClosureContext(name) if name == expected)
    }

    fn is_closure_release(&self, expected: &Identifier) -> bool {
        matches!(self, Self::ClosureRelease(name) if name == expected)
    }
}
