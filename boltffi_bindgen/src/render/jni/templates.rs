use std::fmt::{self, Display, Formatter};

use askama::Template;

use super::plan::{
    JniAsyncCallbackInvoker, JniAsyncFunction, JniCallbackTrait, JniClass, JniClosureTrampoline,
    JniFunction, JniModule, JniParam, JniWireCtor, JniWireFunction, JniWireMethod,
};

#[derive(Template)]
#[template(path = "jni_glue.txt", escape = "none")]
pub struct JniGlueTemplate<'a> {
    pub prefix: &'a str,
    pub jni_prefix: &'a str,
    pub package_path: &'a str,
    pub module_name: &'a str,
    pub header_include: &'a str,
    pub class_name: &'a str,
    pub has_async: bool,
    pub has_async_runtime: bool,
    pub has_async_callbacks: bool,
    pub functions: &'a [JniFunction],
    pub wire_functions: &'a [JniWireFunction],
    pub async_functions: &'a [JniAsyncFunction],
    pub classes: &'a [JniClass],
    pub callback_traits: &'a [JniCallbackTrait],
    pub async_callback_invokers: &'a [JniAsyncCallbackInvoker],
    pub closure_trampolines: &'a [JniClosureTrampoline],
}

impl<'a> JniGlueTemplate<'a> {
    pub fn new(module: &'a JniModule) -> Self {
        Self {
            prefix: module.prefix.as_str(),
            jni_prefix: module.jni_prefix.as_str(),
            package_path: module.package_path.as_str(),
            module_name: module.module_name.as_str(),
            header_include: module.header_include.as_str(),
            class_name: module.class_name.as_str(),
            has_async: module.has_async,
            has_async_runtime: module.has_async_runtime,
            has_async_callbacks: module.has_async_callbacks,
            functions: &module.functions,
            wire_functions: &module.wire_functions,
            async_functions: &module.async_functions,
            classes: &module.classes,
            callback_traits: &module.callback_traits,
            async_callback_invokers: &module.async_callback_invokers,
            closure_trampolines: &module.closure_trampolines,
        }
    }
}

#[derive(Template)]
#[template(path = "jni_wire_function.txt", escape = "none")]
pub struct JniWireFunctionTemplate<'a> {
    pub ffi_name: &'a str,
    pub jni_name: &'a str,
    pub jni_params: &'a str,
    pub params: &'a [JniParam],
    pub return_is_unit: bool,
    pub return_is_direct: bool,
    pub return_composite_c_type: &'a Option<String>,
    pub jni_return_type: &'a str,
    pub jni_c_return_type: &'a str,
    pub jni_return_expr: &'a str,
}

impl<'a> JniWireFunctionTemplate<'a> {
    pub fn new(func: &'a JniWireFunction) -> Self {
        Self {
            ffi_name: func.ffi_name.as_str(),
            jni_name: func.jni_name.as_str(),
            jni_params: func.jni_params.as_str(),
            params: &func.params,
            return_is_unit: func.return_is_unit,
            return_is_direct: func.return_is_direct,
            return_composite_c_type: &func.return_composite_c_type,
            jni_return_type: func.jni_return_type.as_str(),
            jni_c_return_type: func.jni_c_return_type.as_str(),
            jni_return_expr: func.jni_return_expr.as_str(),
        }
    }
}

impl Display for JniWireFunction {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        JniWireFunctionTemplate::new(self)
            .render()
            .map_err(|_| fmt::Error)
            .and_then(|rendered| formatter.write_str(&rendered))
    }
}

#[derive(Template)]
#[template(path = "jni_wire_method.txt", escape = "none")]
pub struct JniWireMethodTemplate<'a> {
    pub ffi_name: &'a str,
    pub jni_name: &'a str,
    pub jni_params: &'a str,
    pub params: &'a [JniParam],
    pub return_is_unit: bool,
    pub return_is_direct: bool,
    pub return_composite_c_type: &'a Option<String>,
    pub jni_return_type: &'a str,
    pub jni_c_return_type: &'a str,
    pub jni_return_expr: &'a str,
    pub include_handle: bool,
}

impl<'a> JniWireMethodTemplate<'a> {
    pub fn new(method: &'a JniWireMethod) -> Self {
        Self {
            ffi_name: method.ffi_name.as_str(),
            jni_name: method.jni_name.as_str(),
            jni_params: method.jni_params.as_str(),
            params: &method.params,
            return_is_unit: method.return_is_unit,
            return_is_direct: method.return_is_direct,
            return_composite_c_type: &method.return_composite_c_type,
            jni_return_type: method.jni_return_type.as_str(),
            jni_c_return_type: method.jni_c_return_type.as_str(),
            jni_return_expr: method.jni_return_expr.as_str(),
            include_handle: method.include_handle,
        }
    }
}

impl Display for JniWireMethod {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        JniWireMethodTemplate::new(self)
            .render()
            .map_err(|_| fmt::Error)
            .and_then(|rendered| formatter.write_str(&rendered))
    }
}

#[derive(Template)]
#[template(path = "jni_wire_ctor.txt", escape = "none")]
pub struct JniWireCtorTemplate<'a> {
    pub ffi_name: &'a str,
    pub jni_name: &'a str,
    pub jni_params: &'a str,
    pub params: &'a [JniParam],
}

impl<'a> JniWireCtorTemplate<'a> {
    pub fn new(ctor: &'a JniWireCtor) -> Self {
        Self {
            ffi_name: ctor.ffi_name.as_str(),
            jni_name: ctor.jni_name.as_str(),
            jni_params: ctor.jni_params.as_str(),
            params: &ctor.params,
        }
    }
}

impl Display for JniWireCtor {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        JniWireCtorTemplate::new(self)
            .render()
            .map_err(|_| fmt::Error)
            .and_then(|rendered| formatter.write_str(&rendered))
    }
}

#[cfg(test)]
mod tests {
    use super::JniWireFunctionTemplate;
    use crate::ir;
    use crate::model::{
        CallbackTrait, Class, ClosureSignature, Constructor, ConstructorParam, Enumeration,
        Function, Method, Module, Parameter, Primitive, Receiver, Record, RecordField, ReturnType,
        TraitMethod, TraitMethodParam, Type, Variant,
    };
    use crate::render::jni::plan::{
        JniArrayReleaseMode, JniParam, JniParamKind, JniPrimitiveArrayElementsKind, JniWireFunction,
    };
    use crate::render::jni::{JniEmitter, JniLowerer};
    use askama::Template;

    fn build_test_module() -> Module {
        let point = Record::new("Point")
            .with_field(RecordField::new("x", Type::Primitive(Primitive::F64)))
            .with_field(RecordField::new("y", Type::Primitive(Primitive::F64)));

        let message = Record::new("Message").with_field(RecordField::new("text", Type::String));

        let color = Enumeration::new("Color")
            .with_variant(Variant::new("red").with_discriminant(0))
            .with_variant(Variant::new("green").with_discriminant(1))
            .with_variant(Variant::new("blue").with_discriminant(2));

        let shape = Enumeration::new("Shape").with_variant(
            Variant::new("circle")
                .with_field(RecordField::new("radius", Type::Primitive(Primitive::F64))),
        );

        let api_error = Enumeration::new("ApiError").as_error().with_variant(
            Variant::new("failed")
                .with_field(RecordField::new("code", Type::Primitive(Primitive::I32))),
        );

        let listener = CallbackTrait::new("Listener")
            .with_method(
                TraitMethod::new("on_value")
                    .with_param(TraitMethodParam::new(
                        "value",
                        Type::Primitive(Primitive::I32),
                    ))
                    .with_return(ReturnType::Void),
            )
            .with_method(
                TraitMethod::new("on_done")
                    .with_param(TraitMethodParam::new(
                        "status",
                        Type::Primitive(Primitive::U32),
                    ))
                    .with_return(ReturnType::value(Type::String))
                    .make_async(),
            );

        let engine = Class::new("Engine")
            .with_constructor(
                Constructor::new().with_param(ConstructorParam::new("name", Type::String)),
            )
            .with_method(
                Method::new("ping", Receiver::Ref).with_output(Type::Primitive(Primitive::I32)),
            )
            .with_method(
                Method::new("fetch", Receiver::Ref)
                    .with_output(Type::String)
                    .make_async(),
            );

        let closure = ClosureSignature::single_param(Type::Record("Point".to_string()));
        let closure_function = Function::new("with_closure")
            .with_param(Parameter::new("callback", Type::Closure(closure.clone())))
            .with_return(ReturnType::Void);

        Module::new("test")
            .with_record(point)
            .with_record(message)
            .with_enum(color)
            .with_enum(shape)
            .with_enum(api_error)
            .with_callback_trait(listener)
            .with_class(engine)
            .with_function(
                Function::new("add")
                    .with_param(Parameter::new("a", Type::Primitive(Primitive::I32)))
                    .with_param(Parameter::new("b", Type::Primitive(Primitive::I32)))
                    .with_output(Type::Primitive(Primitive::I32)),
            )
            .with_function(
                Function::new("echo_point")
                    .with_param(Parameter::new("point", Type::Record("Point".to_string())))
                    .with_output(Type::Record("Point".to_string())),
            )
            .with_function(
                Function::new("echo_color")
                    .with_param(Parameter::new("color", Type::Enum("Color".to_string())))
                    .with_output(Type::Enum("Color".to_string())),
            )
            .with_function(
                Function::new("echo_shape")
                    .with_param(Parameter::new("shape", Type::Enum("Shape".to_string())))
                    .with_output(Type::Enum("Shape".to_string())),
            )
            .with_function(Function::new("fallible").with_return(ReturnType::fallible(
                Type::Primitive(Primitive::I32),
                Type::Enum("ApiError".to_string()),
            )))
            .with_function(closure_function)
    }

    fn build_status_callback_module() -> Module {
        let status_mapper = CallbackTrait::new("StatusMapper").with_method(
            TraitMethod::new("map_status")
                .with_param(TraitMethodParam::new(
                    "status",
                    Type::Primitive(Primitive::I32),
                ))
                .with_return(ReturnType::value(Type::Primitive(Primitive::I32))),
        );

        Module::new("test").with_callback_trait(status_mapper)
    }

    #[test]
    fn jni_ir_generates_valid_glue() {
        let module = build_test_module();
        let package = "com.example";
        let class_name = "BenchBoltFFI";

        let mut ir_module = module.clone();
        let contract = ir::build_contract(&mut ir_module);
        let abi_contract = ir::Lowerer::new(&contract).to_abi_contract();
        let jni_module = JniLowerer::new(
            &contract,
            &abi_contract,
            package.to_string(),
            class_name.to_string(),
        )
        .lower();
        let ir_code = JniEmitter::emit(&jni_module);

        assert!(!ir_code.is_empty());
        assert!(ir_code.contains("boltffi_free_buf"));
        assert!(ir_code.contains("jbyteArray"));
        assert!(!ir_code.contains("NewDirectByteBuffer(env, (void*)_buf.ptr"));
    }

    #[test]
    fn callback_status_out_param_does_not_collide_with_user_status_param() {
        let module = build_status_callback_module();
        let package = "com.example";
        let class_name = "BenchBoltFFI";

        let mut ir_module = module.clone();
        let contract = ir::build_contract(&mut ir_module);
        let abi_contract = ir::Lowerer::new(&contract).to_abi_contract();
        let jni_module = JniLowerer::new(
            &contract,
            &abi_contract,
            package.to_string(),
            class_name.to_string(),
        )
        .lower();
        let glue_code = JniEmitter::emit(&jni_module);

        assert!(
            glue_code.contains("StatusMapper_vtable_map_status(uint64_t handle, int32_t status"),
            "callback glue should preserve the user param `status`"
        );
        assert!(
            glue_code.contains("FfiStatus* _out_status"),
            "callback glue should rename the status out param to avoid colliding with user param `status`"
        );
        assert!(
            !glue_code.contains("FfiStatus* status)"),
            "callback glue should not reuse `status` as the out status param name"
        );
    }

    #[test]
    fn sync_only_callback_trait_does_not_look_up_future_continuation() {
        let module = build_status_callback_module();
        let mut ir_module = module.clone();
        let contract = ir::build_contract(&mut ir_module);
        let abi_contract = ir::Lowerer::new(&contract).to_abi_contract();
        let jni_module = JniLowerer::new(
            &contract,
            &abi_contract,
            "com.example".to_string(),
            "Native".to_string(),
        )
        .lower();

        assert!(
            !jni_module.has_async_runtime,
            "sync-only callback trait must not set has_async_runtime"
        );

        let glue = JniEmitter::emit(&jni_module);
        assert!(
            glue.contains("JNI_OnLoad"),
            "JNI_OnLoad must still be emitted for callback init"
        );
        assert!(
            !glue.contains("boltffiFutureContinuationCallback"),
            "must not look up Kotlin method that was never generated"
        );
    }

    #[test]
    fn android_callback_glue_caches_native_thread_attachment_with_local_frames() {
        let module = build_test_module();
        let mut ir_module = module.clone();
        let contract = ir::build_contract(&mut ir_module);
        let abi_contract = ir::Lowerer::new(&contract).to_abi_contract();
        let jni_module = JniLowerer::new(
            &contract,
            &abi_contract,
            "com.example".to_string(),
            "Native".to_string(),
        )
        .lower();

        let glue = JniEmitter::emit(&jni_module);

        assert!(
            glue.contains("pthread_key_create(&g_boltffi_jni_env_key"),
            "Android callback glue should create a pthread TLS key for cached JNI attachments"
        );
        assert!(
            glue.contains("AttachCurrentThreadAsDaemon"),
            "Android callback glue should attach native worker threads as daemon Java threads"
        );
        assert!(
            glue.contains("pthread_setspecific(g_boltffi_jni_env_key"),
            "Android callback glue should mark BoltFFI-owned thread attachments in TLS"
        );
        assert!(
            glue.contains("JNIEnv* callback_env = *env;"),
            "callback enter should convert JNIEnv** to a local JNIEnv* before calling JNI functions"
        );
        assert!(
            glue.contains(
                "(*callback_env)->PushLocalFrame(callback_env, BOLTFFI_JNI_CALLBACK_LOCAL_FRAME_CAPACITY)"
            ),
            "cached Android attachments should bound callback-local JNI references"
        );
        assert!(
            !glue.contains("(*env)->PushLocalFrame(*env"),
            "callback enter should not call JNI methods directly on the JNIEnv** parameter"
        );
        assert!(
            glue.contains("PopLocalFrame(env, NULL)"),
            "cached Android attachments should release callback-local JNI references before returning"
        );
        let exit_helper = glue
            .split("static inline void boltffi_jni_callback_exit(JNIEnv* env, int attached)")
            .nth(1)
            .expect("callback exit helper should be rendered");
        let android_exit_branch = exit_helper
            .split("#else")
            .next()
            .expect("Android callback exit branch should be rendered");
        assert!(
            android_exit_branch.contains("boltffi_consume_pending_exception(env);"),
            "cached Android attachments should not preserve pending JNI exceptions between callbacks"
        );
        assert!(
            glue.contains("boltffi_jni_callback_enter(&env, &attached)"),
            "native-to-Java callback paths should use the shared Android-aware callback entry helper"
        );
    }

    #[test]
    fn wire_template_uses_typed_array_region_for_stack_copy_fast_path() {
        let function = JniWireFunction {
            ffi_name: "boltffi_sum".to_string(),
            jni_name: "Java_com_example_Native_sum".to_string(),
            jni_params: ", jintArray values".to_string(),
            params: vec![JniParam {
                name: "values".to_string(),
                ffi_arg: "((FfiSlice_i32){ .ptr = _values_ptr, .len = _values_len })".to_string(),
                jni_decl: ", jintArray values".to_string(),
                kind: JniParamKind::PrimitiveArray {
                    c_type: "int32_t".to_string(),
                    elements_kind: JniPrimitiveArrayElementsKind::Int,
                    release_mode: JniArrayReleaseMode::Abort,
                    stack_copy_max_len: Some(8),
                },
            }],
            return_is_unit: false,
            return_is_direct: true,
            return_composite_c_type: None,
            jni_return_type: "jint".to_string(),
            jni_c_return_type: "int32_t".to_string(),
            jni_return_expr: "(jint)_result".to_string(),
        };

        let rendered = JniWireFunctionTemplate::new(&function)
            .render()
            .expect("wire function should render");

        assert!(
            rendered.contains("GetIntArrayRegion(env, values"),
            "stack-copy fast path should use the typed JNI region accessor: {rendered}"
        );
        assert!(
            !rendered.contains("GetByteArrayRegion(env, values"),
            "stack-copy fast path should not hardcode byte-array accessors: {rendered}"
        );
    }
}
