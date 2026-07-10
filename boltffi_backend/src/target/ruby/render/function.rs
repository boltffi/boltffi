use askama::Template as AskamaTemplate;
use boltffi_binding::{
    DirectValueType, EncodedParamTransport, ErrorDecl, ExecutionDecl, FunctionDecl, IncomingParam,
    IntoRust, Native, OutOfRust, ParamDecl, ParamPlan, Primitive, Receive, RecordDecl, ReturnPlan,
    TypeRef, native,
};

use crate::{
    bridge::c::{CBridgeContract, Function as CFunction, Type},
    core::{Emitted, Error, RenderContext, Result},
    target::ruby::name_style::Name,
};

use super::record::{Symbols as RecordSymbols, encoded_record_supported};

/// Ruby's VM accepts fixed C-function arities in the range `-2..=15`
/// (`rb_add_method_cfunc` in `vm_method.c`). Wrappers whose argument count fits
/// are registered with that exact arity and receive each argument as a named
/// `VALUE argN` parameter: the VM (and YJIT) then pass arguments in registers
/// and enforce the count themselves, so the wrapper skips the manual check.
/// Wider argument lists fall back to the variadic `(argc, argv, self)` form.
pub(crate) const MAX_FIXED_ARITY: usize = 15;

pub(crate) fn is_variadic(argc: usize) -> bool {
    argc > MAX_FIXED_ARITY
}

/// The arity passed to `rb_define_method` and friends: the exact count for
/// fixed-arity wrappers, or `-1` for the variadic convention.
pub(crate) fn registration_arity(argc: usize) -> i32 {
    if is_variadic(argc) { -1 } else { argc as i32 }
}

/// C parameter list for a wrapper of the given arity.
pub(crate) fn wrapper_signature_params(argc: usize) -> String {
    if is_variadic(argc) {
        "int argc, VALUE *argv, VALUE self".to_owned()
    } else {
        let mut params = String::from("VALUE self");
        for index in 0..argc {
            params.push_str(&format!(", VALUE arg{index}"));
        }
        params
    }
}

/// C expression that reads positional argument `index` inside a wrapper:
/// the named `argN` parameter for fixed arity, or `argv[index]` for variadic.
pub(crate) fn arg_accessor(index: usize, variadic: bool) -> String {
    if variadic {
        format!("argv[{index}]")
    } else {
        format!("arg{index}")
    }
}

#[derive(AskamaTemplate)]
#[template(path = "target/ruby/function.c.txt", escape = "none")]
struct FunctionTemplate {
    ruby_name: String,
    wrapper: String,
    argc: usize,
    registration_arity: i32,
    signature_params: String,
    variadic: bool,
    params: Vec<ParamConversion>,
    return_decl: String,
    cleanup: Vec<String>,
    return_expr: String,
}

pub struct Function {
    pub ruby_name: String,
    pub wrapper: String,
    pub params: Vec<ParamConversion>,
    pub call: String,
    pub return_decl: String,
    pub return_expr: String,
}

impl Function {
    pub fn from_declaration(
        declaration: &FunctionDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if matches!(
            declaration.callable().execution(),
            ExecutionDecl::Asynchronous(_)
        ) {
            return Err(Error::UnsupportedTarget {
                target: "ruby",
                shape: "async function",
            });
        }
        if !matches!(declaration.callable().error(), ErrorDecl::None(_)) {
            return Err(Error::UnsupportedTarget {
                target: "ruby",
                shape: "fallible function",
            });
        }
        let c_function = bridge
            .functions()
            .iter()
            .find(|function| function.name() == declaration.symbol().name().as_str())
            .ok_or(Error::UnsupportedTarget {
                target: "ruby",
                shape: "function without C bridge symbol",
            })?;
        let ruby_name = Name::new(declaration.name()).function();
        let wrapper = format!("boltffi_ruby_{}", declaration.symbol().name().as_str());
        let parameters = declaration.callable().params();
        let variadic = is_variadic(parameters.len());
        let params = parameters
            .iter()
            .enumerate()
            .map(|(index, param)| {
                ParamConversion::from_parameter(
                    &arg_accessor(index, variadic),
                    param,
                    bridge,
                    context,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let call_args = params
            .iter()
            .flat_map(|param| param.call_args.clone())
            .collect::<Vec<_>>()
            .join(", ");
        let (return_decl, return_expr) = ReturnConversion::from_plan(
            c_function,
            declaration.callable().returns().plan(),
            bridge,
            context,
        )?
        .into_parts();
        let call = format!("{}({call_args})", c_function.name());
        Ok(Self {
            ruby_name,
            wrapper,
            params,
            call,
            return_decl,
            return_expr,
        })
    }

    pub fn render(self) -> Result<Emitted> {
        // Heap buffers built for slice params are freed after the native call
        // returns (the borrow does not outlive the call), so cleanup renders
        // between the call and the boxed return value.
        let cleanup = self
            .params
            .iter()
            .flat_map(|param| param.cleanup.clone())
            .collect::<Vec<_>>();
        let argc = self.params.len();
        Ok(Emitted::primary(
            FunctionTemplate {
                ruby_name: self.ruby_name,
                wrapper: self.wrapper,
                argc,
                registration_arity: registration_arity(argc),
                signature_params: wrapper_signature_params(argc),
                variadic: is_variadic(argc),
                params: self.params,
                return_decl: self.return_decl.replace("CALL", &self.call),
                cleanup,
                return_expr: self.return_expr,
            }
            .render()?,
        ))
    }
}

pub struct ParamConversion {
    pub declarations: Vec<String>,
    pub call_args: Vec<String>,
    pub cleanup: Vec<String>,
}

impl ParamConversion {
    fn from_parameter(
        accessor: &str,
        parameter: &ParamDecl<Native, IntoRust>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = Name::new(parameter.name()).function();
        match parameter.payload() {
            IncomingParam::Value(ParamPlan::Direct {
                ty: DirectValueType::Primitive(primitive),
                receive: Receive::ByValue,
            }) => Self::primitive(accessor, &name, *primitive),
            IncomingParam::Value(ParamPlan::Direct {
                ty: DirectValueType::Enum(_),
                receive: Receive::ByValue,
            }) => Ok(Self {
                declarations: vec![format!("int32_t {name} = NUM2INT({accessor});")],
                call_args: vec![name.to_owned()],
                cleanup: Vec::new(),
            }),
            IncomingParam::Value(ParamPlan::Direct {
                ty: DirectValueType::Record(record),
                receive,
            }) => Self::direct_record(accessor, &name, *record, *receive, bridge, context),
            IncomingParam::Value(ParamPlan::Encoded {
                ty: TypeRef::String | TypeRef::Bytes,
                shape: native::BufferShape::Slice,
                transport: EncodedParamTransport::BorrowedSlice,
                receive: Receive::ByRef,
                ..
            }) => Ok(Self::borrowed_slice(accessor, &name)),
            IncomingParam::Value(ParamPlan::Encoded {
                ty: TypeRef::String | TypeRef::Bytes,
                shape: native::BufferShape::Slice,
                transport: EncodedParamTransport::Wire,
                receive: Receive::ByValue | Receive::ByRef,
                ..
            }) => Ok(Self::encoded_slice(accessor, &name)),
            IncomingParam::Closure(_) => Err(Error::UnsupportedTarget {
                target: "ruby",
                shape: "closure parameter",
            }),
            IncomingParam::Value(ParamPlan::Encoded { .. }) => Err(Error::UnsupportedTarget {
                target: "ruby",
                shape: "unsupported encoded parameter",
            }),
            IncomingParam::Value(ParamPlan::Direct { .. }) => Err(Error::UnsupportedTarget {
                target: "ruby",
                shape: "unsupported direct parameter",
            }),
            IncomingParam::Value(_) => Err(Error::UnsupportedTarget {
                target: "ruby",
                shape: "unsupported parameter",
            }),
        }
    }

    fn primitive(accessor: &str, name: &str, primitive: Primitive) -> Result<Self> {
        Ok(Self {
            declarations: primitive_declarations(accessor, name, primitive)?,
            call_args: vec![name.to_owned()],
            cleanup: Vec::new(),
        })
    }

    fn direct_record(
        accessor: &str,
        name: &str,
        record: boltffi_binding::RecordId,
        receive: Receive,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = RecordSymbols::from_record_id(record, bridge, context)?;
        let c_type = symbols.c_type.as_deref().ok_or(Error::UnsupportedTarget {
            target: "ruby",
            shape: "direct record without C type",
        })?;
        match receive {
            Receive::ByValue => Ok(Self {
                declarations: vec![format!(
                    "{c_type} {name} = {}({accessor});",
                    symbols.unwrapper
                )],
                call_args: vec![name.to_owned()],
                cleanup: Vec::new(),
            }),
            Receive::ByRef | Receive::ByMutRef => Ok(Self {
                declarations: vec![
                    format!("VALUE {name}_value = {accessor};"),
                    format!("{c_type} *{name} = NULL;"),
                    format!(
                        "TypedData_Get_Struct({name}_value, {c_type}, &{}, {name});",
                        symbols.data_type
                    ),
                ],
                call_args: vec![name.to_owned()],
                cleanup: vec![format!("RB_GC_GUARD({name}_value);")],
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "ruby",
                shape: "unknown direct record receive mode",
            }),
        }
    }

    fn borrowed_slice(accessor: &str, name: &str) -> Self {
        Self {
            declarations: vec![
                format!("Check_Type({accessor}, T_STRING);"),
                format!("VALUE {name}_value = {accessor};"),
                format!("const uint8_t *{name}_ptr = (const uint8_t *)RSTRING_PTR({name}_value);"),
                format!("uintptr_t {name}_len = (uintptr_t)RSTRING_LEN({name}_value);"),
            ],
            call_args: vec![format!("{name}_ptr"), format!("{name}_len")],
            cleanup: vec![format!("RB_GC_GUARD({name}_value);")],
        }
    }

    /// Builds the length-prefixed wire buffer for a borrowed `&str`/`&[u8]`
    /// argument via `RB_ALLOCV_N`: small buffers land on the C stack (`alloca`)
    /// while large ones use a GC-tracked temporary so a big argument cannot
    /// overflow the stack. `RB_ALLOCV_END` frees it after the call *and* on any
    /// unwind, so an exception from a later param (e.g. `Check_Type`) or
    /// allocation cannot leak it. The single copy is intentional for now — true
    /// zero-copy needs an upstream borrowed-buffer ABI (see PR follow-up).
    fn encoded_slice(accessor: &str, name: &str) -> Self {
        Self {
            declarations: encoded_slice_declarations(accessor, name),
            call_args: vec![format!("{name}_ptr"), format!("{name}_len")],
            cleanup: vec![format!("RB_ALLOCV_END({name}_tmp);")],
        }
    }
}

/// Statements that build a heap `[u32 wire_len][body]` buffer from the Ruby
/// string argument read by `accessor`.
pub(crate) fn encoded_slice_declarations(accessor: &str, name: &str) -> Vec<String> {
    vec![
        format!("Check_Type({accessor}, T_STRING);"),
        format!("uintptr_t {name}_raw_len = (uintptr_t)RSTRING_LEN({accessor});"),
        format!("uintptr_t {name}_len = {name}_raw_len + sizeof(uint32_t);"),
        format!("VALUE {name}_tmp;"),
        format!("uint8_t *{name}_ptr = (uint8_t *)RB_ALLOCV_N(uint8_t, {name}_tmp, {name}_len);"),
        format!("uint32_t {name}_wire_len = (uint32_t){name}_raw_len;"),
        format!("memcpy({name}_ptr, &{name}_wire_len, sizeof(uint32_t));"),
        format!("memcpy({name}_ptr + sizeof(uint32_t), RSTRING_PTR({accessor}), {name}_raw_len);"),
    ]
}

struct ReturnConversion {
    declaration: String,
    expression: String,
}

impl ReturnConversion {
    fn from_plan(
        c_function: &CFunction,
        plan: &ReturnPlan<Native, OutOfRust>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match plan {
            ReturnPlan::Void => Ok(Self {
                declaration: "CALL;".to_owned(),
                expression: "Qnil".to_owned(),
            }),
            ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Primitive(primitive),
            } => {
                let c_type = c_type_name(c_function.returns())?;
                Ok(Self {
                    declaration: format!("{c_type} _ffi_ret = CALL;"),
                    expression: primitive_box(*primitive),
                })
            }
            ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Enum(_),
            } => {
                let c_type = c_type_name(c_function.returns())?;
                Ok(Self {
                    declaration: format!("{c_type} _ffi_ret = CALL;"),
                    expression: "LL2NUM((long long)_ffi_ret)".to_owned(),
                })
            }
            ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Record(record),
            } => {
                let symbols = RecordSymbols::from_record_id(*record, bridge, context)?;
                let c_type = c_type_name(c_function.returns())?;
                Ok(Self {
                    declaration: format!("{c_type} _ffi_ret = CALL;"),
                    expression: format!("{}(_ffi_ret)", symbols.boxer),
                })
            }
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::String,
                shape: native::BufferShape::Buffer,
                ..
            } => Ok(Self {
                declaration: "FfiBuf_u8 _ffi_ret = CALL;".to_owned(),
                expression: "boltffi_ruby_decode_string(_ffi_ret)".to_owned(),
            }),
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::Bytes,
                shape: native::BufferShape::Buffer,
                ..
            } => Ok(Self {
                declaration: "FfiBuf_u8 _ffi_ret = CALL;".to_owned(),
                expression: "boltffi_ruby_decode_bytes(_ffi_ret)".to_owned(),
            }),
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::Record(record),
                shape: native::BufferShape::Buffer,
                ..
            } => {
                ensure_record_return_supported(*record, context)?;
                let symbols = RecordSymbols::from_record_id(*record, bridge, context)?;
                let decoder = symbols.decoder.ok_or(Error::UnsupportedTarget {
                    target: "ruby",
                    shape: "direct record reached encoded decoder",
                })?;
                Ok(Self {
                    declaration: "FfiBuf_u8 _ffi_ret = CALL;".to_owned(),
                    expression: format!("{decoder}(_ffi_ret)"),
                })
            }
            ReturnPlan::NativeOpaqueRecord { record } => {
                let symbols = RecordSymbols::from_record_id(*record, bridge, context)?;
                Ok(Self {
                    declaration: "void *_ffi_ret = CALL;".to_owned(),
                    expression: format!("{}(_ffi_ret)", symbols.boxer),
                })
            }
            _ => Err(Error::UnsupportedTarget {
                target: "ruby",
                shape: "unsupported return",
            }),
        }
    }

    fn into_parts(self) -> (String, String) {
        (self.declaration, self.expression)
    }
}

fn ensure_record_return_supported(
    record: boltffi_binding::RecordId,
    context: &RenderContext<Native>,
) -> Result<()> {
    match context.record(record) {
        Some(RecordDecl::Direct(_)) => Ok(()),
        Some(RecordDecl::Encoded(record)) => {
            if encoded_record_supported(record) {
                Ok(())
            } else {
                Err(Error::UnsupportedTarget {
                    target: "ruby",
                    shape: "unsupported return",
                })
            }
        }
        _ => Err(Error::UnsupportedTarget {
            target: "ruby",
            shape: "unsupported return",
        }),
    }
}

pub(crate) fn primitive_c_type(primitive: Primitive) -> Result<&'static str> {
    Ok(match primitive {
        Primitive::Bool => "int",
        Primitive::I8 => "int8_t",
        Primitive::U8 => "uint8_t",
        Primitive::I16 => "int16_t",
        Primitive::U16 => "uint16_t",
        Primitive::I32 => "int32_t",
        Primitive::U32 => "uint32_t",
        Primitive::I64 => "int64_t",
        Primitive::U64 => "uint64_t",
        Primitive::ISize => "intptr_t",
        Primitive::USize => "uintptr_t",
        Primitive::F32 => "float",
        Primitive::F64 => "double",
        _ => {
            return Err(Error::UnsupportedTarget {
                target: "ruby",
                shape: "unknown primitive",
            });
        }
    })
}

pub(crate) fn primitive_parse(primitive: Primitive) -> &'static str {
    match primitive {
        Primitive::Bool => "boltffi_ruby_bool",
        Primitive::I64 | Primitive::ISize => "NUM2LL",
        Primitive::U64 | Primitive::USize => "NUM2ULL",
        Primitive::F32 | Primitive::F64 => "NUM2DBL",
        Primitive::U8 | Primitive::U16 | Primitive::U32 => "NUM2UINT",
        _ => "NUM2INT",
    }
}

/// C statements that parse the Ruby value read by `accessor` into a primitive
/// local named `name`.
///
/// Narrow integers (`i8`/`i16`/`u8`/`u16`) are parsed into a wide local and
/// range-checked, raising `RangeError` instead of silently truncating. Wider
/// types already raise via `NUM2INT`/`NUM2UINT`/`NUM2LL`/`NUM2ULL`.
pub(crate) fn primitive_declarations(
    accessor: &str,
    name: &str,
    primitive: Primitive,
) -> Result<Vec<String>> {
    let c_type = primitive_c_type(primitive)?;
    match NarrowIntBounds::for_primitive(primitive) {
        Some(bounds) => {
            let condition = match bounds.lower {
                Some(lower) => format!("{name}_wide < {lower} || {name}_wide > {}", bounds.upper),
                None => format!("{name}_wide > {}", bounds.upper),
            };
            Ok(vec![
                format!(
                    "{} {name}_wide = {}({accessor});",
                    bounds.wide_type, bounds.parse
                ),
                format!(
                    "if ({condition}) rb_raise(rb_eRangeError, \"value out of range for {}\");",
                    bounds.label
                ),
                format!("{c_type} {name} = ({c_type}){name}_wide;"),
            ])
        }
        None => {
            let parse = primitive_parse(primitive);
            Ok(vec![format!("{c_type} {name} = {parse}({accessor});")])
        }
    }
}

/// Range metadata for an integer narrower than the value `NUM2*` decodes into.
struct NarrowIntBounds {
    wide_type: &'static str,
    parse: &'static str,
    lower: Option<&'static str>,
    upper: &'static str,
    label: &'static str,
}

impl NarrowIntBounds {
    fn for_primitive(primitive: Primitive) -> Option<Self> {
        match primitive {
            Primitive::I8 => Some(Self {
                wide_type: "long long",
                parse: "NUM2LL",
                lower: Some("INT8_MIN"),
                upper: "INT8_MAX",
                label: "i8",
            }),
            Primitive::I16 => Some(Self {
                wide_type: "long long",
                parse: "NUM2LL",
                lower: Some("INT16_MIN"),
                upper: "INT16_MAX",
                label: "i16",
            }),
            Primitive::U8 => Some(Self {
                wide_type: "unsigned long long",
                parse: "NUM2ULL",
                lower: None,
                upper: "UINT8_MAX",
                label: "u8",
            }),
            Primitive::U16 => Some(Self {
                wide_type: "unsigned long long",
                parse: "NUM2ULL",
                lower: None,
                upper: "UINT16_MAX",
                label: "u16",
            }),
            _ => None,
        }
    }
}

pub(crate) fn primitive_box(primitive: Primitive) -> String {
    match primitive {
        Primitive::Bool => "_ffi_ret ? Qtrue : Qfalse".to_owned(),
        Primitive::F32 | Primitive::F64 => "rb_float_new((double)_ffi_ret)".to_owned(),
        Primitive::U8 | Primitive::U16 | Primitive::U32 | Primitive::U64 | Primitive::USize => {
            "ULL2NUM((unsigned long long)_ffi_ret)".to_owned()
        }
        _ => "LL2NUM((long long)_ffi_ret)".to_owned(),
    }
}

pub(crate) fn c_type_name(ty: &Type) -> Result<String> {
    Ok(match ty {
        Type::Bool => "int".to_owned(),
        Type::Int8 => "int8_t".to_owned(),
        Type::Uint8 => "uint8_t".to_owned(),
        Type::Int16 => "int16_t".to_owned(),
        Type::Uint16 => "uint16_t".to_owned(),
        Type::Int32 => "int32_t".to_owned(),
        Type::Uint32 => "uint32_t".to_owned(),
        Type::Int64 => "int64_t".to_owned(),
        Type::Uint64 => "uint64_t".to_owned(),
        Type::Float32 => "float".to_owned(),
        Type::Float64 => "double".to_owned(),
        Type::SignedPointerWidth => "intptr_t".to_owned(),
        Type::PointerWidth => "uintptr_t".to_owned(),
        Type::CStyleEnum { repr, .. } => return c_type_name(repr),
        Type::DirectRecord(name) => name.as_str().to_owned(),
        _ => {
            return Err(Error::UnsupportedTarget {
                target: "ruby",
                shape: "unsupported C return type",
            });
        }
    })
}
