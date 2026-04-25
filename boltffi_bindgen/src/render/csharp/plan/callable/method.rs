//! [`CSharpMethodPlan`]: a method or factory constructor on a value type
//! (enum today, records eventually). [`CSharpReceiver`] drives the
//! three rendering shapes (static, C# extension method, native instance
//! method) depending on whether the owning type can hold its own
//! members and how `self` crosses the ABI.

use super::super::super::ast::{
    CSharpArgumentList, CSharpClassName, CSharpExpression, CSharpIdentity, CSharpLocalName,
    CSharpMethodName, CSharpParamName, CSharpParameter, CSharpParameterList, CSharpPropertyName,
    CSharpType, CSharpTypeReference,
};
use super::super::CFunctionName;
use super::param::{native_call_arg_list, native_param_list};
use super::{CSharpParamPlan, CSharpReturnKind, CSharpWireWriterPlan};

/// A method or factory constructor on a value type, today always an
/// enum, eventually also records. The dispatch is driven by [`CSharpReceiver`].
///
/// Examples:
/// ```csharp
/// // 1. Static method (no self)
/// public static Shape MakePoint(double x, double y) => ...;
///
/// // 2. Instance extension method (C-style enum, since C# enums can't
/// //    carry members)
/// public static Direction Opposite(this Direction self) => ...;
///
/// // 3. Native instance method (data enum or record)
/// public double Area() => ...;
/// ```
#[derive(Debug, Clone)]
pub struct CSharpMethodPlan {
    /// Method name as it appears on the owning type's public API.
    pub name: CSharpMethodName,
    /// Name used for this method's DllImport entry inside the shared
    /// `NativeMethods` class. Prefixed with the owning class name (e.g.,
    /// `"DirectionOpposite"`, `"ShapeArea"`) because two types may
    /// declare methods of the same name, and the DllImport class is
    /// flat.
    pub native_method_name: CSharpMethodName,
    /// The C function implementing this method.
    pub ffi_name: CFunctionName,
    /// How `self` (if any) participates in the call.
    pub receiver: CSharpReceiver,
    /// Explicit params. Does not include `self` for instance methods.
    pub params: Vec<CSharpParamPlan>,
    /// C# return type of the public-facing method.
    pub return_type: CSharpType,
    /// How the return value crosses the ABI.
    pub return_kind: CSharpReturnKind,
    /// For each non-blittable record/data-enum param, the setup block
    /// that wire-encodes it into a `byte[]` before the native call.
    pub wire_writers: Vec<CSharpWireWriterPlan>,
}

/// How a method's receiver (`self`) participates in the rendered C#.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSharpReceiver {
    /// Static method, no `self`. Lives on whichever container the
    /// owning type uses: a companion `{Name}Methods` class for C-style
    /// enums, the abstract record for data enums, the record struct for
    /// records. Renders as `public static {ReturnType} {Name}({params})`.
    Static,
    /// Instance method on a C-style enum. Renders as a C# *extension*
    /// method `public static {ReturnType} {Name}(this {EnumType} self,
    /// {params})` in the companion class, giving `d.Name(args)` call
    /// syntax without requiring members on the enum itself. `self`
    /// passes directly to the DllImport since the CLR marshals the enum
    /// as its declared backing integral type.
    InstanceExtension,
    /// Instance method on a type that can hold its own members: data
    /// enums (on the abstract record) and records. Renders as a native
    /// method: `public {ReturnType} {Name}({params})`. When the owning
    /// type is wire-encoded (data enums, non-blittable records), the
    /// body wire-encodes `this` into a `byte[]` before the native call;
    /// blittable records pass `this` by value through P/Invoke.
    InstanceNative,
}

impl CSharpReceiver {
    pub fn is_static(&self) -> bool {
        matches!(self, Self::Static)
    }

    pub fn is_instance_extension(&self) -> bool {
        matches!(self, Self::InstanceExtension)
    }
}

impl CSharpMethodPlan {
    /// Whether the method has any
    /// [`CSharpParamKind::PinnedArray`](super::CSharpParamKind::PinnedArray)
    /// param. See [`CSharpFunctionPlan::has_pinned_params`](super::CSharpFunctionPlan::has_pinned_params).
    pub fn has_pinned_params(&self) -> bool {
        self.params.iter().any(CSharpParamPlan::is_pinned)
    }

    /// Typed param list for the DllImport signature, including the
    /// receiver-dependent self parameter prepended when the method is
    /// an instance method:
    /// - `InstanceExtension`: prepends `{OwnerClass} self`, relying on
    ///   the CLR to marshal the enum as its declared backing integral type.
    /// - `InstanceNative`: prepends `byte[] self, UIntPtr selfLen` for
    ///   wire-encoded `this`; passes `{OwnerClass} self` for blittable
    ///   types.
    /// - `Static`: no self parameter.
    ///
    /// `owner_is_blittable` distinguishes the two `InstanceNative` sub-
    /// cases. For wire-encoded owners it's `false`; for blittable
    /// records it will be `true` once record instance methods land.
    pub fn native_param_list(
        &self,
        owner_class_name: &CSharpClassName,
        owner_is_blittable: bool,
    ) -> CSharpParameterList {
        let mut list = CSharpParameterList::empty();
        match self.receiver {
            CSharpReceiver::Static => {}
            CSharpReceiver::InstanceExtension => {
                list.push(self_param(CSharpType::CStyleEnum(
                    CSharpTypeReference::Plain(owner_class_name.clone()),
                )));
            }
            CSharpReceiver::InstanceNative if owner_is_blittable => {
                list.push(self_param(CSharpType::Record(CSharpTypeReference::Plain(
                    owner_class_name.clone(),
                ))));
            }
            CSharpReceiver::InstanceNative => {
                list.push(CSharpParameter::bare(
                    CSharpType::Array(Box::new(CSharpType::Byte)),
                    CSharpParamName::new("self"),
                ));
                list.push(CSharpParameter::bare(
                    CSharpType::UIntPtr,
                    CSharpParamName::new("selfLen"),
                ));
            }
        }
        list.extend(native_param_list(&self.params));
        list
    }
}

fn self_param(csharp_type: CSharpType) -> CSharpParameter {
    CSharpParameter::bare(csharp_type, CSharpParamName::new("self"))
}

impl CSharpMethodPlan {
    /// Typed argument list *including* the receiver's self-argument
    /// where the receiver needs one. Extension methods prepend the
    /// bound `self` local; data-enum instance methods prepend the
    /// pre-encoded `_selfBytes, (UIntPtr)_selfBytes.Length` pair that
    /// the surrounding method body set up.
    pub fn full_native_call_args(&self) -> CSharpArgumentList {
        let mut list = CSharpArgumentList::empty();
        match self.receiver {
            CSharpReceiver::Static => {}
            CSharpReceiver::InstanceExtension => {
                list.push(local_ident("self"));
            }
            CSharpReceiver::InstanceNative => {
                let buf = local_ident("_selfBytes");
                list.push(buf.clone());
                list.push(uintptr_length_member(buf));
            }
        }
        list.extend(native_call_arg_list(&self.params));
        list
    }
}

fn local_ident(name: &str) -> CSharpExpression {
    CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new(name)))
}

/// `(UIntPtr){receiver}.Length`, the same shape as the per-param length
/// arg in [`super::CSharpParamPlan::native_call_args`].
fn uintptr_length_member(receiver: CSharpExpression) -> CSharpExpression {
    CSharpExpression::Cast {
        target: CSharpType::UIntPtr,
        inner: Box::new(CSharpExpression::MemberAccess {
            receiver: Box::new(receiver),
            name: CSharpPropertyName::from_source("length"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::super::ast::{
        CSharpClassName, CSharpMethodName, CSharpParamName, CSharpType,
    };
    use super::super::CSharpParamKind;
    use super::*;

    fn method(receiver: CSharpReceiver) -> CSharpMethodPlan {
        CSharpMethodPlan {
            name: CSharpMethodName::from_source("test"),
            native_method_name: CSharpMethodName::from_source("OwnerTest"),
            ffi_name: CFunctionName::new("boltffi_test".to_string()),
            receiver,
            params: vec![CSharpParamPlan {
                name: CSharpParamName::from_source("count"),
                csharp_type: CSharpType::Int,
                kind: CSharpParamKind::Direct,
            }],
            return_type: CSharpType::Void,
            return_kind: CSharpReturnKind::Void,
            wire_writers: vec![],
        }
    }

    /// Static methods take no self; the param list is just the explicit
    /// params.
    #[test]
    fn native_param_list_static_has_no_self() {
        let m = method(CSharpReceiver::Static);
        let owner = CSharpClassName::from_source("shape");
        assert_eq!(m.native_param_list(&owner, false).to_string(), "int count",);
    }

    /// C-style enum instance methods render as extensions and prepend
    /// the enum-typed self, marshalled as its backing integral type.
    #[test]
    fn native_param_list_instance_extension_prepends_enum_self() {
        let m = method(CSharpReceiver::InstanceExtension);
        let owner = CSharpClassName::from_source("direction");
        assert_eq!(
            m.native_param_list(&owner, false).to_string(),
            "Direction self, int count",
        );
    }

    /// Blittable record instance methods pass the receiver by value as
    /// a single struct argument.
    #[test]
    fn native_param_list_instance_native_blittable_prepends_record_self() {
        let m = method(CSharpReceiver::InstanceNative);
        let owner = CSharpClassName::from_source("point");
        assert_eq!(
            m.native_param_list(&owner, true).to_string(),
            "Point self, int count",
        );
    }

    /// Wire-encoded receivers (data enums, non-blittable records) split
    /// `this` into a `(byte[] self, UIntPtr selfLen)` pair, matching the
    /// non-blittable-record param shape.
    #[test]
    fn native_param_list_instance_native_wire_encoded_prepends_byte_buffer_self() {
        let m = method(CSharpReceiver::InstanceNative);
        let owner = CSharpClassName::from_source("shape");
        assert_eq!(
            m.native_param_list(&owner, false).to_string(),
            "byte[] self, UIntPtr selfLen, int count",
        );
    }
}
