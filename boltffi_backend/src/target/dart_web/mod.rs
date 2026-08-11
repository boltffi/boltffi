mod interop;
mod name_style;
mod render;
mod syntax;

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
    pub fn new(module: impl Into<String>) -> Result<Self> {
        let module = module.into();
        // Used to build generated file/directory names (`<module>.dart`,
        // `<module>_web_loader.mjs`); anything but a plain identifier risks
        // writing outside the requested output directory.
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
    type Syntax = syntax::Syntax;

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
    fn rejects_a_fallible_callback_method() {
        // target::typescript's wasm callback adapter needs a WireResult/
        // thrown-Error encoding for a fallible callback's Err(E) case that
        // dart_web doesn't emit yet; silently dropping the error channel
        // would give Dart implementations no way to signal failure.
        let result = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&fallible_callback_bindings());
        assert!(result.is_err());
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
        // `Option<Box<dyn Fn>>`'s `None` case has nowhere to go: the
        // generated Dart type stays non-nullable and every value gets
        // wrapped/called unconditionally, so this must be reported
        // unsupported rather than making the Rust API's None case
        // unreachable from Dart.
        let result = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&nullable_closure_bindings());
        assert!(result.is_err());
    }

    // Like a record with its own #[export] impl, an enum with one rejects
    // at the scan stage (ConflictingDeclarations: the same name can't be
    // both an `enum`/`#[data]` declaration and an `#[export] impl`
    // target), so an enum with populated initializers()/methods() is not
    // reachable through supported source syntax today. The defensive
    // rejection in Enumeration::from_declaration stays in place for when
    // that changes, but is not covered by an end-to-end test here for the
    // same reason.

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
        assert!(source.contains("Future<void> init() => _boltffiReady.toDart.then((_) {});"));
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
        assert!(source.contains("int add(int arg0, int arg1)"));
        assert!(source.contains("@JS('__boltffi_demo.shout')"));
        assert!(source.contains("String shout(String arg0)"));
        assert!(source.contains("@JS('__boltffi_demo.addAsync')"));
        assert!(source.contains("addAsync(int arg0, int arg1) async"));
        assert!(source.contains(".toDart"));
        // Must match target::typescript's own reserved-word escaping
        // (prefix underscore) or this binds to a JS export that was never
        // produced.
        assert!(source.contains("@JS('__boltffi_demo._delete')"));

        // `expr` inside interop::from_js's Optional branch is a call
        // (`_boltffiExtern_maybeValue(...)`), not a variable -- it must be
        // evaluated exactly once (into a temporary), not once for the null
        // check and again for the non-null decode, or an Option-returning
        // export invokes the underlying JS/Rust call twice per call site.
        assert_eq!(
            source.matches("_boltffiExtern_maybeValue(").count(),
            2,
            "the extern call site should appear once in the declaration and \
             once in the call, not duplicated by the null check:\n{source}"
        );
        assert!(source.contains(
            "final __boltffiRaw = _boltffiExtern_maybeValue((arg0).toJS); \
             return __boltffiRaw == null ? null : (__boltffiRaw! as JSNumber).toDartInt;"
        ));
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
        assert!(source.contains("int add(int arg0, int arg1);"));
        assert!(source.contains("@JSExport()"));
        assert!(source.contains("class _AdderJSAdapter"));
        assert!(source.contains("final class AdderJsWrapper implements Adder"));
        assert!(source.contains("if (callback is AdderJsWrapper) return callback.js;"));
        assert!(source.contains("createJSInteropWrapper(_AdderJSAdapter(callback))"));
        assert!(source.contains("boltffiCallbackToJSAdder"));
        assert!(source.contains("@JS('__boltffi_demo.callAdder')"));

        // `new` is a Dart keyword, so the Dart-side method is escaped to
        // `new_` -- but target::typescript's callback invoker calls the
        // unescaped JS property name (`new`), so the adapter must export
        // under that name via a per-member @JSExport override rather than
        // just inheriting the escaped Dart method name.
        assert!(source.contains("@JSExport('new')\n  JSAny? new_(JSAny? arg0)"));
        assert!(source.contains("_js.callMethodVarArgs('new'.toJS, [(arg0).toJS])"));
    }

    #[test]
    fn renders_closures_as_wrapped_js_function_values() {
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&bindings())
            .expect("target renders");
        let source = source_of(&output);

        assert!(source.contains("int applyClosure(int Function(int) arg0, int arg1)"));
        assert!(source.contains("(JSAny? __jsArg0)"));
        assert!(source.contains("arg0((__jsArg0 as JSNumber).toDartInt)"));
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
        assert!(source.contains("Future<String> greet(String arg0);"));
        // Adapter must stay sync + convert via .toJS: @JSExport doesn't
        // turn a Future return into a real Promise on its own.
        assert!(source.contains("JSPromise<JSAny?> greet(JSAny? arg0) {"));
        assert!(source.contains("return (() async {"));
        assert!(source.contains("})().toJS;"));
        assert!(source.contains("await _impl.greet("));
        assert!(source.contains("as JSPromise<JSAny?>).toDart"));
        assert!(source.contains("@JS('__boltffi_demo.callAsyncGreeter')"));
        assert!(
            source
                .contains("Future<String> callAsyncGreeter(AsyncGreeter arg0, String arg1) async")
        );
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
        assert!(source.contains("Stream<int> values() {"));
        assert!(source.contains(
            "(js).callMethodVarArgs('values'.toJS, []) as JSObject).callMethodVarArgs('consume'.toJS,"
        ));
        assert!(source.contains("extension EventBus$messagesStream on EventBus"));
        assert!(source.contains("Stream<Message> messages() {"));
        assert!(source.contains("extension EventBus$countsStream on EventBus"));
        assert!(source.contains("Stream<int> counts() {"));
        assert!(source.contains("(js).callMethodVarArgs('counts'.toJS, [((JSAny? __boltffiItem)"));
        assert!(source.contains("getProperty('done'.toJS) as JSPromise"));
        assert!(source.contains("__boltffiCancellable?.callMethodVarArgs('cancel'.toJS, []);"));
        // A rejected `done` promise must reach the stream as an error, not
        // leave the controller open forever with an unhandled Future error.
        assert!(source.contains("onError: (Object error, StackTrace stackTrace) {"));
        assert!(source.contains("__boltffiController.addError(error, stackTrace);"));
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
        assert!(source.contains("final double x;"));
        assert!(source.contains("static Point fromJS(JSObject js)"));
        assert!(source.contains("class User"));
        assert!(source.contains("final String name;"));
        assert!(source.contains("final List<int> scores;"));
        // `extension` is a Dart contextual keyword but not JS-reserved: the
        // Dart field must be escaped (`extension_`), but the wire property
        // key read/written on the JS side must stay unescaped -- it has to
        // match target::typescript's PropertyKey, which only escapes
        // JS-reserved names.
        assert!(source.contains("final bool extension_;"));
        assert!(source.contains("result.setProperty('extension'.toJS, (extension_).toJS);"));
        assert!(
            source.contains("extension_: (js.getProperty('extension'.toJS) as JSBoolean).toDart")
        );
        assert!(!source.contains("'extension_'.toJS"));
        assert!(source.contains("class Status"));
        assert!(source.contains("static const Inactive = Status._(-1);"));
        assert!(source.contains("static const Active = Status._(1);"));
        assert!(source.contains("abstract class Filter"));
        assert!(source.contains("class Filter$None extends Filter"));
        assert!(source.contains("const Filter$None() : super._();"));
        assert!(source.contains("class Filter$ByName extends Filter"));
        assert!(source.contains("case 'ByName': return Filter$ByName("));
        assert!(source.contains("class Filter$ByRange extends Filter"));
        assert!(source.contains("final int value0;"));
        assert!(source.contains("final int value1;"));
        assert!(source.contains("result.setProperty('value0'.toJS, (value0).toJS);"));

        assert!(source.contains("Point echoPoint(Point arg0)"));
        assert!(source.contains("(arg0).toJS()"));
        assert!(source.contains("Point.fromJS("));
        assert!(source.contains("Status echoStatus(Status arg0)"));
        assert!(source.contains("Status.fromJS("));

        // A data enum's unit-variant default instantiates the variant's
        // own subclass (`Filter$None()`) -- `Filter` has no static members,
        // so `Filter.None` (the C-style-enum syntax) would not analyze.
        assert!(source.contains("Filter DEFAULT_FILTER = Filter$None();"));
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
        assert!(source.contains("Timestamp keepTimestamp(Timestamp arg0)"));
        // dart:js_interop has no BigInt.toJS/JSBigInt.toDartInt -- must
        // round-trip through the boltffiInt64ToJS/FromJS helpers instead.
        assert!(source.contains("boltffiInt64ToJS(arg0)"));
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

        assert!(source.contains("final bool ENABLED = true;"));
        assert!(source.contains("final int ANSWER = 42;"));
        assert!(source.contains("final String LABEL = 'boltffi';"));
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
        assert!(source.contains("_boltffiCounterClass.callMethodVarArgs('new'.toJS,"));
        assert!(source.contains("(js).callMethodVarArgs('add'.toJS,"));
        // Async initializer: returns Future<Counter> and awaits the JS Promise.
        assert!(source.contains("static Future<Counter> connect(int arg0) async =>"));
        assert!(source.contains("as JSPromise<JSAny?>).toDart) as JSObject);"));
        // Async instance method: `async` goes after the parameter list, not
        // before the method name (`Future<int> async addAsync(...)` is
        // invalid Dart).
        assert!(source.contains("Future<int> addAsync(int arg0) async =>"));
        assert!(!source.contains("async addAsync"));
        // Without this, a class handle can only ever be released by
        // nondeterministic JS finalization -- there's no way to call the
        // JS wrapper's BoltFFIHandle.dispose() from Dart at all.
        assert!(source.contains("void dispose$() {"));
        assert!(source.contains("js.callMethodVarArgs('dispose'.toJS, []);"));
    }

    #[test]
    fn renders_an_optional_class_return_as_a_nullable_dart_type() {
        // `Option<Self>` crosses as a nullable handle -- the wrapped call
        // can return JS `null` -- so the generated static method must stay
        // nullable (`Counter?`) and null-check before decoding, instead of
        // force-casting a possibly-null result.
        let output = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&class_bindings())
            .expect("target renders");
        let source = source_of(&output);
        assert!(source.contains("static Counter? tryNew(int arg0) =>"));
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
        // dart:js_interop's JSBigInt has no int/BigInt conversion members
        // (no BigInt.toJS, no JSBigInt.toDartInt), which Duration's own
        // helpers also depend on -- verified against a real Dart SDK that
        // `BigInt.toJS`/`JSBigInt.toDartInt` don't analyze, and that this
        // constructor/toString round-trip does.
        assert!(source.contains("JSAny boltffiInt64ToJS(int value) {"));
        assert!(source.contains("int boltffiInt64FromJS(JSAny value) {"));
        assert!(source.contains("globalContext.getProperty('BigInt'.toJS)"));
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
    fn rejects_a_direct_vector_of_non_bool_primitives() {
        // target::typescript crosses this as a typed array (Int32Array),
        // not a plain JS array; dart_web doesn't emit typed-array
        // conversions yet, so this must be reported unsupported rather
        // than silently emitting a broken JSArray cast.
        let result = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&direct_vector_bindings());
        assert!(result.is_err());
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
        // Silently emitting an empty declaration would report full
        // coverage while dropping the constant from the generated API;
        // an associated constant has no owning JS object member to bind
        // to here, so this must surface as unsupported instead.
        let result = DartWebHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&associated_constant_bindings());
        assert!(result.is_err());
    }

    // A record with its own #[export] impl currently rejects at the scan
    // stage (ConflictingDeclarations: the same name can't be both a
    // `#[data]` record and an `#[export] impl` target), so a record with
    // populated initializers()/methods() is not reachable through
    // supported source syntax today. The defensive rejection in
    // Record::from_declaration stays in place for when that changes, but
    // is not covered by an end-to-end test here for the same reason.
}
