use crate::ir::abi::SpanLengthUnit;
use crate::ir::ids::CallbackId;

#[derive(Clone)]
pub struct JniModule {
    pub prefix: String,
    pub jni_prefix: String,
    pub package_path: String,
    pub module_name: String,
    pub header_include: String,
    pub class_name: String,
    pub has_async: bool,
    pub has_async_runtime: bool,
    pub has_async_callbacks: bool,
    pub functions: Vec<JniFunction>,
    pub wire_functions: Vec<JniWireFunction>,
    pub async_functions: Vec<JniAsyncFunction>,
    pub classes: Vec<JniClass>,
    pub callback_traits: Vec<JniCallbackTrait>,
    pub async_callback_invokers: Vec<JniAsyncCallbackInvoker>,
    pub closure_trampolines: Vec<JniClosureTrampoline>,
}

#[derive(Clone)]
pub struct JniFunction {
    pub ffi_name: String,
    pub jni_name: String,
    pub return_info: JniFunctionReturn,
    pub jni_params: String,
    pub params: Vec<JniParam>,
}

#[derive(Clone)]
pub struct JniFunctionReturn {
    pub jni_return: String,
    pub is_void: bool,
}

impl JniFunctionReturn {
    pub fn is_void(&self) -> bool {
        self.is_void
    }
}

#[derive(Clone)]
pub struct JniClass {
    pub ffi_prefix: String,
    pub jni_ffi_prefix: String,
    pub jni_prefix: String,
    pub ctors: Vec<JniWireCtor>,
    pub wire_methods: Vec<JniWireMethod>,
    pub async_methods: Vec<JniAsyncFunction>,
    pub streams: Vec<JniStream>,
}

#[derive(Clone)]
pub struct JniAsyncFunction {
    pub ffi_name: String,
    pub ffi_poll: String,
    pub ffi_complete: String,
    pub ffi_cancel: String,
    pub ffi_free: String,
    pub jni_create_name: String,
    pub jni_params: String,
    pub jni_poll_name: String,
    pub jni_complete_name: String,
    pub jni_cancel_name: String,
    pub jni_free_name: String,
    pub complete_kind: JniAsyncCompleteKind,
    pub params: Vec<JniParam>,
}

#[derive(Clone)]
pub enum JniAsyncCompleteKind {
    Void,
    WireEncoded,
    BlittableStruct {
        c_type: String,
    },
    Direct {
        jni_return: String,
        c_type: String,
        return_expr: String,
    },
}

impl JniAsyncCompleteKind {
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_wire_encoded(&self) -> bool {
        matches!(self, Self::WireEncoded)
    }

    pub fn is_blittable_struct(&self) -> bool {
        matches!(self, Self::BlittableStruct { .. })
    }

    pub fn jni_return(&self) -> &str {
        match self {
            Self::Void => "void",
            Self::WireEncoded | Self::BlittableStruct { .. } => "jbyteArray",
            Self::Direct { jni_return, .. } => jni_return,
        }
    }

    pub fn c_type(&self) -> &str {
        match self {
            Self::BlittableStruct { c_type } => c_type,
            Self::Direct { c_type, .. } => c_type,
            _ => "",
        }
    }

    pub fn return_expr(&self) -> &str {
        match self {
            Self::Direct { return_expr, .. } => return_expr,
            _ => "",
        }
    }
}

#[derive(Clone)]
pub struct JniStream {
    pub subscribe_ffi: String,
    pub subscribe_jni: String,
    pub poll_ffi: String,
    pub poll_jni: String,
    pub pop_batch_ffi: String,
    pub pop_batch_jni: String,
    pub pop_batch_direct_item_c_type: Option<String>,
    pub pop_batch_direct_item_size: Option<usize>,
    pub wait_ffi: String,
    pub wait_jni: String,
    pub unsubscribe_ffi: String,
    pub unsubscribe_jni: String,
    pub free_ffi: String,
    pub free_jni: String,
}

impl JniStream {
    pub fn pop_batch_is_direct(&self) -> bool {
        self.pop_batch_direct_item_c_type.is_some() && self.pop_batch_direct_item_size.is_some()
    }

    pub fn pop_batch_direct_item_c_type(&self) -> &str {
        self.pop_batch_direct_item_c_type.as_deref().unwrap_or("")
    }

    pub fn pop_batch_direct_item_size(&self) -> usize {
        self.pop_batch_direct_item_size.unwrap_or(0)
    }
}

#[derive(Clone)]
pub struct JniCallbackTrait {
    pub trait_name: String,
    pub vtable_type: String,
    pub register_fn: String,
    pub create_fn: String,
    pub callbacks_class: String,
    pub proxy_clone_name: String,
    pub proxy_clone_jni_name: String,
    pub proxy_release_name: String,
    pub proxy_release_jni_name: String,
    pub sync_methods: Vec<JniCallbackMethod>,
    pub async_methods: Vec<JniAsyncCallbackMethod>,
    pub proxy_sync_methods: Vec<JniCallbackProxySyncMethod>,
    pub proxy_async_methods: Vec<JniCallbackProxyAsyncMethod>,
}

#[derive(Clone)]
pub struct JniAsyncCallbackMethod {
    pub ffi_name: String,
    pub jni_method_name: String,
    pub jni_signature: String,
    pub c_params: Vec<JniCallbackCParam>,
    pub setup_lines: Vec<String>,
    pub cleanup_lines: Vec<String>,
    pub jni_args: Vec<String>,
    pub return_c_type: Option<String>,
    pub invoker_jni_name: String,
    pub invoker_native_name: String,
}

impl JniAsyncCallbackMethod {
    pub fn has_return(&self) -> bool {
        self.return_c_type.is_some()
    }

    pub fn is_wire(&self) -> bool {
        self.return_c_type.as_deref() == Some("wire")
    }

    pub fn return_c_type(&self) -> &str {
        self.return_c_type.as_deref().unwrap_or("")
    }
}

#[derive(Clone)]
pub struct JniCallbackMethod {
    pub ffi_name: String,
    pub jni_method_name: String,
    pub jni_signature: String,
    pub c_params: Vec<JniCallbackCParam>,
    pub setup_lines: Vec<String>,
    pub cleanup_lines: Vec<String>,
    pub jni_args: Vec<String>,
    pub return_info: Option<JniCallbackReturn>,
}

#[derive(Clone)]
pub struct JniCallbackReturn {
    pub jni_type: String,
    pub jni_call_type: String,
    pub c_type: String,
    pub is_wire_encoded: bool,
    pub out_ptr_name: Option<String>,
    pub out_len_name: Option<String>,
}

impl JniCallbackMethod {
    pub fn status_param_name(&self) -> &str {
        "_out_status"
    }

    pub fn has_return(&self) -> bool {
        self.return_info.is_some()
    }

    pub fn jni_return_type(&self) -> &str {
        self.return_info
            .as_ref()
            .map(|r| r.jni_type.as_str())
            .unwrap_or("")
    }

    pub fn jni_call_type(&self) -> &str {
        self.return_info
            .as_ref()
            .map(|r| r.jni_call_type.as_str())
            .unwrap_or("")
    }

    pub fn c_return_type(&self) -> &str {
        self.return_info
            .as_ref()
            .map(|r| r.c_type.as_str())
            .unwrap_or("")
    }

    pub fn is_wire_encoded_return(&self) -> bool {
        self.return_info
            .as_ref()
            .map(|r| r.is_wire_encoded)
            .unwrap_or(false)
    }

    pub fn out_ptr_name(&self) -> &str {
        self.return_info
            .as_ref()
            .and_then(|r| r.out_ptr_name.as_deref())
            .unwrap_or("")
    }

    pub fn out_len_name(&self) -> &str {
        self.return_info
            .as_ref()
            .and_then(|r| r.out_len_name.as_deref())
            .unwrap_or("")
    }
}

#[derive(Clone)]
pub struct JniCallbackCParam {
    pub name: String,
    pub c_type: String,
}

#[derive(Clone)]
pub struct JniParam {
    pub name: String,
    pub ffi_arg: String,
    pub jni_decl: String,
    pub kind: JniParamKind,
}

#[derive(Clone)]
pub enum JniParamKind {
    Primitive,
    String,
    PrimitiveArray {
        c_type: String,
        elements_kind: JniPrimitiveArrayElementsKind,
        release_mode: JniArrayReleaseMode,
        stack_copy_max_len: Option<usize>,
    },
    Buffer {
        length: JniBufferLength,
    },
    Composite {
        c_type: String,
    },
    Closure,
}

#[derive(Clone)]
pub enum JniBufferLength {
    Bytes,
    CompositeElements { c_type: String },
}

impl JniBufferLength {
    pub fn composite(unit: SpanLengthUnit, c_type: String) -> Self {
        match unit {
            SpanLengthUnit::Elements => Self::CompositeElements { c_type },
            SpanLengthUnit::Bytes => Self::Bytes,
        }
    }

    fn expr(&self, name: &str) -> String {
        match self {
            Self::Bytes => {
                format!("(_{name}_ptr && _{name}_size > 0) ? (uintptr_t)_{name}_size : 0")
            }
            Self::CompositeElements { c_type } => format!(
                "(_{name}_ptr && _{name}_size > 0) ? (uintptr_t)(_{name}_size / (jlong)sizeof({c_type})) : 0"
            ),
        }
    }

    fn check(&self, name: &str) -> Option<String> {
        match self {
            Self::Bytes => None,
            Self::CompositeElements { c_type } => Some(format!(
                "if (_{name}_size < 0 || (_{name}_size > 0 && (_{name}_size % (jlong)sizeof({c_type})) != 0)) _boltffi_input_error = true;"
            )),
        }
    }
}

#[derive(Clone, Copy)]
pub enum JniPrimitiveArrayElementsKind {
    Boolean,
    Byte,
    Short,
    Int,
    Long,
    Float,
    Double,
}

#[derive(Clone, Copy)]
pub enum JniArrayReleaseMode {
    Commit,
    Abort,
}

impl JniArrayReleaseMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Commit => "0",
            Self::Abort => "JNI_ABORT",
        }
    }
}

impl JniPrimitiveArrayElementsKind {
    pub fn region_fn(&self) -> &'static str {
        match self {
            Self::Boolean => "GetBooleanArrayRegion",
            Self::Byte => "GetByteArrayRegion",
            Self::Short => "GetShortArrayRegion",
            Self::Int => "GetIntArrayRegion",
            Self::Long => "GetLongArrayRegion",
            Self::Float => "GetFloatArrayRegion",
            Self::Double => "GetDoubleArrayRegion",
        }
    }

    pub fn get_fn(&self) -> &'static str {
        match self {
            Self::Boolean => "GetBooleanArrayElements",
            Self::Byte => "GetByteArrayElements",
            Self::Short => "GetShortArrayElements",
            Self::Int => "GetIntArrayElements",
            Self::Long => "GetLongArrayElements",
            Self::Float => "GetFloatArrayElements",
            Self::Double => "GetDoubleArrayElements",
        }
    }

    pub fn release_fn(&self) -> &'static str {
        match self {
            Self::Boolean => "ReleaseBooleanArrayElements",
            Self::Byte => "ReleaseByteArrayElements",
            Self::Short => "ReleaseShortArrayElements",
            Self::Int => "ReleaseIntArrayElements",
            Self::Long => "ReleaseLongArrayElements",
            Self::Float => "ReleaseFloatArrayElements",
            Self::Double => "ReleaseDoubleArrayElements",
        }
    }

    pub fn ptr_type(&self) -> &'static str {
        match self {
            Self::Boolean => "jboolean*",
            Self::Byte => "jbyte*",
            Self::Short => "jshort*",
            Self::Int => "jint*",
            Self::Long => "jlong*",
            Self::Float => "jfloat*",
            Self::Double => "jdouble*",
        }
    }

    pub fn value_type(&self) -> &'static str {
        match self {
            Self::Boolean => "jboolean",
            Self::Byte => "jbyte",
            Self::Short => "jshort",
            Self::Int => "jint",
            Self::Long => "jlong",
            Self::Float => "jfloat",
            Self::Double => "jdouble",
        }
    }
}

impl JniParam {
    pub fn jni_param_decl(&self) -> &str {
        &self.jni_decl
    }

    pub fn ffi_arg(&self) -> &str {
        &self.ffi_arg
    }

    pub fn is_string(&self) -> bool {
        matches!(self.kind, JniParamKind::String)
    }

    pub fn is_primitive_array(&self) -> bool {
        matches!(self.kind, JniParamKind::PrimitiveArray { .. })
    }

    pub fn is_buffer(&self) -> bool {
        matches!(self.kind, JniParamKind::Buffer { .. })
    }

    pub fn is_closure(&self) -> bool {
        matches!(self.kind, JniParamKind::Closure)
    }

    pub fn is_composite(&self) -> bool {
        matches!(self.kind, JniParamKind::Composite { .. })
    }

    pub fn buffer_len_expr(&self) -> String {
        match &self.kind {
            JniParamKind::Buffer { length } => length.expr(&self.name),
            _ => String::new(),
        }
    }

    pub fn has_buffer_len_check(&self) -> bool {
        match &self.kind {
            JniParamKind::Buffer { length } => length.check(&self.name).is_some(),
            _ => false,
        }
    }

    pub fn buffer_len_check(&self) -> String {
        match &self.kind {
            JniParamKind::Buffer { length } => length.check(&self.name).unwrap_or_default(),
            _ => String::new(),
        }
    }

    pub fn composite_c_type(&self) -> &str {
        match &self.kind {
            JniParamKind::Composite { c_type } => c_type,
            _ => "",
        }
    }

    pub fn array_c_type(&self) -> &str {
        match &self.kind {
            JniParamKind::PrimitiveArray { c_type, .. } => c_type,
            _ => "",
        }
    }

    pub fn array_release_mode(&self) -> &str {
        match &self.kind {
            JniParamKind::PrimitiveArray { release_mode, .. } => release_mode.as_str(),
            _ => "",
        }
    }

    pub fn array_has_stack_copy_fast_path(&self) -> bool {
        match &self.kind {
            JniParamKind::PrimitiveArray {
                stack_copy_max_len, ..
            } => stack_copy_max_len.is_some(),
            _ => false,
        }
    }

    pub fn array_stack_copy_max_len(&self) -> usize {
        match &self.kind {
            JniParamKind::PrimitiveArray {
                stack_copy_max_len, ..
            } => stack_copy_max_len.unwrap_or(0),
            _ => 0,
        }
    }

    pub fn array_get_elements_fn(&self) -> &str {
        match &self.kind {
            JniParamKind::PrimitiveArray { elements_kind, .. } => elements_kind.get_fn(),
            _ => "",
        }
    }

    pub fn array_get_region_fn(&self) -> &str {
        match &self.kind {
            JniParamKind::PrimitiveArray { elements_kind, .. } => elements_kind.region_fn(),
            _ => "",
        }
    }

    pub fn array_release_elements_fn(&self) -> &str {
        match &self.kind {
            JniParamKind::PrimitiveArray { elements_kind, .. } => elements_kind.release_fn(),
            _ => "",
        }
    }

    pub fn array_elements_ptr_type(&self) -> &str {
        match &self.kind {
            JniParamKind::PrimitiveArray { elements_kind, .. } => elements_kind.ptr_type(),
            _ => "",
        }
    }

    pub fn array_elements_value_type(&self) -> &str {
        match &self.kind {
            JniParamKind::PrimitiveArray { elements_kind, .. } => elements_kind.value_type(),
            _ => "",
        }
    }
}

#[derive(Clone)]
pub struct JniClosureTrampoline {
    pub trampoline_name: String,
    pub signature_id: String,
    pub vtable_type: String,
    pub handle_create_fn: String,
    pub callbacks_class_jni_path: String,
    pub supports_proxy_wrap: bool,
    pub proxy_clone_name: String,
    pub proxy_clone_jni_name: String,
    pub proxy_release_name: String,
    pub proxy_release_jni_name: String,
    pub proxy_sync_method: JniCallbackProxySyncMethod,
    pub c_params: String,
    pub setup_lines: Vec<String>,
    pub cleanup_lines: Vec<String>,
    pub jni_params_signature: String,
    pub jni_call_args: String,
    pub return_info: Option<JniClosureTrampolineReturn>,
}

#[derive(Clone)]
pub struct JniClosureTrampolineReturn {
    pub c_type: String,
    pub jni_call_method: String,
    pub jni_return_cast: String,
    pub jni_signature: String,
    pub strategy: TrampolineReturnStrategy,
}

#[derive(Clone)]
pub enum TrampolineReturnStrategy {
    WireBuffer,
    BlittableStruct { struct_size: usize },
    RawPointer,
    Direct,
    CallbackHandle { create_fn: String },
}

impl JniClosureTrampolineReturn {
    pub fn wire_encoded() -> Self {
        Self {
            c_type: "FfiBuf_u8".to_string(),
            jni_call_method: "CallStaticObjectMethod".to_string(),
            jni_return_cast: String::new(),
            jni_signature: "[B".to_string(),
            strategy: TrampolineReturnStrategy::WireBuffer,
        }
    }
}

impl JniClosureTrampoline {
    pub fn is_void(&self) -> bool {
        self.return_info.is_none()
    }

    pub fn c_return_type(&self) -> &str {
        self.return_info
            .as_ref()
            .map(|r| r.c_type.as_str())
            .unwrap_or("void")
    }

    pub fn jni_call_method(&self) -> &str {
        self.return_info
            .as_ref()
            .map(|r| r.jni_call_method.as_str())
            .unwrap_or("CallStaticVoidMethod")
    }

    pub fn jni_return_cast(&self) -> &str {
        self.return_info
            .as_ref()
            .map(|r| r.jni_return_cast.as_str())
            .unwrap_or("")
    }

    pub fn jni_return_signature(&self) -> &str {
        self.return_info
            .as_ref()
            .map(|r| r.jni_signature.as_str())
            .unwrap_or("V")
    }

    pub fn is_wire_encoded_return(&self) -> bool {
        self.return_info
            .as_ref()
            .is_some_and(|r| matches!(r.strategy, TrampolineReturnStrategy::WireBuffer))
    }

    pub fn is_blittable_struct_return(&self) -> bool {
        self.return_info
            .as_ref()
            .is_some_and(|r| matches!(r.strategy, TrampolineReturnStrategy::BlittableStruct { .. }))
    }

    pub fn is_raw_pointer_return(&self) -> bool {
        self.return_info
            .as_ref()
            .is_some_and(|r| matches!(r.strategy, TrampolineReturnStrategy::RawPointer))
    }

    pub fn is_callback_handle_return(&self) -> bool {
        self.return_info
            .as_ref()
            .is_some_and(|r| matches!(r.strategy, TrampolineReturnStrategy::CallbackHandle { .. }))
    }

    pub fn callback_create_fn(&self) -> &str {
        self.return_info
            .as_ref()
            .and_then(|r| match &r.strategy {
                TrampolineReturnStrategy::CallbackHandle { create_fn } => Some(create_fn.as_str()),
                _ => None,
            })
            .unwrap_or("")
    }
}

#[derive(Clone)]
pub struct JniAsyncCallbackInvoker {
    pub suffix: String,
    pub jni_fn_name: String,
    pub jni_failure_fn_name: String,
    pub result_type: Option<JniInvokerResult>,
}

#[derive(Clone)]
pub struct JniInvokerResult {
    pub c_type: String,
    pub jni_type: String,
    pub create_fn: Option<String>,
}

impl JniAsyncCallbackInvoker {
    pub fn has_result(&self) -> bool {
        self.result_type.is_some()
    }

    pub fn is_wire(&self) -> bool {
        self.result_type
            .as_ref()
            .map(|r| r.c_type == "wire")
            .unwrap_or(false)
    }

    pub fn c_result_type(&self) -> &str {
        self.result_type
            .as_ref()
            .map(|r| r.c_type.as_str())
            .unwrap_or("")
    }

    pub fn jni_result_type(&self) -> &str {
        self.result_type
            .as_ref()
            .map(|r| r.jni_type.as_str())
            .unwrap_or("")
    }

    pub fn result_create_fn(&self) -> &str {
        self.result_type
            .as_ref()
            .and_then(|result_type| result_type.create_fn.as_deref())
            .unwrap_or("")
    }

    pub fn is_callback_handle(&self) -> bool {
        self.result_type
            .as_ref()
            .is_some_and(|result_type| result_type.create_fn.is_some())
    }
}

#[derive(Clone)]
pub struct JniOptionView {
    pub ffi_type: String,
    pub struct_size: usize,
    pub inner_kind: JniOptionInnerKind,
}

#[derive(Clone)]
pub enum JniOptionInnerKind {
    Primitive32,
    PrimitiveLarge,
    String,
    Record,
    Enum,
    VecPrimitive,
    VecRecord,
    VecString,
    VecEnum,
}

impl JniOptionView {
    pub fn is_vec(&self) -> bool {
        matches!(
            self.inner_kind,
            JniOptionInnerKind::VecPrimitive
                | JniOptionInnerKind::VecRecord
                | JniOptionInnerKind::VecString
                | JniOptionInnerKind::VecEnum
        )
    }

    pub fn is_vec_record(&self) -> bool {
        matches!(self.inner_kind, JniOptionInnerKind::VecRecord)
    }

    pub fn is_vec_primitive(&self) -> bool {
        matches!(self.inner_kind, JniOptionInnerKind::VecPrimitive)
    }

    pub fn is_vec_string(&self) -> bool {
        matches!(self.inner_kind, JniOptionInnerKind::VecString)
    }

    pub fn is_vec_enum(&self) -> bool {
        matches!(self.inner_kind, JniOptionInnerKind::VecEnum)
    }

    pub fn is_packed(&self) -> bool {
        matches!(self.inner_kind, JniOptionInnerKind::Primitive32)
    }

    pub fn is_large_primitive(&self) -> bool {
        matches!(self.inner_kind, JniOptionInnerKind::PrimitiveLarge)
    }

    pub fn is_string(&self) -> bool {
        matches!(self.inner_kind, JniOptionInnerKind::String)
    }

    pub fn is_record(&self) -> bool {
        matches!(self.inner_kind, JniOptionInnerKind::Record)
    }

    pub fn is_enum(&self) -> bool {
        matches!(self.inner_kind, JniOptionInnerKind::Enum)
    }

    pub fn box_class(&self) -> String {
        match self.inner_kind {
            JniOptionInnerKind::Primitive32 => "java/lang/Integer".to_string(),
            JniOptionInnerKind::PrimitiveLarge => "java/lang/Long".to_string(),
            _ => "java/lang/Object".to_string(),
        }
    }

    pub fn box_signature(&self) -> String {
        match self.inner_kind {
            JniOptionInnerKind::Primitive32 => "(I)Ljava/lang/Integer;".to_string(),
            JniOptionInnerKind::PrimitiveLarge => "(J)Ljava/lang/Long;".to_string(),
            _ => "()Ljava/lang/Object;".to_string(),
        }
    }

    pub fn box_jni_type(&self) -> String {
        match self.inner_kind {
            JniOptionInnerKind::Primitive32 => "jint".to_string(),
            JniOptionInnerKind::PrimitiveLarge => "jlong".to_string(),
            _ => "jobject".to_string(),
        }
    }
}

#[derive(Clone)]
pub struct JniResultView {
    pub ok: JniResultVariant,
    pub err: JniResultVariant,
}

impl JniResultView {
    pub fn is_void(&self) -> bool {
        matches!(self.ok, JniResultVariant::Void)
    }

    pub fn is_string(&self) -> bool {
        matches!(self.ok, JniResultVariant::String)
    }

    pub fn is_primitive(&self) -> bool {
        matches!(self.ok, JniResultVariant::Primitive { .. })
    }

    pub fn primitive_c_type(&self) -> String {
        match &self.ok {
            JniResultVariant::Primitive { c_type, .. } => c_type.clone(),
            _ => String::new(),
        }
    }

    pub fn is_record(&self) -> bool {
        matches!(self.ok, JniResultVariant::Record { .. })
    }

    pub fn record_struct_size(&self) -> usize {
        match &self.ok {
            JniResultVariant::Record { struct_size, .. } => *struct_size,
            _ => 0,
        }
    }

    pub fn is_enum(&self) -> bool {
        matches!(self.ok, JniResultVariant::Enum { .. })
    }

    pub fn is_data_enum(&self) -> bool {
        matches!(self.ok, JniResultVariant::DataEnum { .. })
    }

    pub fn data_enum_struct_size(&self) -> usize {
        match &self.ok {
            JniResultVariant::DataEnum { struct_size, .. } => *struct_size,
            _ => 0,
        }
    }

    pub fn err_is_ffi_error(&self) -> bool {
        matches!(self.err, JniResultVariant::String)
    }

    pub fn err_struct_size(&self) -> usize {
        match &self.err {
            JniResultVariant::DataEnum { struct_size, .. } => *struct_size,
            JniResultVariant::String => 24,
            _ => 0,
        }
    }

    pub fn is_vec_primitive(&self) -> bool {
        matches!(self.ok, JniResultVariant::VecPrimitive { .. })
    }

    pub fn is_vec_record(&self) -> bool {
        matches!(self.ok, JniResultVariant::VecRecord { .. })
    }

    pub fn vec_primitive(&self) -> Option<&JniVecPrimitive> {
        match &self.ok {
            JniResultVariant::VecPrimitive { info, .. } => Some(info),
            _ => None,
        }
    }

    pub fn vec_record_struct_size(&self) -> usize {
        match &self.ok {
            JniResultVariant::VecRecord { struct_size, .. } => *struct_size,
            _ => 0,
        }
    }

    pub fn vec_len_fn(&self) -> String {
        match &self.ok {
            JniResultVariant::VecPrimitive { len_fn, .. } => len_fn.clone(),
            JniResultVariant::VecRecord { len_fn, .. } => len_fn.clone(),
            _ => String::new(),
        }
    }

    pub fn vec_copy_fn(&self) -> String {
        match &self.ok {
            JniResultVariant::VecPrimitive { copy_fn, .. } => copy_fn.clone(),
            JniResultVariant::VecRecord { copy_fn, .. } => copy_fn.clone(),
            _ => String::new(),
        }
    }

    pub fn ok_is_void(&self) -> bool {
        matches!(self.ok, JniResultVariant::Void)
    }

    pub fn ok_is_string(&self) -> bool {
        matches!(self.ok, JniResultVariant::String)
    }

    pub fn err_is_string(&self) -> bool {
        matches!(self.err, JniResultVariant::String)
    }

    pub fn ok_c_type(&self) -> String {
        match &self.ok {
            JniResultVariant::Primitive { c_type, .. } => c_type.clone(),
            JniResultVariant::Record { c_type, .. } => c_type.clone(),
            _ => String::new(),
        }
    }

    pub fn ok_jni_type(&self) -> String {
        match &self.ok {
            JniResultVariant::Primitive { jni_type, .. } => jni_type.clone(),
            JniResultVariant::Record { jni_type, .. } => jni_type.clone(),
            JniResultVariant::Enum { jni_type } => jni_type.clone(),
            JniResultVariant::DataEnum { jni_type, .. } => jni_type.clone(),
            _ => String::new(),
        }
    }
}

#[derive(Clone)]
pub enum JniResultVariant {
    Void,
    Primitive {
        c_type: String,
        jni_type: String,
    },
    String,
    Record {
        c_type: String,
        jni_type: String,
        struct_size: usize,
    },
    Enum {
        jni_type: String,
    },
    DataEnum {
        jni_type: String,
        struct_size: usize,
    },
    VecPrimitive {
        info: JniVecPrimitive,
        len_fn: String,
        copy_fn: String,
    },
    VecRecord {
        len_fn: String,
        copy_fn: String,
        struct_size: usize,
    },
}

#[derive(Clone)]
pub struct JniVecPrimitive {
    pub c_type_name: String,
    pub jni_array_type: String,
}

impl JniVecPrimitive {
    pub fn c_type_name(&self) -> &str {
        &self.c_type_name
    }

    pub fn jni_array_type(&self) -> &str {
        &self.jni_array_type
    }
}

#[derive(Clone)]
pub struct JniWireFunction {
    pub ffi_name: String,
    pub jni_name: String,
    pub jni_params: String,
    pub params: Vec<JniParam>,
    pub return_is_unit: bool,
    pub return_is_direct: bool,
    pub return_composite_c_type: Option<String>,
    pub jni_return_type: String,
    pub jni_c_return_type: String,
    pub jni_return_expr: String,
}

#[derive(Clone)]
pub struct JniWireMethod {
    pub ffi_name: String,
    pub jni_name: String,
    pub jni_params: String,
    pub params: Vec<JniParam>,
    pub return_is_unit: bool,
    pub return_is_direct: bool,
    pub return_composite_c_type: Option<String>,
    pub jni_return_type: String,
    pub jni_c_return_type: String,
    pub jni_return_expr: String,
    pub include_handle: bool,
}

#[derive(Clone)]
pub struct JniCallbackProxySyncMethod {
    pub vtable_field: String,
    pub native_name: String,
    pub jni_name: String,
    pub jni_params: String,
    pub params: Vec<JniParam>,
    pub return_is_unit: bool,
    pub return_is_direct: bool,
    pub return_composite_c_type: Option<String>,
    pub jni_return_type: String,
    pub jni_c_return_type: String,
    pub jni_return_expr: String,
}

#[derive(Clone)]
pub struct JniCallbackProxyAsyncMethod {
    pub vtable_field: String,
    pub native_name: String,
    pub jni_name: String,
    pub jni_params: String,
    pub params: Vec<JniParam>,
    pub return_c_type: Option<String>,
    pub success_method_name: String,
    pub success_method_id: String,
    pub success_signature: String,
    pub failure_method_name: String,
    pub failure_method_id: String,
}

impl JniCallbackProxyAsyncMethod {
    pub fn has_return(&self) -> bool {
        self.return_c_type.is_some()
    }

    pub fn is_wire(&self) -> bool {
        self.return_c_type.as_deref() == Some("wire")
    }

    pub fn return_c_type(&self) -> &str {
        self.return_c_type.as_deref().unwrap_or("")
    }
}

#[derive(Clone)]
pub struct JniWireCtor {
    pub ffi_name: String,
    pub jni_name: String,
    pub jni_params: String,
    pub params: Vec<JniParam>,
}

#[derive(Clone)]
pub struct JniClosureInfo {
    pub signature_id: String,
    pub callback_id: CallbackId,
}
