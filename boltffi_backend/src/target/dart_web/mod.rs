mod interop;
mod name_style;
mod render;

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    RecordDecl, StreamDecl, Wasm32,
};

use crate::{
    bridge::wasm::{WasmBridge, WasmBridgeContract},
    core::{
        BindingCapability, BridgeCapability, CapabilityRequirements, Emitted, Error,
        GeneratedOutput, HostCapabilities, RenderContext, RenderedDeclaration, Result, Target,
        contract::sealed, host,
    },
};

use render::{Callback, Class, Constant, CustomType, Enumeration, Function, Record, Stream};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct DartWebHost {
    module: String,
}

impl DartWebHost {
    /// JavaScript support required by the generated Dart web bindings.
    pub fn js_support() -> &'static str {
        include_str!("../../../templates/target/dart_web/support.js")
    }

    pub fn new(module: impl Into<String>) -> Result<Self> {
        let module = module.into();
        let is_valid_identifier = !module.is_empty()
            && module
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
            && module
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        if !is_valid_identifier {
            return Err(Error::UnsupportedTarget {
                target: "dart_web",
                shape: "module name must be a non-empty identifier of ASCII letters, digits, and underscores, not starting with a digit",
            });
        }
        Ok(Self { module })
    }

    pub fn into_target(self) -> Target<Self, WasmBridge> {
        Target::new(self, WasmBridge)
    }

    pub fn js_namespace(&self) -> String {
        format!("__boltffi_{}", self.module)
    }
}

impl host::HostBackend for DartWebHost {
    type Surface = Wasm32;
    type Bridge = WasmBridgeContract;
    type Syntax = crate::target::dart::syntax::Syntax;

    fn name(&self) -> &'static str {
        "dart_web"
    }

    fn binding_capabilities(&self) -> HostCapabilities {
        HostCapabilities::new()
            .stable(BindingCapability::Records)
            .stable(BindingCapability::Enums)
            .stable(BindingCapability::Functions)
            .stable(BindingCapability::Classes)
            .stable(BindingCapability::Callbacks)
            .stable(BindingCapability::Streams)
            .stable(BindingCapability::Constants)
            .stable(BindingCapability::CustomTypes)
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        CapabilityRequirements::new().require(BridgeCapability::Wasm)
    }

    fn record(
        &self,
        decl: &RecordDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Record::from_declaration(decl, context)?.render()
    }

    fn enumeration(
        &self,
        decl: &EnumDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Enumeration::from_declaration(decl, context)?.render()
    }

    fn function(
        &self,
        decl: &FunctionDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Function::from_declaration(decl, context, &self.js_namespace())?.render()
    }

    fn class(
        &self,
        decl: &ClassDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Class::from_declaration(decl, context, &self.js_namespace())?.render()
    }

    fn callback(
        &self,
        decl: &CallbackDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Callback::from_declaration(decl, context)?.render()
    }

    fn stream(
        &self,
        decl: &StreamDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Stream::from_declaration(decl, context, &self.js_namespace())?.render()
    }

    fn constant(
        &self,
        decl: &ConstantDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Constant::from_declaration(decl, context)?.render()
    }

    fn custom_type(
        &self,
        decl: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        CustomType::from_declaration(decl, context)?.render()
    }

    fn assemble<'decl>(
        &self,
        _bindings: &Bindings<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
        declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        render::Module::new(&self.module, &self.js_namespace()).render(declarations)
    }
}

impl sealed::HostBackend for DartWebHost {}

#[cfg(test)]
mod tests {
    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Bindings, Wasm32, lower};

    use super::DartWebHost;

    #[test]
    fn rejects_an_empty_module_name() {
        assert!(DartWebHost::new("").is_err());
    }

    #[test]
    fn rejects_a_module_name_with_path_traversal() {
        assert!(DartWebHost::new("../demo").is_err());
        assert!(DartWebHost::new("demo/../../etc").is_err());
        assert!(DartWebHost::new("demo/sub").is_err());
        assert!(DartWebHost::new("demo\\sub").is_err());
    }

    #[test]
    fn rejects_a_module_name_starting_with_a_digit() {
        assert!(DartWebHost::new("1demo").is_err());
    }

    #[test]
    fn accepts_an_underscore_prefixed_module_name() {
        assert!(DartWebHost::new("_demo").is_ok());
    }

    #[test]
    fn js_namespace_is_derived_from_the_module_name() {
        let host = DartWebHost::new("demo").expect("host constructs");
        assert_eq!(host.js_namespace(), "__boltffi_demo");
    }

    fn bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub fn add(a: i32, b: i32) -> i32 { a + b }

                #[export]
                pub fn shout(name: String) -> String { name.to_uppercase() }

                #[export]
                pub fn delete() -> i32 { 0 }

                #[export]
                pub fn init() -> i32 { 0 }

                #[export]
                pub fn maybe_value(present: bool) -> Option<i32> {
                    if present { Some(42) } else { None }
                }

                #[export]
                pub async fn add_async(a: i32, b: i32) -> i32 { a + b }

                #[export]
                pub trait Adder {
                    fn add(&self, a: i32, b: i32) -> i32;
                    fn new(&self, a: i32) -> i32;
                }

                #[export]
                pub fn call_adder(adder: Box<dyn Adder>, a: i32, b: i32) -> i32 {
                    adder.add(a, b)
                }

                #[export]
                pub fn apply_closure(callback: impl Fn(i32) -> i32, value: i32) -> i32 {
                    callback(value)
                }

                #[export]
                #[allow(async_fn_in_trait)]
                pub trait AsyncGreeter {
                    async fn greet(&self, name: String) -> String;
                }

                #[export]
                pub async fn call_async_greeter(greeter: impl AsyncGreeter, name: String) -> String {
                    greeter.greet(name).await
                }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    fn fallible_callback_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub trait Validator {
                    fn validate(&self, value: i32) -> Result<i32, String>;
                }

                #[export]
                pub fn call_validator(validator: Box<dyn Validator>, value: i32) -> i32 {
                    validator.validate(value).unwrap_or(0)
                }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    #[test]
    fn renders_a_fallible_callback_method_via_wire_result() {
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&fallible_callback_bindings())
            .expect("target renders");
        let source = source_of(&output);

        assert!(source.contains("abstract interface class Validator"));
        assert!(source.contains("int validate(int value);"));

        // Adapter: Dart implementation's throw becomes a wire error.
        assert!(source.contains("@JSExport('validate')"));
        assert!(source.contains("try {"));
        assert!(
            source
                .contains("final __boltffiResult = _impl.validate((arg0 as JSNumber).toDartInt);")
        );
        assert!(source.contains("return boltffiWireOk((__boltffiResult).toJS);"));
        assert!(source.contains("} on $$BoltException catch (__boltffiError) {"));
        assert!(source.contains("return boltffiWireErr((__boltffiError.message).toJS);"));

        // JsWrapper escape hatch: a raw JS object's wire error becomes a
        // thrown Dart exception instead of a silently-wrong success value.
        assert!(
            source.contains(
                "final __boltffiTag = (__boltffiRaw as JSObject).getProperty('tag'.toJS);"
            )
        );
        assert!(
            source.contains(
                "if (__boltffiTag != null && (__boltffiTag as JSString).toDart == 'err') {"
            )
        );
        assert!(source.contains(
            "throw $$BoltException(((__boltffiRaw as JSObject).getProperty('error'.toJS) as JSString).toDart);"
        ));
    }

    fn fallible_async_record_error_callback_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[data]
                pub struct ValidationError {
                    pub reason: String,
                }

                #[export]
                pub trait AsyncValidator {
                    async fn validate(&self, value: i32) -> Result<i32, ValidationError>;
                }

                #[export]
                pub async fn call_async_validator(validator: Box<dyn AsyncValidator>, value: i32) -> i32 {
                    validator.validate(value).await.unwrap_or(0)
                }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    #[test]
    fn renders_an_async_fallible_callback_method_with_a_record_error() {
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&fallible_async_record_error_callback_bindings())
            .expect("target renders");
        let source = source_of(&output);

        assert!(source.contains("class ValidationError implements Exception"));
        assert!(source.contains("Future<int> validate(int value);"));
        assert!(source.contains("JSPromise<JSAny?> validate(JSAny? arg0) {"));
        assert!(source.contains("on ValidationError catch (__boltffiError) {"));
    }

    fn nullable_closure_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub fn apply_optional_closure(
                    callback: Option<Box<dyn Fn(i32) -> i32>>,
                    value: i32,
                ) -> i32 {
                    callback.map(|f| f(value)).unwrap_or(value)
                }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    #[test]
    fn rejects_a_nullable_closure_parameter() {
        let result = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&nullable_closure_bindings());
        assert!(result.is_err());
    }

    fn record_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[data]
                #[repr(C)]
                pub struct Point {
                    pub x: f64,
                    pub y: f64,
                }

                #[data]
                pub struct User {
                    pub name: String,
                    pub scores: Vec<i32>,
                    pub extension: bool,
                }

                #[data]
                #[repr(i8)]
                pub enum Status {
                    Inactive = -1,
                    Active = 1,
                }

                #[data]
                pub enum Filter {
                    None,
                    ByName { name: String },
                    ByRange(i32, i32),
                }

                #[export]
                pub fn echo_point(value: Point) -> Point { value }

                #[export]
                pub fn echo_user(value: User) -> User { value }

                #[export]
                pub fn echo_status(value: Status) -> Status { value }

                #[export]
                pub fn echo_filter(value: Filter) -> Filter { value }

                #[export]
                pub const DEFAULT_FILTER: Filter = Filter::None;
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    fn stream_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                use std::sync::Arc;
                use boltffi::EventSubscription;

                #[data]
                pub struct Message {
                    pub text: String,
                }

                pub struct EventBus;

                #[export]
                impl EventBus {
                    pub fn new() -> Self { Self }

                    #[ffi_stream(item = i32)]
                    pub fn values(&self) -> Arc<EventSubscription<i32>> { todo!() }

                    #[ffi_stream(item = Message, mode = "batch")]
                    pub fn messages(&self) -> Arc<EventSubscription<Message>> { todo!() }

                    #[ffi_stream(item = i32, mode = "callback")]
                    pub fn counts(&self) -> Arc<EventSubscription<i32>> { todo!() }
                }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    fn constant_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub const ENABLED: bool = true;

                #[export]
                pub const ANSWER: u32 = 42;

                #[export]
                pub const LABEL: &str = "boltffi";
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    fn class_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                pub struct Counter(i32);

                #[export]
                impl Counter {
                    pub fn new(initial: i32) -> Self { Self(initial) }

                    pub async fn connect(initial: i32) -> Self { Self(initial) }

                    pub fn try_new(initial: i32) -> Option<Self> {
                        if initial < 0 { None } else { Some(Self(initial)) }
                    }

                    pub fn get(&self) -> i32 { self.0 }

                    pub fn add(&self, amount: i32) -> i32 { self.0 + amount }

                    pub async fn add_async(&self, amount: i32) -> i32 { self.0 + amount }
                }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    fn source_of(output: &crate::core::GeneratedOutput) -> String {
        output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.dart"))
            .expect("dart module")
            .contents()
            .to_owned()
    }

    #[test]
    fn renders_an_init_gate_bound_to_the_pack_step_loader_global() {
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&bindings())
            .expect("target renders");
        let source = source_of(&output);

        assert!(source.contains("@JS('__boltffi_demo_ready')"));
        assert!(source.contains("external JSPromise<JSAny?> get _boltffiReady;"));
        assert!(
            source.contains("Future<void> boltffiInit() => _boltffiReady.toDart.then((_) {});")
        );
    }

    #[test]
    fn does_not_collide_with_an_exported_function_named_init() {
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&bindings())
            .expect("target renders");
        let source = source_of(&output);

        assert!(source.contains("Future<void> boltffiInit() =>"));
        assert!(source.contains("int init() {"));
        assert_eq!(source.matches("int init()").count(), 1);
    }

    #[test]
    fn renders_free_functions_calling_the_wrapped_js_module() {
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&bindings())
            .expect("target renders");
        let source = source_of(&output);

        assert!(source.contains("@JS('__boltffi_demo.add')"));
        assert!(source.contains("int add(int a, int b)"));
        assert!(source.contains("@JS('__boltffi_demo.shout')"));
        assert!(source.contains("String shout(String name)"));
        assert!(source.contains("@JS('__boltffi_demo.addAsync')"));
        assert!(source.contains(
            "addAsync(int a, int b, { $$BoltCancellationToken? cancellationToken }) async"
        ));
        assert!(source.contains(".toDart"));
        assert!(source.contains("@JS('__boltffi_demo._delete')"));

        assert_eq!(
            source.matches("_boltffiExtern_maybeValue(").count(),
            2,
            "the extern call site should appear once in the declaration and \
             once in the call, not duplicated by the null check:\n{source}"
        );
        assert!(source.contains(
            "final __boltffiRaw = _boltffiExtern_maybeValue((present).toJS); \
             return __boltffiRaw == null ? null : (__boltffiRaw! as JSNumber).toDartInt;"
        ));
    }

    fn cancellation_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub async fn ping() {}

                #[export]
                pub fn add(a: i32, b: i32) -> i32 { a + b }

                pub struct Counter(i32);

                #[export]
                impl Counter {
                    pub async fn connect() -> Self { Self(0) }

                    pub async fn tick(&self) -> i32 { self.0 }
                }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    #[test]
    fn defines_the_cancellation_token_in_the_preamble() {
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&cancellation_bindings())
            .expect("target renders");
        let source = source_of(&output);

        assert!(source.contains("final class $$BoltCancellationToken {"));
        assert!(source.contains("bool get isCancelled => _isCancelled;"));
        assert!(source.contains("void cancel() {"));
        assert!(source.contains("_BoltAbortController? _controller;"));
        assert!(source.contains("_controller?.abort();"));
        assert!(source.contains("final class $$BoltCancelledException implements Exception {"));
        assert!(source.contains("@JS('AbortController')"));
        assert!(source.contains("external JSObject get signal;"));
    }

    #[test]
    fn appends_a_cancellation_token_to_every_async_call_shape() {
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&cancellation_bindings())
            .expect("target renders");
        let source = source_of(&output);

        // Zero-parameter async free function: the token is the sole
        // parameter, no leading ", " from an empty parameter list.
        assert!(source.contains("ping({ $$BoltCancellationToken? cancellationToken }) async"));
        assert!(
            source.contains("_boltffiExtern_ping(_boltffiCancellationOptions(cancellationToken))")
        );
        assert!(source.contains("external JSPromise<JSAny?> _boltffiExtern_ping(JSAny? options);"));
        assert!(source.contains("'signal'.toJS, token._signal"));
        assert!(source.contains("_call_connect(_boltffiCancellationOptions(cancellationToken))"));
        assert!(source.contains("_call_tick(_boltffiCancellationOptions(cancellationToken))"));
        assert!(source.contains("if (cancellationToken?.isCancelled ?? false)"));
        assert!(source.contains("_boltffiCancellationOptions(cancellationToken)"));
        assert!(!source.contains("_activeCallIds"));

        // A sync free function must not gain a token it can never use.
        assert!(source.contains("int add(int a, int b)"));
        assert!(!source.contains("int add(int arg0, int arg1, {"));

        // Async initializer, rendered as a static method.
        assert!(source.contains(
            "static Future<Counter> connect({ $$BoltCancellationToken? cancellationToken }) async {"
        ));

        // Async instance method.
        assert!(
            source.contains(
                "Future<int> tick({ $$BoltCancellationToken? cancellationToken }) async {"
            )
        );
    }

    #[test]
    fn async_optional_returns_await_outside_the_decoder_closure() {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub async fn maybe_value(flag: bool) -> Option<i32> { None }

                pub struct Probe;

                #[export]
                impl Probe {
                    pub fn new() -> Self { Self }

                    pub async fn sense(&self) -> Option<i32> { None }
                }

                #[export]
                #[allow(async_fn_in_trait)]
                pub trait MaybeGreeter {
                    async fn maybe_greet(&self) -> Option<String>;
                }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        let bindings = lower::<Wasm32>(&source).expect("source lowers");

        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&bindings)
            .expect("target renders");
        let source = source_of(&output);

        assert!(source.contains("_boltffiCaptureAsync(_boltffiExtern_maybeValue("));
        assert!(source.contains("_boltffiCaptureAsync(_boltffiProbeClassMethods(js)._call_sense("));
        assert!(source.contains(
            "final __boltffiAwaited = (await (_MaybeGreeterJSMethods(js)._call_maybeGreet("
        ));
        assert!(source.contains("final __boltffiRaw = __boltffiAwaited;"));
        assert!(source.contains(
            "return __boltffiRaw == null ? null : (__boltffiRaw! as JSNumber).toDartInt;"
        ));
        // No `await` may remain inside the synchronous decoder IIFE.
        assert!(!source.contains("(() { final __boltffiRaw = (await"));
    }

    #[test]
    fn renders_callback_interface_adapter_and_js_wrapper_escape_hatch() {
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&bindings())
            .expect("target renders");
        let source = source_of(&output);

        assert!(source.contains("abstract interface class Adder"));
        assert!(source.contains("int add(int a, int b);"));
        assert!(source.contains("@JSExport()"));
        assert!(source.contains("class _AdderJSAdapter"));
        assert!(source.contains("final class AdderJsWrapper implements Adder"));
        assert!(source.contains("if (callback is AdderJsWrapper) return callback.js;"));
        assert!(source.contains("createJSInteropWrapper(_AdderJSAdapter(callback))"));
        assert!(source.contains("boltffiCallbackToJSAdder"));
        assert!(source.contains("@JS('__boltffi_demo.callAdder')"));

        assert!(source.contains("@JSExport('new')\n  JSAny? $new(JSAny? arg0)"));
        assert!(source.contains("_AdderJSMethods(js)._call_new((a).toJS)"));
    }

    #[test]
    fn renders_closures_as_wrapped_js_function_values() {
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&bindings())
            .expect("target renders");
        let source = source_of(&output);

        assert!(source.contains("int applyClosure(int Function(int) callback, int value)"));
        assert!(source.contains("(JSAny? __jsArg0)"));
        assert!(source.contains("callback((__jsArg0 as JSNumber).toDartInt)"));
        assert!(source.contains(".toJS,"));
    }

    #[test]
    fn renders_async_callback_methods_with_a_real_js_promise() {
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&bindings())
            .expect("target renders");
        let source = source_of(&output);

        assert!(source.contains("abstract interface class AsyncGreeter"));
        assert!(source.contains("Future<String> greet(String name);"));
        // Adapter must stay sync + convert via .toJS: @JSExport doesn't
        // turn a Future return into a real Promise on its own.
        assert!(source.contains("JSPromise<JSAny?> greet(JSAny? arg0) {"));
        assert!(source.contains("return (() async {"));
        assert!(source.contains("})().toJS;"));
        assert!(source.contains("await _impl.greet("));
        assert!(source.contains("as JSPromise<JSAny?>).toDart"));
        assert!(source.contains("@JS('__boltffi_demo.callAsyncGreeter')"));
        assert!(source.contains(
            "Future<String> callAsyncGreeter(AsyncGreeter greeter, String name, \
             { $$BoltCancellationToken? cancellationToken }) async"
        ));
    }

    #[test]
    fn renders_streams_as_extension_methods_returning_dart_streams() {
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&stream_bindings())
            .expect("target renders");
        let source = source_of(&output);

        assert!(source.contains("extension EventBus$valuesStream on EventBus"));
        assert!(source.contains("Stream<int> values() =>"));
        assert!(source.contains(
            "(js).callMethodVarArgs('values'.toJS, []) as JSObject).callMethodVarArgs('consume'.toJS,"
        ));
        assert!(source.contains("extension EventBus$messagesStream on EventBus"));
        assert!(source.contains("$$BoltStreamPopBatchHandle<Message> messages() {"));
        assert!(source.contains("extension EventBus$countsStream on EventBus"));
        assert!(source.contains("StreamSubscription<int> counts(void Function(int) callback)"));
        assert!(source.contains("(js).callMethodVarArgs('counts'.toJS, [callback])"));
        assert!(source.contains("getProperty('done'.toJS) as JSPromise"));
        assert!(source.contains("subscription?.callMethodVarArgs('cancel'.toJS, []);"));
        // A rejected `done` promise must reach the stream as an error, not
        // leave the controller open forever with an unhandled Future error.
        assert!(source.contains("onError: (Object error, StackTrace stack) {"));
        assert!(source.contains("controller.addError(error, stack);"));
    }

    #[test]
    fn renders_records_and_enums_as_plain_js_object_shapes() {
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&record_bindings())
            .expect("target renders");
        let source = source_of(&output);

        assert!(source.contains("class Point"));
        assert!(source.contains("double x;"));
        assert!(!source.contains("final double x;"));
        assert!(source.contains("static Point fromJS(JSObject js)"));
        assert!(source.contains("class User"));
        assert!(source.contains("String name;"));
        assert!(source.contains("Int32List scores;"));
        assert!(source.contains("bool $extension;"));
        assert!(!source.contains("final bool $extension;"));
        // Field references are `this.`-qualified so a field named e.g.
        // `result` can't resolve to the toJS accumulator local instead.
        assert!(source.contains("result.setProperty('extension'.toJS, (this.$extension).toJS);"));
        assert!(
            source.contains("$extension: (js.getProperty('extension'.toJS) as JSBoolean).toDart")
        );
        assert!(!source.contains("'$extension'.toJS"));
        // A real Dart `enum` with lowerCamelCase variants, matching
        // target::dart's own C-style enum shape exactly.
        assert!(source.contains("enum Status {"));
        assert!(source.contains("  inactive(-1),"));
        assert!(source.contains("  active(1);"));
        assert!(source.contains("final int value;"));
        assert!(source.contains("const Status(this.value);"));
        assert!(source.contains("sealed class Filter"));
        assert!(!source.contains("abstract class Filter"));
        // `const factory` -- app code invoking `const Filter.none()` etc.
        // in a constant context must keep compiling.
        assert!(source.contains("const factory Filter.none() = Filter$None;"));
        assert!(
            source.contains("const factory Filter.byName({required String name}) = Filter$ByName;")
        );
        assert!(source.contains(
            "const factory Filter.byRange({required int value0, required int value1}) = Filter$ByRange;"
        ));
        assert!(source.contains("class Filter$None extends Filter"));
        assert!(source.contains("const Filter$None() : super._();"));
        assert!(source.contains("class Filter$ByName extends Filter"));
        assert!(source.contains("case 'ByName': return Filter$ByName("));
        assert!(source.contains("class Filter$ByRange extends Filter"));
        assert!(source.contains("final int value0;"));
        assert!(source.contains("final int value1;"));
        assert!(source.contains("result.setProperty('value0'.toJS, (this.value0).toJS);"));

        assert!(source.contains("Point echoPoint(Point value)"));
        assert!(source.contains("(value).toJS()"));
        assert!(source.contains("Point.fromJS("));
        assert!(source.contains("Status echoStatus(Status value)"));
        assert!(source.contains("Status.fromJS("));

        assert!(source.contains("const Filter defaultFilter = Filter$None();"));
    }

    #[test]
    fn record_field_named_result_does_not_collide_with_tojs_accumulator() {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[data]
                pub struct Outcome {
                    pub result: i32,
                }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        let bindings = lower::<Wasm32>(&source).expect("source lowers");

        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&bindings)
            .expect("target renders");
        let source = source_of(&output);

        assert!(source.contains("result.setProperty('result'.toJS, (this.result).toJS);"));
        assert!(source.contains("result: (js.getProperty('result'.toJS) as JSNumber).toDartInt"));
    }

    #[test]
    fn renders_custom_type_call_sites_through_its_representation() {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                custom_type!(
                    pub Timestamp,
                    remote = TimestampRust,
                    repr = i64,
                    into_ffi = timestamp_into_ffi,
                    try_from_ffi = timestamp_from_ffi
                );

                #[export]
                pub fn keep_timestamp(value: TimestampRust) -> TimestampRust { value }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        let bindings = lower::<Wasm32>(&source).expect("source lowers");

        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&bindings)
            .expect("target renders");
        let source = source_of(&output);

        assert!(source.contains("typedef Timestamp = int;"));
        assert!(source.contains("Timestamp keepTimestamp(Timestamp value)"));
        // dart:js_interop has no BigInt.toJS/JSBigInt.toDartInt -- must
        // round-trip through the boltffiInt64ToJS/FromJS helpers instead.
        assert!(source.contains("boltffiInt64ToJS(value)"));
        assert!(source.contains("boltffiInt64FromJS("));
    }

    #[test]
    fn renders_inline_constants() {
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&constant_bindings())
            .expect("target renders");
        let source = source_of(&output);

        // Matches target::dart's own constant convention: lowerCamelCase
        // name, `const` (not `final`).
        assert!(source.contains("const bool enabled = true;"));
        assert!(source.contains("const int answer = 42;"));
        assert!(source.contains("const String label = \"boltffi\";"));
    }

    #[test]
    fn renders_classes_wrapping_the_js_class_instance() {
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&class_bindings())
            .expect("target renders");
        let source = source_of(&output);
        assert!(source.contains("@JS('__boltffi_demo.Counter')"));
        assert!(source.contains("external JSObject get _boltffiCounterClass;"));
        assert!(source.contains("class Counter"));
        assert!(source.contains("_boltffiCounterClassMethods(_boltffiCounterClass)._call_new("));
        assert!(source.contains("_boltffiCounterClassMethods(js)._call_add("));
        // Async initializer: returns Future<Counter> and awaits the JS Promise.
        assert!(source.contains(
            "static Future<Counter> connect(int initial, \
             { $$BoltCancellationToken? cancellationToken }) async {"
        ));
        assert!(source.contains("as JSPromise<JSAny?>).toDart, null)) as JSObject);"));
        assert!(source.contains(
            "Future<int> addAsync(int amount, { $$BoltCancellationToken? cancellationToken }) async {"
        ));
        assert!(!source.contains("async addAsync"));
        assert!(source.contains("void dispose$() {"));
        assert!(source.contains("_boltffiCounterClassMethods(js).dispose();"));
    }

    #[test]
    fn renders_an_optional_class_return_as_a_nullable_dart_type() {
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&class_bindings())
            .expect("target renders");
        let source = source_of(&output);
        assert!(source.contains("static Counter? tryNew(int initial)"));
        assert!(source.contains("== null ? null :"));
        assert!(source.contains("Counter.fromJS(__boltffiRaw! as JSObject)"));
        assert!(!source.contains("static Counter tryNew"));
    }

    #[test]
    fn defines_duration_conversion_helpers_in_the_preamble() {
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&bindings())
            .expect("target renders");
        let source = source_of(&output);
        assert!(source.contains("JSObject boltffiDurationToJS(Duration value) {"));
        assert!(source.contains("Duration boltffiDurationFromJS(JSObject value) {"));
        assert!(source.contains("JSBigInt boltffiInt64ToJS(int value) {"));
        assert!(source.contains("int boltffiInt64FromJS(JSAny value) {"));
        assert!(source.contains("external JSBigInt _boltffiBigInt(JSNumber value);"));
        assert!(!source.contains("BigInt.from(wholeSeconds).toJS"));
        assert!(!source.contains("as JSBigInt).toDartInt"));
    }

    fn direct_vector_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub fn scores() -> Vec<i32> { vec![1, 2, 3] }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    #[test]
    fn renders_a_direct_vector_as_a_typed_array() {
        let result = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&direct_vector_bindings());
        let output = result.expect("numeric vectors render");
        assert!(source_of(&output).contains("as JSInt32Array).toDart"));
    }

    fn class_with_unsupported_method_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                pub struct Counter(i32);

                #[export]
                impl Counter {
                    pub fn new(initial: i32) -> Self { Self(initial) }

                    pub fn get(&self) -> i32 { self.0 }

                    pub fn scores(&self) -> std::collections::HashMap<i32, i32> { todo!() }

                    pub fn add(&self, amount: i32) -> i32 { self.0 + amount }
                }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    #[test]
    fn rejects_a_class_with_an_unsupported_method_under_complete_coverage() {
        let result = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&class_with_unsupported_method_bindings());
        assert!(result.is_err());
    }

    #[test]
    fn keeps_supported_class_members_when_one_method_is_unsupported_under_partial_coverage() {
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render_partial(&class_with_unsupported_method_bindings())
            .expect("target renders under partial coverage");
        let source = source_of(&output);

        // A sync initializer returning exactly `Self` renders as the
        // class's unnamed `factory` constructor, matching target::dart.
        assert!(source.contains("factory Counter(int initial)"));
        assert!(source.contains("int $get()"));
        assert!(source.contains("int add(int amount)"));
        assert!(!source.contains(" scores("));
    }

    fn associated_constant_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                pub struct Counter(i32);

                #[export]
                impl Counter {
                    pub const DEFAULT: i32 = 0;

                    pub fn new(initial: i32) -> Self { Self(initial) }
                }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    #[test]
    fn rejects_an_associated_constant() {
        let result = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&associated_constant_bindings());
        assert!(result.is_err());
    }
}
