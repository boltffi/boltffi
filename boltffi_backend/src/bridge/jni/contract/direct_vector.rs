//! Direct-vector parameters passed from Java into Rust through JNI.
//!
//! The lower C bridge exposes direct vectors as pointer plus element count. JNI
//! cannot pass that shape directly, so the Java side receives a primitive array
//! and the generated native method borrows or copies its storage before calling
//! the C bridge.
//!
//! This module records that whole JNI contract once: the Java array parameter,
//! the local pointer and length variables, the C pointer cast, and the optional
//! stack-copy path used for small primitive arrays. Templates consume this
//! prepared contract. They do not decide which vectors are primitive, which ones
//! are packed record bytes, or which JNI array function should be called.

use crate::{
    bridge::{
        c::{self, DirectVectorElementAbi, Expression, Identifier, TypeFragment},
        jni::JniType,
    },
    core::Result,
};

const DIRECT_VECTOR_STACK_COPY_MAX_LEN: usize = 8;

/// A Java primitive-array parameter expanded to C pointer and length arguments.
///
/// The value represents one direct-vector parameter after the C bridge has
/// already grouped the ABI pieces. It keeps the Java parameter name beside the
/// exact local variables that must be passed into the C bridge call.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct DirectVectorParameter {
    name: Identifier,
    pointer: Identifier,
    length: Identifier,
    pointer_type: TypeFragment,
    jni_type: JniType,
    access: ArrayAccess,
    stack_copy: Option<DirectVectorStackCopy>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArrayAccess {
    Read,
    ReadWrite,
}

/// Stack storage policy for a small primitive direct-vector parameter.
///
/// JNI primitive arrays can be copied into caller-owned stack storage with
/// `Get*ArrayRegion`. That avoids pinning or heap allocation for the common
/// small-array case while leaving the normal `Get*ArrayElements` path available
/// for larger arrays.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct DirectVectorStackCopy {
    max_len: usize,
    region_getter: &'static str,
}

impl DirectVectorParameter {
    /// Returns the generated JNI array parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the local pointer variable passed to the C bridge.
    pub fn pointer(&self) -> &Identifier {
        &self.pointer
    }

    /// Returns the local length variable passed to the C bridge.
    pub fn length(&self) -> &Identifier {
        &self.length
    }

    /// Returns the C pointer type expected by the C bridge.
    pub fn pointer_type(&self) -> &TypeFragment {
        &self.pointer_type
    }

    /// Returns the JNI array type.
    pub fn array_type(&self) -> TypeFragment {
        self.jni_type.as_array_type_fragment()
    }

    /// Returns the JNI primitive element type.
    pub fn jni_type(&self) -> JniType {
        self.jni_type
    }

    /// Returns the JNI array element type.
    pub fn element_type(&self) -> TypeFragment {
        self.jni_type.as_type_fragment()
    }

    /// Returns the `Get*ArrayElements` JNI function table member.
    pub fn getter(&self) -> &'static str {
        self.jni_type.array_elements_getter()
    }

    /// Returns the `Release*ArrayElements` JNI function table member.
    pub fn releaser(&self) -> &'static str {
        self.jni_type.array_elements_releaser()
    }

    /// Returns the JNI array release mode.
    pub const fn release_mode(&self) -> &'static str {
        match self.access {
            ArrayAccess::Read => "JNI_ABORT",
            ArrayAccess::ReadWrite => "0",
        }
    }

    /// Returns the stack-copy path for small primitive arrays.
    pub fn stack_copy(&self) -> Option<&DirectVectorStackCopy> {
        self.stack_copy.as_ref()
    }

    /// Returns C bridge call arguments produced from this Java array.
    pub fn c_arguments(&self) -> Vec<Expression> {
        vec![
            Expression::cast(
                self.pointer_type.clone(),
                Expression::identifier(self.pointer.clone()),
            ),
            Expression::cast(
                TypeFragment::new("uintptr_t"),
                Expression::identifier(self.length.clone()),
            ),
        ]
    }

    /// Creates a direct-vector JNI parameter from a C direct-vector parameter group.
    pub fn from_c_group(vector: &c::DirectVectorParameter, function: &c::Function) -> Result<Self> {
        let pointer = function.parameter(vector.pointer());
        let access = match pointer.ty() {
            c::Type::MutPointer(_) => ArrayAccess::ReadWrite,
            _ => ArrayAccess::Read,
        };
        let jni_type = JniType::from_direct_vector_element(vector.element())?;
        Ok(Self {
            pointer: Identifier::parse(format!("__boltffi_{}_ptr", vector.name()))?,
            length: Identifier::parse(format!("__boltffi_{}_len", vector.name()))?,
            pointer_type: TypeFragment::anonymous(pointer.ty())?,
            stack_copy: (access == ArrayAccess::Read
                && matches!(vector.element(), DirectVectorElementAbi::Typed(_)))
            .then(|| DirectVectorStackCopy::for_primitive(jni_type)),
            jni_type,
            access,
            name: Identifier::escape(vector.name())?,
        })
    }
}

impl DirectVectorStackCopy {
    /// Creates the stack-copy policy for a primitive JNI array.
    pub fn for_primitive(jni_type: JniType) -> Self {
        Self {
            max_len: DIRECT_VECTOR_STACK_COPY_MAX_LEN,
            region_getter: jni_type.array_region_getter(),
        }
    }

    /// Returns the largest Java array length copied through the stack buffer.
    pub fn max_len(&self) -> usize {
        self.max_len
    }

    /// Returns the `Get*ArrayRegion` JNI function table member.
    pub fn region_getter(&self) -> &'static str {
        self.region_getter
    }
}
