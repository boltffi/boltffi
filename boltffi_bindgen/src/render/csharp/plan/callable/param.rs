use super::super::super::ast::{
    CSharpArgumentList, CSharpAttribute, CSharpAttributeArg, CSharpBinaryOp, CSharpClassName,
    CSharpExpression, CSharpIdentity, CSharpLocalDecl, CSharpLocalName, CSharpMethodName,
    CSharpParamName, CSharpParameter, CSharpParameterList, CSharpPropertyName, CSharpType,
    CSharpTypeReference,
};

/// One parameter on a generated C# wrapper. May expand into one or two
/// `[DllImport]`-side parameters depending on its [`CSharpParamKind`].
///
/// Examples:
/// ```csharp
/// // Public wrapper signature: one CSharpParamPlan per input
/// public static string Echo(string s)
/// //                        ^^^^^^^^
///
/// // DllImport signature: a string CSharpParamPlan expands into
/// // (byte[] buffer, UIntPtr length)
/// internal static extern FfiBuf boltffi_echo(byte[] s, UIntPtr sLen);
/// //                                         ^^^^^^^^^^^^^^^^^^^^^^
/// ```
#[derive(Debug, Clone)]
pub struct CSharpParamPlan {
    /// Parameter name as it appears in the public wrapper signature.
    pub name: CSharpParamName,
    /// C# type as it appears in the public wrapper signature.
    pub csharp_type: CSharpType,
    /// How the parameter crosses the ABI.
    pub kind: CSharpParamKind,
}

impl CSharpParamPlan {
    /// `[DllImport]`-side declaration(s). Returns one or two
    /// parameters depending on the marshalling shape:
    /// - Primitives pass through as one parameter (with `[MarshalAs(I1)]`
    ///   on bool to override P/Invoke's 4-byte BOOL default).
    /// - Strings, wire-encoded records, and direct/pinned arrays expand
    ///   into a `(buffer, UIntPtr length)` pair.
    pub fn native_declarations(&self) -> Vec<CSharpParameter> {
        match &self.kind {
            CSharpParamKind::Direct if self.csharp_type.is_bool() => vec![CSharpParameter {
                attributes: vec![marshal_as(CSharpAttributeArg::Positional(
                    unmanaged_type_member("I1"),
                ))],
                csharp_type: CSharpType::Bool,
                name: self.name.clone(),
            }],
            CSharpParamKind::Direct => {
                vec![CSharpParameter::bare(
                    self.csharp_type.clone(),
                    self.name.clone(),
                )]
            }
            CSharpParamKind::Utf8Bytes | CSharpParamKind::WireEncoded { .. } => {
                buffer_and_length(CSharpType::Array(Box::new(CSharpType::Byte)), &self.name)
            }
            CSharpParamKind::DirectArray => {
                let element = self
                    .csharp_type
                    .array_element()
                    .expect("DirectArray param must carry an Array type")
                    .clone();
                let buf_param = CSharpParameter {
                    attributes: if matches!(element, CSharpType::Bool) {
                        vec![marshal_as_lp_array_u1()]
                    } else {
                        vec![]
                    },
                    csharp_type: CSharpType::Array(Box::new(element)),
                    name: self.name.clone(),
                };
                vec![buf_param, length_param(&self.name)]
            }
            CSharpParamKind::PinnedArray { .. } => {
                buffer_and_length(CSharpType::IntPtr, &self.name)
            }
        }
    }
}

fn length_param(base: &CSharpParamName) -> CSharpParameter {
    CSharpParameter::bare(
        CSharpType::UIntPtr,
        CSharpParamName::new(format!("{base}Len")),
    )
}

fn buffer_and_length(buffer_type: CSharpType, base: &CSharpParamName) -> Vec<CSharpParameter> {
    vec![
        CSharpParameter::bare(buffer_type, base.clone()),
        length_param(base),
    ]
}

fn marshal_as(arg: CSharpAttributeArg) -> CSharpAttribute {
    CSharpAttribute {
        name: CSharpClassName::new("MarshalAs"),
        args: vec![arg],
    }
}

fn unmanaged_type_member(member: &str) -> CSharpExpression {
    CSharpExpression::MemberAccess {
        receiver: Box::new(CSharpExpression::TypeRef(CSharpTypeReference::Plain(
            CSharpClassName::new("UnmanagedType"),
        ))),
        name: CSharpPropertyName::from_source(member),
    }
}

/// `[MarshalAs(UnmanagedType.LPArray, ArraySubType = UnmanagedType.U1)]`:
/// the single shape used to override the CLR's BOOL marshaling for
/// `bool[]` direct-array params.
fn marshal_as_lp_array_u1() -> CSharpAttribute {
    CSharpAttribute {
        name: CSharpClassName::new("MarshalAs"),
        args: vec![
            CSharpAttributeArg::Positional(unmanaged_type_member("LPArray")),
            CSharpAttributeArg::Named {
                name: CSharpPropertyName::from_source("array_sub_type"),
                value: unmanaged_type_member("U1"),
            },
        ],
    }
}

impl CSharpParamPlan {
    /// Argument expression(s) to hand to the native call. Returns one
    /// or two expressions matching the shape of `native_declarations`.
    pub fn native_call_args(&self) -> Vec<CSharpExpression> {
        match &self.kind {
            CSharpParamKind::Direct => vec![param_ident(&self.name)],
            CSharpParamKind::Utf8Bytes => {
                let buf = CSharpLocalName::for_bytes(&self.name);
                buffer_and_uintptr_length_local(buf)
            }
            CSharpParamKind::WireEncoded { binding_name } => {
                buffer_and_uintptr_length_local(binding_name.clone())
            }
            CSharpParamKind::DirectArray => buffer_and_uintptr_length_param(&self.name),
            // The Rust FFI shim for `Vec<Passable>` expects a byte length, so
            // multiply element count by `Unsafe.SizeOf<T>()` (a JIT-time
            // constant for `unmanaged` struct types).
            CSharpParamKind::PinnedArray {
                element_type,
                ptr_local,
            } => {
                let ptr_arg = CSharpExpression::Cast {
                    target: CSharpType::IntPtr,
                    inner: Box::new(CSharpExpression::Identity(CSharpIdentity::Local(
                        ptr_local.clone(),
                    ))),
                };
                let length_arg = CSharpExpression::Cast {
                    target: CSharpType::UIntPtr,
                    inner: Box::new(CSharpExpression::Paren(Box::new(
                        CSharpExpression::Binary {
                            op: CSharpBinaryOp::Mul,
                            left: Box::new(CSharpExpression::MemberAccess {
                                receiver: Box::new(param_ident(&self.name)),
                                name: CSharpPropertyName::from_source("length"),
                            }),
                            right: Box::new(CSharpExpression::MethodCall {
                                receiver: Box::new(CSharpExpression::TypeRef(
                                    CSharpTypeReference::Plain(CSharpClassName::new("Unsafe")),
                                )),
                                method: CSharpMethodName::new("SizeOf"),
                                type_args: vec![element_type.clone()],
                                args: CSharpArgumentList::default(),
                            }),
                        },
                    ))),
                };
                vec![ptr_arg, length_arg]
            }
        }
    }
}

fn param_ident(name: &CSharpParamName) -> CSharpExpression {
    CSharpExpression::Identity(CSharpIdentity::Param(name.clone()))
}

/// `(UIntPtr){receiver}.Length`, the length pair partner for
/// buffer-style native call args.
fn uintptr_length_member(receiver: CSharpExpression) -> CSharpExpression {
    CSharpExpression::Cast {
        target: CSharpType::UIntPtr,
        inner: Box::new(CSharpExpression::MemberAccess {
            receiver: Box::new(receiver),
            name: CSharpPropertyName::from_source("length"),
        }),
    }
}

fn buffer_and_uintptr_length_local(local: CSharpLocalName) -> Vec<CSharpExpression> {
    let buf = CSharpExpression::Identity(CSharpIdentity::Local(local));
    let len = uintptr_length_member(buf.clone());
    vec![buf, len]
}

fn buffer_and_uintptr_length_param(name: &CSharpParamName) -> Vec<CSharpExpression> {
    let buf = param_ident(name);
    let len = uintptr_length_member(buf.clone());
    vec![buf, len]
}

impl CSharpParamPlan {
    /// The local declaration that prepares this param before the native
    /// call, or `None` when the param passes through directly. UTF-8
    /// encoding is the only inline setup; record wire encoding needs a
    /// `using` block and is handled separately via
    /// [`CSharpFunctionPlan::wire_writers`](super::CSharpFunctionPlan::wire_writers).
    pub fn setup_declaration(&self) -> Option<CSharpLocalDecl> {
        match &self.kind {
            CSharpParamKind::Utf8Bytes => Some(CSharpLocalDecl {
                declared_type: CSharpType::Array(Box::new(CSharpType::Byte)),
                name: CSharpLocalName::for_bytes(&self.name),
                rhs: CSharpExpression::MethodCall {
                    receiver: Box::new(CSharpExpression::MemberAccess {
                        receiver: Box::new(CSharpExpression::TypeRef(CSharpTypeReference::Plain(
                            CSharpClassName::new("Encoding"),
                        ))),
                        name: CSharpPropertyName::from_source("UTF8"),
                    }),
                    method: CSharpMethodName::new("GetBytes"),
                    type_args: vec![],
                    args: vec![CSharpExpression::Identity(CSharpIdentity::Param(
                        self.name.clone(),
                    ))]
                    .into(),
                },
            }),
            _ => None,
        }
    }

    /// Whether this param needs a `fixed` statement around the native call.
    /// Drives the `unsafe { fixed (...) { ... } }` scaffolding in the
    /// wrapper templates.
    pub fn is_pinned(&self) -> bool {
        matches!(self.kind, CSharpParamKind::PinnedArray { .. })
    }
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
    /// directly as `(T[], UIntPtr)`. The CLR's default P/Invoke
    /// marshaller pins the array and hands the native side a pointer
    /// to the element buffer.
    DirectArray,
    /// A managed array of a blittable record element type, pinned with
    /// a `fixed` statement so Rust reads the C# heap directly. The
    /// wrapper passes a raw `IntPtr` and length to the DllImport.
    ///
    /// `element_type` is the element's C# type. `ptr_local` is the
    /// pointer local introduced in the `fixed` statement (e.g.,
    /// `_xPtr` for a param named `x`).
    PinnedArray {
        element_type: CSharpType,
        ptr_local: CSharpLocalName,
    },
}

pub(super) fn native_param_list(params: &[CSharpParamPlan]) -> CSharpParameterList {
    let mut list = CSharpParameterList::empty();
    for p in params {
        list.extend(p.native_declarations());
    }
    list
}

pub(super) fn native_call_arg_list(params: &[CSharpParamPlan]) -> CSharpArgumentList {
    let mut list = CSharpArgumentList::empty();
    for p in params {
        list.extend(p.native_call_args());
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(name: &str, csharp_type: CSharpType, kind: CSharpParamKind) -> CSharpParamPlan {
        CSharpParamPlan {
            name: CSharpParamName::from_source(name),
            csharp_type,
            kind,
        }
    }

    fn record_type(name: &str) -> CSharpType {
        CSharpType::Record(CSharpClassName::from_source(name).into())
    }

    fn wire_encoded_local() -> CSharpLocalName {
        CSharpLocalName::for_bytes(&CSharpParamName::from_source("person"))
    }

    /// Render a Vec of native_declarations as it would appear inside the
    /// DllImport's parens: comma-joined, matching CSharpParameterList's
    /// Display.
    fn render_native_decls(p: &CSharpParamPlan) -> String {
        let list: CSharpParameterList = p.native_declarations().into();
        list.to_string()
    }

    /// Render a Vec of native_call_args as it would appear inside the
    /// invocation's parens: comma-joined, matching CSharpArgumentList's
    /// Display.
    fn render_native_args(p: &CSharpParamPlan) -> String {
        let list: CSharpArgumentList = p.native_call_args().into();
        list.to_string()
    }

    /// Direct primitives pass through the native declaration unchanged.
    #[test]
    fn native_declaration_direct_primitive_matches_wrapper() {
        let p = param("value", CSharpType::Int, CSharpParamKind::Direct);
        assert_eq!(render_native_decls(&p), "int value");
    }

    /// P/Invoke marshals `bool` as a 4-byte Win32 BOOL by default, but the
    /// C ABI uses a 1-byte native bool, so the `DllImport` signature must
    /// force `UnmanagedType.I1`. The public wrapper side stays plain.
    #[test]
    fn native_declaration_bool_gets_marshal_attribute() {
        let p = param("flag", CSharpType::Bool, CSharpParamKind::Direct);
        assert_eq!(
            render_native_decls(&p),
            "[MarshalAs(UnmanagedType.I1)] bool flag"
        );
    }

    /// Blittable record params use `Direct` kind and pass by value, so the
    /// native declaration is just the struct name, no byte[] split.
    #[test]
    fn native_declaration_blittable_record_passes_by_value() {
        let p = param("point", record_type("point"), CSharpParamKind::Direct);
        assert_eq!(render_native_decls(&p), "Point point");
    }

    /// String params split into two arguments to match the C ABI
    /// `(const uint8_t* ptr, uintptr_t len)`.
    #[test]
    fn native_declaration_string_splits_into_bytes_and_length() {
        let p = param("v", CSharpType::String, CSharpParamKind::Utf8Bytes);
        assert_eq!(render_native_decls(&p), "byte[] v, UIntPtr vLen");
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
        assert_eq!(render_native_decls(&p), "byte[] person, UIntPtr personLen");
    }

    #[test]
    fn native_call_arg_direct_passes_name() {
        let p = param("value", CSharpType::Int, CSharpParamKind::Direct);
        assert_eq!(render_native_args(&p), "value");
    }

    #[test]
    fn native_call_arg_utf8_bytes_passes_buffer_and_length() {
        let p = param("v", CSharpType::String, CSharpParamKind::Utf8Bytes);
        assert_eq!(render_native_args(&p), "_vBytes, (UIntPtr)_vBytes.Length");
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
            render_native_args(&p),
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
