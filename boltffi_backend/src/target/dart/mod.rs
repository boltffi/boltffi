#![allow(missing_docs)]

mod codec;
mod default_value;
mod name_style;
mod native;
mod render;
mod syntax;
mod type_name;
mod value_semantics;

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    Native, RecordDecl, StreamDecl,
};

use crate::{
    bridge::c::{CBridge, CBridgeContract},
    core::{
        BindingCapability, BridgeCapability, CapabilityRequirements, Emitted, Error,
        GeneratedOutput, HostCapabilities, RenderContext, RenderedDeclaration,
        ResolvedCustomTypeMappings, Result, Target, contract::sealed, host,
    },
};

use name_style::Name;
use syntax::Syntax;

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct DartHost {
    package: Option<String>,
    artifact: Option<String>,
}

impl DartHost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn package(mut self, package: impl Into<String>) -> Self {
        self.package = Some(package.into());
        self
    }

    pub fn native_artifact(mut self, artifact: impl Into<String>) -> Self {
        self.artifact = Some(artifact.into());
        self
    }

    pub fn into_target(self) -> Result<Target<Self, CBridge>> {
        Ok(Target::new(self, CBridge::default_header()?))
    }

    fn package_for(&self, bindings: &Bindings<Native>) -> Result<String> {
        self.package
            .clone()
            .map(Ok)
            .unwrap_or_else(|| Ok(Name::new(bindings.package().name()).snake()))
    }

    fn artifact_for(&self, bindings: &Bindings<Native>) -> String {
        self.artifact
            .clone()
            .unwrap_or_else(|| Name::new(bindings.package().name()).snake())
    }
}

impl host::HostBackend for DartHost {
    type Surface = Native;
    type Bridge = CBridgeContract;
    type Syntax = Syntax;

    fn name(&self) -> &'static str {
        "dart"
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
        CapabilityRequirements::new().require(BridgeCapability::CAbi)
    }

    fn custom_type_mappings(
        &self,
        _: &Bindings<Self::Surface>,
    ) -> Result<ResolvedCustomTypeMappings> {
        Ok(ResolvedCustomTypeMappings::default())
    }

    fn record(
        &self,
        declaration: &RecordDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        render::Record::from_declaration(declaration, bridge, context).map(render::Record::render)
    }

    fn enumeration(
        &self,
        declaration: &EnumDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        render::Enumeration::from_declaration(declaration, bridge, context)
            .map(render::Enumeration::render)
    }

    fn function(
        &self,
        declaration: &FunctionDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        render::Function::from_callable(
            declaration.name(),
            declaration.symbol(),
            declaration.callable(),
            render::function::Placement::TopLevel,
            bridge,
            context,
            declaration.meta().doc(),
        )
        .map(render::Function::render)
    }

    fn class(
        &self,
        declaration: &ClassDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        render::Class::from_declaration(declaration, bridge, context).map(render::Class::render)
    }

    fn callback(
        &self,
        declaration: &CallbackDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        render::Callback::from_declaration(declaration, bridge, context)
            .map(render::Callback::render)
    }

    fn stream(
        &self,
        declaration: &StreamDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        render::Stream::from_declaration(declaration, bridge, context).map(render::Stream::render)
    }

    fn constant(
        &self,
        declaration: &ConstantDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        if declaration.owner().is_some() {
            return Ok(Emitted::primary(""));
        }
        render::Constant::from_declaration(declaration, false, bridge, context)
            .map(render::Constant::render)
    }

    fn custom_type(
        &self,
        declaration: &CustomTypeDecl,
        _: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        render::CustomType::from_declaration(declaration, context).map(render::CustomType::render)
    }

    fn assemble<'decl>(
        &self,
        bindings: &Bindings<Native>,
        bridge: &CBridgeContract,
        _: &RenderContext<Native>,
        declarations: Vec<RenderedDeclaration<'decl, Native>>,
    ) -> Result<GeneratedOutput> {
        render::Module::new(self, bridge, declarations).render(bindings)
    }
}

impl sealed::HostBackend for DartHost {}

fn unsupported<T>(shape: &'static str) -> Result<T> {
    Err(Error::UnsupportedTarget {
        target: "dart",
        shape,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Bindings, Native, lower};

    use crate::{GeneratedOutput, Target, bridge::c::CBridge};

    use super::DartHost;

    fn bindings(source: &str) -> Bindings<Native> {
        let scanned = boltffi_scan::scan_file(
            syn::parse_str(source).expect("valid Rust source"),
            PackageInfo::new("demo_api", None),
        )
        .expect("source should scan");
        lower::<Native>(&scanned).expect("source should lower to native Binding IR")
    }

    fn target(host: DartHost) -> Target<DartHost, CBridge> {
        host.into_target().expect("Dart target")
    }

    fn file(output: &GeneratedOutput, path: impl AsRef<Path>) -> &str {
        output
            .files()
            .iter()
            .find(|file| file.path().as_path() == path.as_ref())
            .map(|file| file.contents())
            .expect("generated Dart file")
    }

    #[test]
    fn dart_target_renders_primitive_functions_from_native_bindings() {
        let bindings = bindings(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 { left + right }

            #[export]
            pub fn notify(value: u64) {}
            "#,
        );
        let output = target(
            DartHost::new()
                .package("demo")
                .native_artifact("demo_native"),
        )
        .render(&bindings)
        .expect("primitive functions should render");

        let source = file(&output, "demo/lib/demo.dart");
        assert!(source.contains("int add(int left, int right)"));
        assert!(source.contains("external int _f$boltffi_function_demo_api_add"));
        assert!(source.contains("void notify(int value)"));
        assert!(source.contains("_$$BoltFFIStatus"));
        assert_eq!(source.matches("symbol: 'boltffi_free_buf'").count(), 1);
        assert!(file(&output, "demo/hook/build.dart").contains("demo_native"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn dart_target_uses_binding_ir_record_classification() {
        let bindings = bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
                pub y: i32,
            }

            #[data]
            pub struct Profile {
                pub name: String,
                pub location: Point,
            }

            #[export]
            pub fn move_point(point: Point) -> Point { point }

            #[export]
            pub fn echo_profile(profile: Profile) -> Profile { profile }
            "#,
        );
        let output = target(DartHost::new().package("demo"))
            .render(&bindings)
            .expect("classified records should render");

        let source = file(&output, "demo/lib/demo.dart");
        assert!(source.contains("final class Point"));
        assert!(source.contains("extends $$ffi.Struct"));
        assert!(source.contains("factory Point._m$fromStruct"));
        assert!(source.contains("final class Profile"));
        assert!(source.contains("Profile._m$wireDecode"));
        assert!(source.contains("profile._m$wireEncode"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn dart_target_preserves_async_api_shape() {
        let bindings = bindings(
            r#"
            #[export]
            pub async fn fetch_count(seed: i32) -> i32 { seed + 1 }
            "#,
        );
        let output = target(DartHost::new().package("demo"))
            .render(&bindings)
            .expect("async functions should render");

        let source = file(&output, "demo/lib/demo.dart");
        assert!(source.contains("Future<int> fetchCount(int seed)"));
        assert!(source.contains("_$$BoltFFIAsync.create"));
        assert!(source.contains("pollFuture:"));
        assert!(source.contains("completeFuture:"));
    }

    #[test]
    fn dart_target_renders_callbacks_from_the_typed_c_protocol() {
        let bindings = bindings(
            r#"
            #[export]
            pub trait Transformer {
                fn transform(&self, value: i32) -> i32;
                async fn load(&self, key: String) -> Option<i64>;
            }

            #[export]
            pub fn invoke(transformer: impl Transformer, value: i32) -> i32 {
                transformer.transform(value)
            }
            "#,
        );
        let output = target(DartHost::new().package("demo"))
            .render(&bindings)
            .expect("callbacks should render from the typed C callback protocol");

        let source = file(&output, "demo/lib/demo.dart");
        assert!(source.contains("abstract interface class Transformer"));
        assert!(source.contains("Future<int?> load(String key)"));
        assert!(source.contains("TransformerVTable extends $$ffi.Struct"));
        assert!(source.contains("TransformerBridge.create(transformer)"));
        assert!(source.contains("$$ffi.NativeCallable.isolateLocal"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn dart_target_renders_closure_registration_from_the_typed_c_protocol() {
        let bindings = bindings(
            r#"
            #[export]
            pub fn apply(callback: impl Fn(i32) -> i32, value: i32) -> i32 {
                callback(value)
            }

            #[export]
            pub fn map_label(callback: impl Fn(String) -> String, value: String) -> String {
                callback(value)
            }

            #[export]
            pub fn maybe_apply(
                callback: Option<Box<dyn Fn(i32) -> i32>>,
                value: i32,
            ) -> i32 {
                callback.map_or(value, |callback| callback(value))
            }

            #[error]
            pub enum MathError { InvalidInput }

            #[export]
            pub fn try_apply(
                callback: impl Fn(i32) -> Result<i32, MathError>,
                value: i32,
            ) -> Result<i32, MathError> {
                callback(value)
            }
            "#,
        );
        let output = target(DartHost::new().package("demo"))
            .render(&bindings)
            .expect("closure parameters should render from the typed C closure protocol");

        let source = file(&output, "demo/lib/demo.dart");
        assert!(source.contains("int apply(int Function(int) callback, int value)"));
        assert!(source.contains("$$ffi.NativeCallable<"));
        assert!(source.contains(".isolateLocal("));
        assert!(source.contains("callbackCall.nativeFunction"));
        assert!(source.contains("callbackRelease.nativeFunction"));
        assert!(source.contains("callbackCall.close();"));
        assert!(source.contains("callbackRelease.close();"));
        assert!(source.contains("(int Function(int))? callback"));
        assert!(source.contains("int tryApply(int Function(int) callback, int value)"));
        assert!(source.contains("on MathError catch"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn dart_target_renders_returned_closure_ownership_from_binding_ir() {
        let bindings = bindings(
            r#"
            #[export]
            pub fn make_adder(base: i32) -> impl Fn(i32) -> i32 {
                move |value| base + value
            }

            #[export]
            pub fn make_labeler(prefix: String) -> Box<dyn Fn(String) -> String> {
                Box::new(move |value| format!("{prefix}{value}"))
            }

            #[export]
            pub async fn make_async_adder(base: i32) -> Box<dyn Fn(i32) -> i32> {
                Box::new(move |value| base + value)
            }

            #[export]
            pub fn try_make_adder(
                base: i32,
            ) -> Result<Box<dyn Fn(i32) -> i32>, String> {
                Ok(Box::new(move |value| base + value))
            }

            #[export]
            pub fn make_checker() -> impl Fn(i32) -> Result<i32, String> {
                |value| Ok(value)
            }
            "#,
        );
        let output = target(DartHost::new().package("demo"))
            .render(&bindings)
            .expect("returned closures should render from the typed closure protocol");

        let source = file(&output, "demo/lib/demo.dart");
        assert!(source.contains("int Function(int) makeAdder(int $base)"));
        assert!(source.contains("String Function(String) makeLabeler(String prefix)"));
        assert!(source.contains("Future<int Function(int)> makeAsyncAdder(int $base)"));
        assert!(source.contains("int Function(int) tryMakeAdder(int $base)"));
        assert!(source.contains("int Function(int) makeChecker()"));
        assert!(source.contains("_$$BoltReturnedClosureRegistration"));
        assert!(source.contains("_$$BoltReturnedClosureOwner"));
        assert!(source.contains("returnedClosureOwner.invoke.cast<$$ffi.NativeFunction"));
        assert!(source.contains("returnedClosureOwner.context"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn dart_target_renders_scalar_options_and_direct_vectors() {
        let bindings = bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point { pub x: i32, pub y: i32 }

            #[export]
            pub fn maybe(value: Option<i64>) -> Option<i64> { value }

            #[export]
            pub fn points(values: Vec<Point>) -> Vec<Point> { values }

            #[export]
            pub fn offsets(values: Vec<isize>) -> Vec<isize> { values }
            "#,
        );
        let output = target(DartHost::new().package("demo"))
            .render(&bindings)
            .expect("classified option and direct-vector crossings should render");

        let source = file(&output, "demo/lib/demo.dart");
        assert!(source.contains("int? maybe(int? value)"));
        assert!(source.contains("List<Point> points(List<Point> values)"));
        assert!(source.contains("$$typed_data.Int64List offsets($$typed_data.Int64List values)"));
        assert!(source.contains("ptr.elementAt(_l$index).value = values[_l$index]"));
        assert!(source.contains("List<int>.generate"));
        assert!(!source.contains("cast<$$ffi.IntPtr>().asTypedList"));
        assert!(source.contains("_m$writeStruct"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn dart_target_preserves_builtin_and_map_api_types() {
        let bindings = bindings(
            r#"
            use std::collections::HashMap;
            use uuid::Uuid;

            #[export]
            pub fn echo_uuid(value: Uuid) -> Uuid { value }

            #[export]
            pub fn echo_scores(
                values: HashMap<String, i32>,
            ) -> HashMap<String, i32> {
                values
            }
            "#,
        );
        let output = target(DartHost::new().package("demo"))
            .render(&bindings)
            .expect("builtins and maps should retain their classified wire contracts");

        let source = file(&output, "demo/lib/demo.dart");
        assert!(source.contains("$$BoltUUIDValue echoUuid($$BoltUUIDValue value)"));
        assert!(source.contains("Map<String, int> echoScores(Map<String, int> values)"));
        assert!(source.contains("readMap"));
        assert!(source.contains("writeUUID"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn dart_target_preserves_nested_wire_representations() {
        let bindings = bindings(
            r#"
            use url::Url;

            #[repr(u8)]
            #[data]
            pub enum Mode { Fast = 1, Slow = 2 }

            #[repr(u64)]
            #[data]
            pub enum WideMode { Fast = 1, Slow = 2 }

            #[data]
            pub struct Request {
                pub mode: Mode,
                pub wide_mode: WideMode,
                pub endpoint: Url,
                pub result: Result<i32, String>,
            }

            #[export]
            pub fn echo_request(request: Request) -> Request { request }
            "#,
        );
        let output = target(DartHost::new().package("demo"))
            .render(&bindings)
            .expect("nested wire values should render");

        let source = file(&output, "demo/lib/demo.dart");
        assert!(source.contains("$$BoltResult<int, $$BoltException> result;"));
        assert!(source.contains("Mode._m$fromDiscriminant(_p$reader.readU8())"));
        assert!(source.contains("_p$writer.writeU8(mode.value);"));
        assert!(source.contains("WideMode._m$fromDiscriminant(_p$reader.readU64())"));
        assert!(source.contains("_p$writer.writeU64(wideMode.value);"));
        assert!(source.contains("((endpoint).toString().length * 3)"));
        assert!(source.contains("$$BoltResult.err($$BoltException(_p$reader.readString()))"));
        assert!(source.contains(".writeString(_l$boltffiValue0.message);"));
        assert!(source.contains("utf8.encode(_l$boltffiValue0.message).length"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn dart_target_preserves_model_api_and_value_semantics() {
        let bindings = bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point { pub x: i32, pub y: i32 }

            #[data(impl)]
            impl Point {
                pub fn new(x: i32, y: i32) -> Self { Self { x, y } }
            }

            #[data]
            pub enum Message {
                Ping,
                Values { items: Vec<i32> },
            }

            pub struct Counter;

            #[export]
            impl Counter {
                pub fn create() -> Self { Self }
                pub fn get(&self) -> i32 { 0 }
                pub fn dispose(&self) {}
            }

            #[export]
            pub fn echo_message(message: Message) -> Message { message }
            "#,
        );
        let output = target(DartHost::new().package("demo"))
            .render(&bindings)
            .expect("established Dart model APIs should render");

        let source = file(&output, "demo/lib/demo.dart");
        assert!(source.contains("Point $new(int x, int y)"));
        assert!(source.contains("factory Message.ping() = Message$Ping;"));
        assert!(source.contains("factory Message.values({"));
        assert!(source.contains("void dispose$()"));
        assert!(source.contains("void dispose()"));
        assert!(source.contains("int $get()"));
        assert!(source.contains("bool operator ==(Object other)"));
        assert!(source.contains("_$$BoltUtil.listCompare(items, other.items"));
        assert!(source.contains("_$$BoltUtil.listHash(items"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn dart_target_preserves_single_element_record_syntax() {
        let bindings = bindings(
            r#"
            #[export]
            pub fn echo_single(value: (i32,)) -> (i32,) { value }
            "#,
        );
        let output = target(DartHost::new().package("demo"))
            .render(&bindings)
            .expect("single-element tuples should render as Dart records");

        let source = file(&output, "demo/lib/demo.dart");
        assert!(source.contains("(int,) echoSingle((int,) value)"));
        assert!(source.contains("_l$decodedResult = (_l$resultReader.readI32(),);"));
        assert!(source.contains("writeI32(value.$1);"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn dart_target_rejects_unadvertised_interned_strings_before_rendering() {
        let bindings = bindings(
            r#"
            use boltffi::InternedString;

            boltffi::interned_string_pool! {
                pub BrowserName { Chrome = "Chrome" }
            }

            #[export]
            pub fn browser() -> InternedString<BrowserName> {
                BrowserName::CHROME
            }
            "#,
        );
        let output = target(DartHost::new().package("demo"))
            .render_partial(&bindings)
            .expect("partial Dart generation should report unsupported interned strings");

        let source = file(&output, "demo/lib/demo.dart");
        assert!(!source.contains("String browser()"));
        assert_eq!(output.coverage().unsupported().len(), 1);
        assert_eq!(
            output.coverage().unsupported()[0].reason(),
            "capability was not advertised"
        );
    }

    #[test]
    fn dart_target_renders_stream_delivery_modes_from_binding_ir() {
        let bindings = bindings(
            r#"
            use boltffi::EventSubscription;
            use std::sync::Arc;

            #[data]
            pub struct Message { pub text: String }

            pub struct Engine;

            #[export]
            impl Engine {
                #[ffi_stream(item = i32)]
                pub fn values(&self) -> Arc<EventSubscription<i32>> { loop {} }

                #[ffi_stream(item = Message, mode = "batch")]
                pub fn messages(&self) -> Arc<EventSubscription<Message>> { loop {} }

                #[ffi_stream(item = i32, mode = "callback")]
                pub fn ticks(&self) -> Arc<EventSubscription<i32>> { loop {} }
            }
            "#,
        );
        let output = target(DartHost::new().package("demo"))
            .render(&bindings)
            .expect("stream delivery modes should render from Binding IR");

        let source = file(&output, "demo/lib/demo.dart");
        assert!(source.contains("$$async.Stream<int> values()"));
        assert!(source.contains("$$BoltStreamPopBatchHandle<Message> messages()"));
        assert!(
            source.contains("$$async.StreamSubscription<int> ticks(void Function(int) callback)")
        );
        assert!(source.contains("$$ffi.NativeCallable.listener(streamCallback)"));
        assert!(source.contains("unsubscribeFn(handle);\n        release();"));
        assert!(output.diagnostics().is_empty());
    }
}
