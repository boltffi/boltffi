//! [`CSharpMethod`]: a method or factory constructor on a value type
//! (enum today, records eventually). [`CSharpReceiver`] drives the
//! three rendering shapes (static, C# extension method, native instance
//! method) depending on whether the owning type can hold its own
//! members and how `self` crosses the ABI.

use super::super::CSharpType;
use super::{CSharpParam, CSharpReturnKind, CSharpWireWriter, pinned_fixed_args};

/// A method or factory constructor on a value type, today always an
/// enum, eventually also records. Renders as a static method, a C#
/// extension method (for C-style enum instance methods, since C# enums
/// can't have members), or a native instance method on the owning type.
/// The dispatch is driven by [`CSharpReceiver`].
#[derive(Debug, Clone)]
pub struct CSharpMethod {
    /// PascalCase method name as it appears on the owning type's public
    /// API (e.g., `"Opposite"`, `"UnitCircle"`).
    pub name: String,
    /// Name used for this method's DllImport entry inside the shared
    /// `NativeMethods` class. Prefixed with the owning class name (e.g.,
    /// `"DirectionOpposite"`, `"ShapeArea"`) because two types may
    /// declare methods of the same name, and the DllImport class is
    /// flat.
    pub native_method_name: String,
    /// The C FFI symbol implementing this method (e.g.,
    /// `"boltffi_direction_opposite"`).
    pub ffi_name: String,
    /// How `self` (if any) participates in the call.
    pub receiver: CSharpReceiver,
    /// Explicit params. Does not include `self` for instance methods.
    pub params: Vec<CSharpParam>,
    /// C# return type of the public-facing method.
    pub return_type: CSharpType,
    /// How the return value crosses the ABI.
    pub return_kind: CSharpReturnKind,
    /// For each non-blittable record/data-enum param, the setup block
    /// that wire-encodes it into a `byte[]` before the native call.
    pub wire_writers: Vec<CSharpWireWriter>,
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

    pub fn is_instance_native(&self) -> bool {
        matches!(self, Self::InstanceNative)
    }
}

impl CSharpMethod {
    pub fn is_void(&self) -> bool {
        matches!(self.return_kind, CSharpReturnKind::Void)
    }

    /// Comma-joined param declarations for the method signature.
    /// Excludes `self`, which the template handles separately based on
    /// the receiver kind.
    pub fn wrapper_param_list(&self) -> String {
        self.params
            .iter()
            .map(CSharpParam::wrapper_declaration)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Comma-joined call arguments for the native DllImport invocation,
    /// excluding `self`. Matches [`CSharpFunction::native_call_args`](super::CSharpFunction::native_call_args).
    pub fn native_call_args(&self) -> String {
        self.params
            .iter()
            .map(CSharpParam::native_call_arg)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The return type used in the DllImport signature. Wire-decoded
    /// returns (strings, non-blittable records, data enums) come back
    /// as an `FfiBuf`; everything else uses the C# type directly.
    pub fn native_return_type(&self) -> String {
        if self.return_kind.native_returns_ffi_buf() {
            "FfiBuf".to_string()
        } else {
            self.return_type.to_string()
        }
    }

    /// Declarations for nested `fixed` statements pinning every
    /// [`CSharpParamKind::PinnedArray`](super::CSharpParamKind::PinnedArray) param in the signature.
    pub fn pinned_fixed_args(&self) -> Vec<String> {
        pinned_fixed_args(&self.params)
    }

    pub fn has_pinned_params(&self) -> bool {
        !self.pinned_fixed_args().is_empty()
    }

    /// Param list used in the DllImport signature, including the
    /// receiver-dependent self declaration prepended when the method is
    /// an instance method:
    /// - `InstanceExtension`: prepends `{OwnerClass} self`, relying on
    ///   the CLR to marshal the enum as its declared backing integral type.
    /// - `InstanceNative`: prepends `byte[] self, UIntPtr selfLen` for
    ///   wire-encoded `this`; passes `{OwnerClass} self` for blittable
    ///   types.
    /// - `Static`: no self declaration.
    ///
    /// `owner_is_blittable` distinguishes the two `InstanceNative` sub-
    /// cases. For wire-encoded owners it's `false`; for blittable
    /// records it will be `true` once record instance methods land.
    pub fn native_param_list(&self, owner_class_name: &str, owner_is_blittable: bool) -> String {
        let explicit: Vec<String> = self
            .params
            .iter()
            .map(CSharpParam::native_declaration)
            .collect();
        let self_decl: Option<String> = match self.receiver {
            CSharpReceiver::Static => None,
            CSharpReceiver::InstanceExtension => Some(format!("{} self", owner_class_name)),
            CSharpReceiver::InstanceNative if owner_is_blittable => {
                Some(format!("{} self", owner_class_name))
            }
            CSharpReceiver::InstanceNative => Some("byte[] self, UIntPtr selfLen".to_string()),
        };
        match self_decl {
            Some(d) => std::iter::once(d)
                .chain(explicit)
                .collect::<Vec<_>>()
                .join(", "),
            None => explicit.join(", "),
        }
    }

    /// Comma-joined call arguments *including* the receiver's
    /// self-argument where the receiver needs one. Extension methods
    /// prepend the bound `self` local; data-enum instance methods
    /// prepend the pre-encoded `_selfBytes, (UIntPtr)_selfBytes.Length`
    /// pair that the surrounding method body set up.
    pub fn full_native_call_args(&self) -> String {
        let explicit = self.native_call_args();
        let self_prefix: &str = match self.receiver {
            CSharpReceiver::Static => "",
            CSharpReceiver::InstanceExtension => "self",
            CSharpReceiver::InstanceNative => "_selfBytes, (UIntPtr)_selfBytes.Length",
        };
        match (self_prefix.is_empty(), explicit.is_empty()) {
            (true, _) => explicit,
            (false, true) => self_prefix.to_string(),
            (false, false) => format!("{self_prefix}, {explicit}"),
        }
    }
}
