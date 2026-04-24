//! Callables: things you invoke across the ABI. Holds the two callable
//! shapes ([`CSharpFunction`] top-level, [`CSharpMethod`] on a type)
//! plus the per-parameter vocabulary they both use: [`CSharpParam`] +
//! [`CSharpParamKind`] decide how a value crosses the boundary, and
//! [`CSharpWireWriter`] carries the setup block for wire-encoded
//! record params.

mod function;
mod method;

pub use function::{CSharpFunction, CSharpReturnKind};
pub use method::{CSharpMethod, CSharpReceiver};

use super::{
    CSharpExpression, CSharpLocalDecl, CSharpLocalName, CSharpParamName, CSharpStatement,
    CSharpType,
};

/// A parameter in a C# function.
#[derive(Debug, Clone)]
pub struct CSharpParam {
    /// Parameter name as it appears in the public wrapper signature.
    pub name: CSharpParamName,
    /// C# type as it appears in the public wrapper signature.
    pub csharp_type: CSharpType,
    /// How the parameter crosses the ABI.
    pub kind: CSharpParamKind,
}

impl CSharpParam {
    /// Declaration as it appears in the public wrapper signature,
    /// e.g. `"int value"`, `"string v"`, `"Point point"`.
    pub fn wrapper_declaration(&self) -> String {
        format!("{} {}", self.csharp_type, self.name)
    }

    /// Declaration as it appears in the `[DllImport]` signature. This
    /// is where the different marshalling paths diverge:
    /// - Primitives and blittable records pass through directly.
    /// - Bool needs the `[MarshalAs(UnmanagedType.I1)]` attribute
    ///   because P/Invoke defaults to the 4-byte Win32 BOOL.
    /// - Strings and wire-encoded records are split into
    ///   `(byte[] x, UIntPtr xLen)`.
    pub fn native_declaration(&self) -> String {
        match &self.kind {
            CSharpParamKind::Utf8Bytes | CSharpParamKind::WireEncoded { .. } => {
                format!("byte[] {name}, UIntPtr {name}Len", name = self.name)
            }
            CSharpParamKind::Direct if self.csharp_type.is_bool() => {
                format!("[MarshalAs(UnmanagedType.I1)] bool {}", self.name)
            }
            CSharpParamKind::Direct => {
                format!("{} {}", self.csharp_type, self.name)
            }
            CSharpParamKind::DirectArray => {
                let element = self
                    .csharp_type
                    .array_element()
                    .expect("DirectArray param must carry an Array type");
                let decl = format!("{element}[] {name}, UIntPtr {name}Len", name = self.name);
                if matches!(element, CSharpType::Bool) {
                    format!(
                        "[MarshalAs(UnmanagedType.LPArray, ArraySubType = UnmanagedType.U1)] {decl}"
                    )
                } else {
                    decl
                }
            }
            // The wrapper's `fixed` block takes the managed array and
            // hands the native side a raw pointer, so the DllImport sees
            // only `IntPtr` and a length. No element type, no P/Invoke
            // marshaling.
            CSharpParamKind::PinnedArray { .. } => {
                format!("IntPtr {name}, UIntPtr {name}Len", name = self.name)
            }
        }
    }

    /// The argument expression to hand to the native call: either the
    /// raw param, or the pre-encoded byte array plus its length.
    pub fn native_call_arg(&self) -> String {
        match &self.kind {
            CSharpParamKind::Direct => self.name.as_str().to_string(),
            CSharpParamKind::Utf8Bytes => {
                let buf = CSharpLocalName::for_bytes(&self.name);
                format!("{buf}, (UIntPtr){buf}.Length")
            }
            CSharpParamKind::WireEncoded { binding_name } => {
                format!("{binding_name}, (UIntPtr){binding_name}.Length")
            }
            CSharpParamKind::DirectArray => {
                format!("{name}, (UIntPtr){name}.Length", name = self.name)
            }
            // `_{name}Ptr` is the pointer local introduced by the
            // enclosing `fixed` statement; see `pinned_fixed_args`. The
            // cast to `IntPtr` matches the DllImport signature.
            //
            // The Rust FFI shim for `Vec<Passable>` takes a raw byte
            // length and divides by `size_of::<T>()` to recover the
            // element count, the opposite of `Vec<Primitive>`, which
            // takes element count directly. The primitive path and this
            // path therefore send different numbers across the same
            // `UIntPtr` slot. `Unsafe.SizeOf<T>()` is a JIT-time constant
            // for `unmanaged` struct types, so the multiply folds away.
            CSharpParamKind::PinnedArray { element_type } => {
                let ptr_name = self
                    .pinned_ptr_name()
                    .expect("PinnedArray params must have a pointer local");
                format!(
                    "(IntPtr){ptr_name}, (UIntPtr)({name}.Length * Unsafe.SizeOf<{element_type}>())",
                    ptr_name = ptr_name,
                    name = self.name,
                )
            }
        }
    }

    /// The local declaration that prepares this param before the
    /// native call, or `None` when the param passes through directly.
    /// UTF-8 encoding is the only inline setup; record wire encoding
    /// needs a `using` block and is handled separately via
    /// [`CSharpFunction::wire_writers`](super::CSharpFunction::wire_writers).
    pub fn setup_declaration(&self) -> Option<CSharpLocalDecl> {
        match &self.kind {
            CSharpParamKind::Utf8Bytes => Some(CSharpLocalDecl {
                declared_type: CSharpType::Array(Box::new(CSharpType::Byte)),
                name: CSharpLocalName::for_bytes(&self.name),
                rhs: CSharpExpression::Raw(format!("Encoding.UTF8.GetBytes({})", self.name)),
            }),
            _ => None,
        }
    }

    pub fn pinned_fixed_arg(&self) -> Option<String> {
        match &self.kind {
            CSharpParamKind::PinnedArray { element_type } => Some(format!(
                "{element_type}* {ptr_name} = {name}",
                ptr_name = self
                    .pinned_ptr_name()
                    .expect("PinnedArray params must have a pointer local"),
                name = self.name,
            )),
            _ => None,
        }
    }

    fn pinned_ptr_name(&self) -> Option<String> {
        match self.kind {
            CSharpParamKind::PinnedArray { .. } => {
                let base_name = self
                    .name
                    .as_str()
                    .strip_prefix('@')
                    .unwrap_or(self.name.as_str());
                Some(format!("_{base_name}Ptr"))
            }
            _ => None,
        }
    }
}

pub(super) fn pinned_fixed_args(params: &[CSharpParam]) -> Vec<String> {
    params
        .iter()
        .filter_map(CSharpParam::pinned_fixed_arg)
        .collect()
}

/// How a parameter is marshalled across the C# / C ABI boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSharpParamKind {
    /// Passed directly as a primitive (bool, int, double, etc.).
    Direct,
    /// A managed `string` that must be UTF-8 encoded into a `byte[]`
    /// and passed as `(byte[], UIntPtr)` to the native call.
    Utf8Bytes,
    /// A record that must be wire-encoded into a `byte[]` by a
    /// `WireWriter` and passed as `(byte[], UIntPtr)`. `binding_name`
    /// is the local holding the encoded byte array.
    WireEncoded { binding_name: CSharpLocalName },
    /// A managed array of a blittable primitive element type, passed
    /// directly as `(T[], UIntPtr)` without any wire encoding. The CLR's
    /// default P/Invoke marshaller pins the array and hands the native
    /// side a pointer to the element buffer. `bool[]` gets an explicit
    /// `[MarshalAs(LPArray, ArraySubType = U1)]` override so CLR emits
    /// one byte per element instead of the 4-byte Win32 BOOL default.
    DirectArray,
    /// A managed array of a blittable record element type, pinned with
    /// a `fixed` statement so Rust can read directly from the C# heap.
    ///
    /// The struct layout of a blittable record matches Rust's `#[repr(C)]`
    /// exactly, so a pointer to the first element plus an element count
    /// is everything Rust needs. Primitive arrays can use the CLR's
    /// built-in direct-array path, but record arrays are trickier once
    /// the element type stops being blittable to the marshaller (for
    /// example because it contains `bool` or `char`): P/Invoke may
    /// marshal through a temporary native buffer instead of exposing the
    /// managed array in place. With the right field-level marshalling
    /// that copy can still be layout-compatible, but it is no longer the
    /// zero-copy contract this fast path wants. The wrapper sidesteps the
    /// marshaller entirely by taking a raw pointer with `fixed (T* _xPtr
    /// = x)`, which pins the array in place for the duration of the
    /// native call and passes the pointer as `IntPtr`. C# and Rust then
    /// read the same block of managed heap memory.
    ///
    /// `element_type` is the C# type literal for `T` (e.g., `"Location"`),
    /// threaded here so `pinned_fixed_args` can render
    /// `Location* _xPtr = x` without re-deriving from `csharp_type`.
    PinnedArray { element_type: String },
}

/// Bookkeeping for a single record param that must be wire-encoded into a
/// `byte[]` before the native call. The template wraps these setup lines
/// in a `using` block so each `WireWriter` is disposed (and its rented
/// buffer recycled) even if the native call throws.
#[derive(Debug, Clone)]
pub struct CSharpWireWriter {
    /// Local holding the `WireWriter` instance.
    pub binding_name: CSharpLocalName,
    /// Local holding the resulting `byte[]`.
    pub bytes_binding_name: CSharpLocalName,
    /// The param this writer encodes, used to correlate with the
    /// corresponding [`CSharpParam`] at render time.
    pub param_name: CSharpParamName,
    /// Expression rendered against the param that returns its
    /// wire-encoded byte size (e.g., `point.WireEncodedSize()`).
    pub size_expr: CSharpExpression,
    /// Statement that writes the param's contents into the
    /// `WireWriter` named by `binding_name` (e.g.,
    /// `point.WireEncodeTo(_wire_point)`).
    pub encode_expr: CSharpStatement,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(name: &str, csharp_type: CSharpType, kind: CSharpParamKind) -> CSharpParam {
        CSharpParam {
            name: CSharpParamName::from_source(name),
            csharp_type,
            kind,
        }
    }

    fn record_type(name: &str) -> CSharpType {
        CSharpType::Record(super::super::CSharpClassName::from_source(name).into())
    }

    fn wire_encoded_local() -> CSharpLocalName {
        CSharpLocalName::for_bytes(&CSharpParamName::from_source("person"))
    }

    #[test]
    fn wrapper_declaration_puts_type_before_name() {
        let p = param("value", CSharpType::Int, CSharpParamKind::Direct);
        assert_eq!(p.wrapper_declaration(), "int value");
    }

    #[test]
    fn wrapper_declaration_uses_record_class_name() {
        let p = param(
            "point",
            record_type("point"),
            CSharpParamKind::Direct,
        );
        assert_eq!(p.wrapper_declaration(), "Point point");
    }

    /// Direct primitives pass through the native declaration unchanged.
    #[test]
    fn native_declaration_direct_primitive_matches_wrapper() {
        let p = param("value", CSharpType::Int, CSharpParamKind::Direct);
        assert_eq!(p.native_declaration(), "int value");
    }

    /// P/Invoke marshals `bool` as a 4-byte Win32 BOOL by default, but the
    /// C ABI uses a 1-byte native bool, so the `DllImport` signature must
    /// force `UnmanagedType.I1`. The public wrapper side stays plain.
    #[test]
    fn native_declaration_bool_gets_marshal_attribute() {
        let p = param("flag", CSharpType::Bool, CSharpParamKind::Direct);
        assert_eq!(
            p.native_declaration(),
            "[MarshalAs(UnmanagedType.I1)] bool flag"
        );
    }

    /// Blittable record params use `Direct` kind and pass by value, so the
    /// native declaration is just the struct name, no byte[] split.
    #[test]
    fn native_declaration_blittable_record_passes_by_value() {
        let p = param(
            "point",
            record_type("point"),
            CSharpParamKind::Direct,
        );
        assert_eq!(p.native_declaration(), "Point point");
    }

    /// String params split into two arguments to match the C ABI
    /// `(const uint8_t* ptr, uintptr_t len)`.
    #[test]
    fn native_declaration_string_splits_into_bytes_and_length() {
        let p = param("v", CSharpType::String, CSharpParamKind::Utf8Bytes);
        assert_eq!(p.native_declaration(), "byte[] v, UIntPtr vLen");
    }

    /// Wire-encoded record params use the same `byte[] + UIntPtr` split
    /// as strings because the C ABI signature is identical.
    #[test]
    fn native_declaration_wire_encoded_record_splits_into_bytes_and_length() {
        let p = param(
            "person",
            record_type("person"),
            CSharpParamKind::WireEncoded {
                binding_name: wire_encoded_local(),
            },
        );
        assert_eq!(p.native_declaration(), "byte[] person, UIntPtr personLen");
    }

    #[test]
    fn native_call_arg_direct_passes_name() {
        let p = param("value", CSharpType::Int, CSharpParamKind::Direct);
        assert_eq!(p.native_call_arg(), "value");
    }

    #[test]
    fn native_call_arg_utf8_bytes_passes_buffer_and_length() {
        let p = param("v", CSharpType::String, CSharpParamKind::Utf8Bytes);
        assert_eq!(p.native_call_arg(), "_vBytes, (UIntPtr)_vBytes.Length");
    }

    #[test]
    fn native_call_arg_wire_encoded_uses_binding_name() {
        let p = param(
            "person",
            record_type("person"),
            CSharpParamKind::WireEncoded {
                binding_name: wire_encoded_local(),
            },
        );
        assert_eq!(
            p.native_call_arg(),
            "_personBytes, (UIntPtr)_personBytes.Length"
        );
    }

    /// Direct params need no prep.
    #[test]
    fn setup_declaration_direct_has_none() {
        let p = param("x", CSharpType::Int, CSharpParamKind::Direct);
        assert!(p.setup_declaration().is_none());
    }

    /// Wire-encoded records use a `using` block around the call, not a
    /// flat setup line, so their setup_declaration is `None`.
    #[test]
    fn setup_declaration_wire_encoded_has_none() {
        let p = param(
            "person",
            record_type("person"),
            CSharpParamKind::WireEncoded {
                binding_name: wire_encoded_local(),
            },
        );
        assert!(p.setup_declaration().is_none());
    }

    #[test]
    fn setup_declaration_utf8_bytes_encodes_string() {
        let p = param("v", CSharpType::String, CSharpParamKind::Utf8Bytes);
        assert_eq!(
            p.setup_declaration().map(|d| d.to_string()).as_deref(),
            Some("byte[] _vBytes = Encoding.UTF8.GetBytes(v);"),
        );
    }
}
