mod codec;
mod name_style;
mod primitive;
mod render;
mod syntax;

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    RecordDecl, StreamDecl, Wasm32,
};

use crate::{
    bridge::wasm::{WasmBridge, WasmBridgeContract},
    core::{
        BindingCapability, BridgeCapability, CapabilityRequirements, Emitted, GeneratedOutput,
        HostCapabilities, RenderContext, RenderedDeclaration, Result, Target, contract::sealed,
        host,
    },
};

use name_style::ModuleName;
use render::{
    Callback, Class, Constant, CustomType, Enumeration, Function, Module, Record, Stream,
};
use syntax::{StringLiteral, Syntax};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct TypeScriptHost {
    module: ModuleName,
    runtime_package: StringLiteral,
}

impl TypeScriptHost {
    pub fn new(module: impl Into<String>) -> Result<Self> {
        Ok(Self {
            module: ModuleName::parse(module)?,
            runtime_package: StringLiteral::new("@boltffi/runtime"),
        })
    }

    pub fn runtime_package(mut self, package: impl AsRef<str>) -> Self {
        self.runtime_package = StringLiteral::new(package.as_ref());
        self
    }

    pub fn into_target(self) -> Target<Self, WasmBridge> {
        Target::new(self, WasmBridge)
    }
}

impl host::HostBackend for TypeScriptHost {
    type Surface = Wasm32;
    type Bridge = WasmBridgeContract;
    type Syntax = Syntax;

    fn name(&self) -> &'static str {
        "typescript"
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
        Function::from_declaration(decl, context)?.render()
    }

    fn class(
        &self,
        decl: &ClassDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Class::from_declaration(decl, context)?.render()
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
        Stream::from_declaration(decl, context)?.render()
    }

    fn constant(
        &self,
        decl: &ConstantDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        if decl.owner().is_some() {
            return Ok(Emitted::primary(""));
        }
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
        bindings: &Bindings<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
        declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        Module::new(&self.module, &self.runtime_package).render(bindings, context, declarations)
    }
}

impl sealed::HostBackend for TypeScriptHost {}

#[cfg(test)]
mod tests {
    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Bindings, Wasm32, lower};

    use super::TypeScriptHost;

    fn bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub fn noop() {}

                #[export]
                pub fn echo_bool(value: bool) -> bool { value }

                #[export]
                pub fn add(left: i32, right: i32) -> i32 { left + right }

                #[export]
                pub fn apply_closure(callback: impl Fn(i32) -> i32, value: i32) -> i32 {
                    callback(value)
                }

                #[export]
                pub fn echo_u64(value: u64) -> u64 { value }

                #[export]
                pub fn echo_string(value: String) -> String { value }

                #[export]
                pub fn echo_bytes(value: Vec<u8>) -> Vec<u8> { value }

                #[export]
                pub fn echo_vec_i32(value: Vec<i32>) -> Vec<i32> { value }

                #[export]
                pub fn echo_vec_bool(value: Vec<bool>) -> Vec<bool> { value }

                #[export]
                pub fn increment_u64(value: &mut [u64]) {
                    if let Some(first) = value.first_mut() {
                        *first += 1;
                    }
                }

                #[export]
                pub fn echo_optional_i32(value: Option<i32>) -> Option<i32> { value }

                #[export]
                pub fn echo_optional_i64(value: Option<i64>) -> Option<i64> { value }

                #[export]
                pub fn echo_optional_f64(value: Option<f64>) -> Option<f64> { value }

                #[export]
                pub fn echo_optional_vec_i32(value: Option<Vec<i32>>) -> Option<Vec<i32>> { value }

                #[export]
                pub fn result_to_string(value: Result<i32, String>) -> String {
                    format!("{value:?}")
                }

                #[export]
                pub fn echo_vec_string(value: Vec<String>) -> Vec<String> { value }

                #[export]
                pub fn echo_vec_vec_i32(value: Vec<Vec<i32>>) -> Vec<Vec<i32>> { value }

                #[export]
                pub fn echo_hash_map(
                    value: std::collections::HashMap<String, Vec<i32>>,
                ) -> std::collections::HashMap<String, Vec<i32>> { value }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    fn custom_type_default_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(include_str!(
                "../../../tests/fixtures/source/records/custom_type_default.rs"
            ))
            .expect("valid custom type default source"),
            PackageInfo::new("demo", None),
        )
        .expect("custom type default source scans");
        lower::<Wasm32>(&source).expect("custom type default source lowers")
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
                pub const LARGE: i64 = 9_007_199_254_740_993;

                #[export]
                pub const HALF: f64 = 0.5;

                #[export]
                pub const LABEL: &str = "boltffi";

                #[export]
                pub const BYTES: &'static [u8] = b"ffi";

                #[data]
                #[repr(u8)]
                pub enum Mode {
                    Fast = 1,
                    Safe = 2,
                }

                #[export]
                pub const MODE: Mode = Mode::Fast;

                #[data]
                pub enum State {
                    Idle,
                    Busy { jobs: u32 },
                }

                #[export]
                pub const IDLE: State = State::Idle;

                #[export]
                pub const ALIAS: &str = LABEL;

                #[export]
                pub const COMPUTED: u32 = 6 * 7;

                #[export]
                pub const PAIR: (u32, u32) = (3, 5);

                #[export]
                pub const BUSY: State = State::Busy { jobs: 3 };
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    fn associated_constant_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(include_str!(
                "../../../tests/fixtures/source/constant/associated.rs"
            ))
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    fn partially_supported_constant_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub trait Handler {
                    fn invoke(&self, value: i32) -> i32;
                }

                #[export]
                pub const ANSWER: u32 = 6 * 7;

                #[export]
                pub const HANDLERS: Vec<Box<dyn Handler>> = Vec::new();
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    fn byte_return_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub fn owned() -> Vec<u8> { Vec::new() }

                #[export]
                pub fn borrowed() -> &'static [u8] { b"ffi" }

                #[export]
                pub const CONSTANT: &'static [u8] = b"ffi";
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    fn record_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[data]
                #[repr(C)]
                pub struct Point {
                    pub x: f64,
                    pub active: bool,
                    pub y: f64,
                }

                #[data(impl)]
                impl Point {
                    pub fn origin() -> Self {
                        Self { x: 0.0, active: false, y: 0.0 }
                    }

                    pub fn x_value(&self) -> f64 { self.x }

                }

                #[data]
                pub struct MutablePoint {
                    pub x: f64,
                    pub y: f64,
                }

                #[data(impl)]
                impl MutablePoint {
                    pub fn scale(&mut self, factor: f64) {
                        self.x *= factor;
                        self.y *= factor;
                    }
                }

                #[data]
                pub struct User {
                    pub name: String,
                    pub scores: Vec<i32>,
                }

                #[data]
                #[repr(i8)]
                pub enum Status {
                    Inactive = -1,
                    Active = 1,
                }

                #[data(impl)]
                impl Status {
                    pub fn new(value: i8) -> Self {
                        if value == 1 { Self::Active } else { Self::Inactive }
                    }

                    pub fn inactive() -> Self { Self::Inactive }

                    pub fn is_active(&self) -> bool { matches!(self, Self::Active) }
                }

                #[data]
                pub enum Filter {
                    None,
                    ByName { name: String },
                    ByRange(i32, i32),
                }

                #[data(impl)]
                impl Filter {
                    pub fn none() -> Self { Self::None }

                    pub fn is_none(&self) -> bool { matches!(self, Self::None) }
                }

                #[data]
                pub struct Task {
                    pub title: String,
                    pub status: Status,
                }

                #[export]
                pub fn echo_user(value: User) -> User { value }

                #[export]
                pub fn echo_status(value: Status) -> Status { value }

                #[export]
                pub fn echo_task(value: Task) -> Task { value }

                #[export]
                pub fn echo_filter(value: Filter) -> Filter { value }

                #[export]
                pub fn echo_point(value: Point) -> Point { value }

                #[export]
                pub fn point_x(value: Point) -> f64 { value.x }
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

                    pub fn get(&self) -> i32 { self.0 }

                    pub fn add(&self, amount: i32) -> i32 { self.0 + amount }

                    pub fn doubled(value: i32) -> i32 { value * 2 }

                    pub fn duplicate(&self) -> Self { Self(self.0) }

                    pub fn optional(value: Option<Self>) -> Option<Self> { value }
                }

                #[export]
                pub fn describe_counter(value: &Counter) -> i32 { value.0 }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    fn custom_bindings() -> Bindings<Wasm32> {
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
        lower::<Wasm32>(&source).expect("source lowers")
    }

    fn async_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub async fn async_add(left: i32, right: i32) -> i32 { left + right }

                #[export]
                pub async fn async_name(value: String) -> String { value }

                #[export]
                pub async fn async_values(value: Vec<i32>) -> Vec<i32> { value }

                #[export]
                pub async fn async_size() -> usize { 2 }

                #[data]
                #[repr(C)]
                pub struct AsyncPoint {
                    pub x: f64,
                    pub y: f64,
                }

                #[export]
                pub async fn async_point(value: AsyncPoint) -> AsyncPoint { value }

                pub struct Worker(i32);

                #[export]
                impl Worker {
                    pub fn new(value: i32) -> Self { Self(value) }

                    pub async fn get(&self) -> i32 { self.0 }

                    pub async fn duplicate(&self) -> Self { Self(self.0) }
                }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    fn fallible_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[data]
                pub enum ParseError {
                    Empty,
                    Invalid { value: String },
                }

                #[data]
                #[repr(C)]
                pub struct FalliblePoint {
                    pub x: f64,
                    pub y: f64,
                }

                #[data]
                pub struct AppError {
                    pub message: String,
                    pub code: i32,
                }

                pub struct FallibleCounter(i32);

                #[export]
                impl FallibleCounter {
                    pub fn new(value: i32) -> Self { Self(value) }

                    pub fn try_new(value: i32) -> Result<Self, String> { Ok(Self(value)) }
                }

                #[export]
                pub fn safe_divide(left: i32, right: i32) -> Result<i32, String> {
                    Ok(left / right)
                }

                #[export]
                pub fn parse_value(value: String) -> Result<i32, ParseError> { Ok(1) }

                #[export]
                pub fn fallible_name(value: String) -> Result<String, String> { Ok(value) }

                #[export]
                pub fn fallible_point(value: FalliblePoint) -> Result<FalliblePoint, AppError> {
                    Ok(value)
                }

                #[export]
                pub async fn async_parse_value(value: String) -> Result<i32, ParseError> { Ok(1) }

                #[export]
                pub async fn async_fallible_values(value: Vec<i32>) -> Result<Vec<i32>, String> {
                    Ok(value)
                }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    fn reserved_member_bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[export]
                #[allow(async_fn_in_trait)]
                pub trait Store {
                    async fn delete(&self, key: String);
                    fn r#new(&self, key: String);
                }

                #[export]
                pub async fn evict(store: impl Store, key: String) { store.delete(key).await }
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
                #[repr(C)]
                pub struct Point {
                    pub x: f64,
                    pub y: f64,
                }

                #[data]
                pub struct Message {
                    pub text: String,
                    pub values: Vec<i32>,
                }

                pub struct EventBus;

                #[export]
                impl EventBus {
                    pub fn new() -> Self { Self }

                    #[ffi_stream(item = i32)]
                    pub fn values(&self) -> Arc<EventSubscription<i32>> { todo!() }

                    #[ffi_stream(item = Point, mode = "batch")]
                    pub fn points(&self) -> Arc<EventSubscription<Point>> { todo!() }

                    #[ffi_stream(item = Message, mode = "callback")]
                    pub fn messages(&self) -> Arc<EventSubscription<Message>> { todo!() }

                    #[ffi_stream(item = (i32, String))]
                    pub fn tuples(&self) -> Arc<EventSubscription<(i32, String)>> { todo!() }

                    #[ffi_stream(item = std::collections::HashMap<String, i32>)]
                    pub fn maps(&self) -> Arc<EventSubscription<std::collections::HashMap<String, i32>>> { todo!() }
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
    fn browser_init_accepts_a_precompiled_wasm_module_source() {
        let output = TypeScriptHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&bindings())
            .expect("target renders");

        let browser = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.ts"))
            .expect("browser module");
        assert!(browser.contents().contains(
            "export default async function init(source: BufferSource | Response | WebAssembly.Module): Promise<void>"
        ));
    }

    #[test]
    fn renders_primitive_functions_through_the_wasm_surface() {
        let output = TypeScriptHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&bindings())
            .expect("target renders");

        assert_eq!(output.files().len(), 2);
        let browser = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.ts"))
            .expect("browser module");
        let node = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo_node.ts"))
            .expect("node module");
        assert!(
            browser
                .contents()
                .contains("instantiateBoltFFI(source, WASM_ABI_VERSION,")
        );
        assert!(
            node.contents()
                .contains("instantiateBoltFFISync(_wasmBytes, WASM_ABI_VERSION,")
        );
        assert!(browser.contents().contains("export function noop(): void"));
        assert!(
            browser
                .contents()
                .contains("export function echoBool(value: boolean): boolean")
        );
        assert!(browser.contents().contains(
            "return (_exports.boltffi_function_demo_echo_bool as Function)(value) !== 0;"
        ));
        assert!(
            browser
                .contents()
                .contains("export function add(left: number, right: number): number")
        );
        assert!(
            browser
                .contents()
                .contains("export type ClosureI32ToI32 = (arg0: number) => number;")
        );
        assert!(browser.contents().contains(
            "_callbackImports[\"__boltffi_callback_closure____closure__i32_to_i32_call\"]"
        ));
        assert!(browser.contents().contains(
            "export function applyClosure(callback: ClosureI32ToI32, value: number): number"
        ));
        assert!(
            browser
                .contents()
                .contains("export function echoU64(value: bigint): bigint")
        );
        assert!(
            browser
                .contents()
                .contains("const __boltffi_value_allocation = _module.allocOwnedString(value);")
        );
        assert!(browser.contents().contains(
            "return _module.takePackedUtf8String((_exports.boltffi_function_demo_echo_string as Function)(__boltffi_value_allocation.ptr, __boltffi_value_allocation.len) as bigint);"
        ));
        assert_eq!(
            browser
                .contents()
                .matches("_module.freeAlloc(__boltffi_value_allocation);")
                .count(),
            1
        );
        assert!(
            browser
                .contents()
                .contains("const __boltffi_value_allocation = _module.allocWireBytes(value);")
        );
        // The argument keeps its length prefix; the return does not need one,
        // since the packed value already carries the length.
        assert!(browser.contents().contains(
            "return _module.takePackedBytes((_exports.boltffi_function_demo_echo_bytes as Function)(__boltffi_value_allocation.ptr, __boltffi_value_allocation.len) as bigint);"
        ));
        assert!(browser.contents().contains(
            "export function echoVecI32(value: readonly number[] | Int32Array): Int32Array"
        ));
        assert!(
            browser
                .contents()
                .contains("const __boltffi_value_allocation = _module.allocI32Array(value);")
        );
        assert!(
            browser
                .contents()
                .contains("return _module.takeSlotI32Array();")
        );
        assert!(
            browser
                .contents()
                .contains("export function echoVecBool(value: readonly boolean[]): boolean[]")
        );
        assert!(
            browser
                .contents()
                .contains("export function incrementU64(value: BigUint64Array): void")
        );
        assert!(browser.contents().contains(
            "_module.copyPrimitiveBufferInto(__boltffi_value_allocation, value, \"u64\");"
        ));
        assert!(
            browser
                .contents()
                .contains("export function echoOptionalI32(value: number | null): number | null")
        );
        assert!(browser.contents().contains(
            "(_exports.boltffi_function_demo_echo_optional_i32 as Function)((value === null ? Number.NaN : value))"
        ));
        assert!(browser.contents().contains(
            "export function resultToString(value: number | WireResult<number, string> | Error): string"
        ));
        assert!(browser.contents().contains("_module.unpackOptionI32("));
        assert!(
            browser
                .contents()
                .contains("export function echoOptionalI64(value: bigint | null): bigint | null")
        );
        assert!(browser.contents().contains(
            "const __boltffi_value_writer = _module.allocWriter(wireOptionalSize(value, (__boltffiValue0) => 8));"
        ));
        assert!(
            browser
                .contents()
                .contains("__boltffi_value_writer.writeOptional(value, (__boltffiValue0) => {")
        );
        assert!(
            browser
                .contents()
                .contains("__boltffi_value_writer.writeI64(__boltffiValue0);")
        );
        assert!(
            browser
                .contents()
                .contains("return _module.takePackedOptionalI64(")
        );
        assert!(
            browser
                .contents()
                .contains("export function echoOptionalF64(value: number | null): number | null")
        );
        assert!(browser.contents().contains(
            "export function echoOptionalVecI32(value: Array<number> | Int32Array | null): Array<number> | Int32Array | null"
        ));
        assert!(
            browser
                .contents()
                .contains("__boltffiReader.readOptional(() => __boltffiReader.readI32Array())")
        );
        assert!(
            browser
                .contents()
                .contains("export function echoVecString(value: Array<string>): Array<string>")
        );
        assert!(browser.contents().contains(
            "wireArraySize(value, (__boltffiValue0) => wireStringSize(__boltffiValue0))"
        ));
        assert!(
            browser
                .contents()
                .contains("__boltffiReader.readArray(() => __boltffiReader.readString())")
        );
        assert!(browser.contents().contains(
            "export function echoVecVecI32(value: Array<Array<number> | Int32Array>): Array<Array<number> | Int32Array>"
        ));
        assert!(browser.contents().contains(
            "export function echoHashMap(value: Map<string, Array<number> | Int32Array>): Map<string, Array<number> | Int32Array>"
        ));
        assert!(browser.contents().contains("wireMapSize(value,"));
        assert!(
            browser
                .contents()
                .contains("__boltffi_value_writer.writeMap(value,")
        );
        assert!(browser.contents().contains(
            "__boltffiReader.readMap(() => __boltffiReader.readString(), () => __boltffiReader.readI32Array())"
        ));
    }

    #[test]
    fn renders_stream_modes_and_items_from_shared_plans() {
        let output = TypeScriptHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&stream_bindings())
            .expect("target renders");
        let browser = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.ts"))
            .expect("browser module");
        let contents = browser.contents();

        assert!(contents.contains("values(): AsyncIterable<number>;"));
        assert!(contents.contains("points(): StreamSession<Point>;"));
        assert!(
            contents.contains(
                "messages(callback: (item: Message) => void): StreamCancellable<Message>;"
            )
        );
        assert!(contents.contains(
            "(_exports.boltffi_stream_demo_event_bus_values_poll as Function)(subscription);"
        ));
        assert!(contents.contains("_module.streamManager,"));
        assert!(contents.contains("tuples(): AsyncIterable<[number, string]>;"));
        assert!(contents.contains("[reader.readI32(), reader.readString()]"));
        assert!(contents.contains("maps(): AsyncIterable<Map<string, number>>;"));
        assert!(
            contents.contains("reader.readMap(() => reader.readString(), () => reader.readI32())")
        );
        assert!(!contents.contains("setTimeout"));
    }

    #[test]
    fn renders_inline_and_accessor_constants_after_wasm_initialization() {
        let output = TypeScriptHost::new("demo")
            .expect("host builds")
            .into_target()
            .render(&constant_bindings())
            .expect("target renders");
        let browser = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.ts"))
            .expect("browser module");
        let node = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo_node.ts"))
            .expect("node module");

        assert!(
            browser
                .contents()
                .contains("export const enabled: boolean = true;")
        );
        assert!(
            browser
                .contents()
                .contains("export const answer: number = 42;")
        );
        assert!(
            browser
                .contents()
                .contains("export const large: bigint = 9007199254740993n;")
        );
        assert!(
            browser
                .contents()
                .contains("export const half: number = 0.5;")
        );
        assert!(
            browser
                .contents()
                .contains("export const label: string = \"boltffi\";")
        );
        assert!(browser.contents().contains("export let bytes: Uint8Array;"));
        assert!(
            browser
                .contents()
                .contains("export const mode: Mode = Mode.Fast;")
        );
        assert!(
            browser
                .contents()
                .contains("export const idle: State = { tag: \"Idle\" };")
        );
        assert!(browser.contents().contains("export let alias: string;"));
        assert!(browser.contents().contains("export let computed: number;"));
        assert!(
            browser
                .contents()
                .contains("export let pair: [number, number];")
        );
        assert!(browser.contents().contains("export let busy: State;"));
        assert!(
            browser
                .contents()
                .contains("const _readBytes = (): Uint8Array =>")
        );
        assert!(browser.contents().contains("  bytes = _readBytes();"));
        assert!(browser.contents().contains("  alias = _readAlias();"));
        assert!(browser.contents().contains("  computed = _readComputed();"));
        assert!(browser.contents().contains("  pair = _readPair();"));
        assert!(browser.contents().contains("  busy = _readBusy();"));
        assert!(node.contents().contains("const _exports: BoltFFIExports"));
        assert!(node.contents().contains("  bytes = _readBytes();"));
    }

    #[test]
    fn renders_associated_constants_on_the_owner_value() {
        let output = TypeScriptHost::new("demo")
            .expect("host builds")
            .into_target()
            .render(&associated_constant_bindings())
            .expect("target renders");
        let browser = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.ts"))
            .expect("browser module");
        let source = browser.contents();

        assert!(source.contains("BLACK: undefined as unknown as Color,"));
        assert!(source.contains("CHANNEL_COUNT: 4,"));
        assert!(source.contains("const _readColorBlack = (): Color =>"));
        assert!(source.contains("Object.assign(Color, { BLACK: _readColorBlack() });"));
        assert!(source.contains("DEFAULT: 1,"));
        assert!(source.contains("INITIAL: { tag: \"Idle\" },"));
        assert!(source.contains("static readonly MAX_COLORS: number = 16;"));
        assert!(!source.contains("export let black:"));
        assert!(!source.contains("UNEXPORTED_ASSOCIATED"));
    }

    #[test]
    fn partial_constants_initialize_only_rendered_declarations() {
        let output = TypeScriptHost::new("demo")
            .expect("host builds")
            .into_target()
            .render_partial(&partially_supported_constant_bindings())
            .expect("partial target renders supported constants");
        let browser = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.ts"))
            .expect("browser module");

        assert!(browser.contents().contains("export let answer: number;"));
        assert!(browser.contents().contains("  answer = _readAnswer();"));
        assert!(!browser.contents().contains("handlers"));
        assert_eq!(output.coverage().unsupported().len(), 1);
        assert_eq!(
            output.coverage().unsupported()[0].declaration().name(),
            "handlers"
        );
    }

    #[test]
    fn renders_record_codecs_from_shared_field_plans() {
        let output = TypeScriptHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&record_bindings())
            .expect("target renders");
        let browser = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.ts"))
            .expect("browser module");

        assert!(browser.contents().contains("export interface Point"));
        assert!(
            browser
                .contents()
                .contains("size: (value) => ((8 + 1) + 8)")
        );
        assert!(
            browser
                .contents()
                .contains("writer.writeBool(value.active);")
        );
        assert!(browser.contents().contains("reader.readBool();"));
        assert!(browser.contents().contains("export interface User"));
        assert!(browser.contents().contains(
            "size: (value) => (wireStringSize(value.name) + (4 + (value.scores.length * 4)))"
        ));
        assert!(
            browser
                .contents()
                .contains("writer.writeString(value.name);")
        );
        assert!(
            browser
                .contents()
                .contains("UserCodec.encode(__boltffi_value_writer, value);")
        );
        assert!(
            browser
                .contents()
                .contains("UserCodec.decode(__boltffiReader)")
        );
        assert!(
            browser
                .contents()
                .contains("export function echoUser(value: User): User")
        );
        assert!(browser.contents().contains("export const Status ="));
        assert!(browser.contents().contains("Inactive: -1"));
        assert!(
            browser
                .contents()
                .contains("fromRaw(value: number): Status")
        );
        assert!(!browser.contents().contains("new(value: number): Status"));
        assert!(browser.contents().contains("inactive(): Status"));
        assert!(
            browser
                .contents()
                .contains("isActive(self: Status): boolean")
        );
        assert!(browser.contents().contains("writer.writeI8(value);"));
        assert!(
            browser
                .contents()
                .contains("case -1: return Status.Inactive;")
        );
        assert!(
            browser
                .contents()
                .contains("export function echoStatus(value: Status): Status")
        );
        assert!(browser.contents().contains("readonly status: Status;"));
        assert!(
            browser
                .contents()
                .contains("StatusCodec.encode(writer, value.status);")
        );
        assert!(browser.contents().contains("StatusCodec.decode(reader)"));
        assert!(browser.contents().contains("export type Filter ="));
        assert!(browser.contents().contains("export const Filter ="));
        assert!(browser.contents().contains("none(): Filter"));
        assert!(browser.contents().contains("isNone(self: Filter): boolean"));
        assert!(
            browser
                .contents()
                .contains("| { readonly tag: \"ByName\"; readonly name: string }")
        );
        assert!(browser.contents().contains(
            "| { readonly tag: \"ByRange\"; readonly value0: number; readonly value1: number };"
        ));
        assert!(browser.contents().contains("case \"ByName\": return"));
        assert!(
            browser
                .contents()
                .contains("case 1: return { tag: \"ByName\", name: reader.readString() };")
        );
        assert!(
            browser
                .contents()
                .contains("export function echoFilter(value: Filter): Filter")
        );
        assert!(
            browser
                .contents()
                .contains("export function echoPoint(value: Point): Point")
        );
        assert!(browser.contents().contains(
            "const __boltffi_value_writer = _module.allocWriter(PointCodec.size(value));"
        ));
        assert!(browser.contents().contains(
            "return _module.readPackedBuffer((_exports.boltffi_function_demo_echo_point"
        ));
        assert!(
            browser
                .contents()
                .contains("(__boltffiReader) => PointCodec.decode(__boltffiReader)")
        );
        assert!(
            browser
                .contents()
                .contains("export function pointX(value: Point): number")
        );
        assert!(browser.contents().contains("export const Point ="));
        assert!(browser.contents().contains("origin(): Point"));
        assert!(browser.contents().contains("xValue(self: Point): number"));
        assert!(
            browser
                .contents()
                .contains("scale(self: MutablePoint, factor: number): MutablePoint")
        );
        assert!(browser.contents().contains(
            "const __boltffi_self_writer = _module.allocWriter(MutablePointCodec.size(self));"
        ));
        assert!(
            browser
                .contents()
                .contains("_module.checkStatus((_exports.")
        );
        assert!(
            browser
                .contents()
                .contains("Object.assign(self, MutablePointCodec.decode(_module.readerFromWriter(__boltffi_self_writer)));")
        );
    }

    #[test]
    fn renders_class_lifetimes_and_handle_calls_from_shared_plans() {
        let output = TypeScriptHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&class_bindings())
            .expect("target renders");
        let browser = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.ts"))
            .expect("browser module");

        assert!(browser.contents().contains("export class Counter"));
        assert!(
            browser
                .contents()
                .contains("_CounterFinalizer?.register(this, handle, this);")
        );
        assert!(
            browser
                .contents()
                .contains("static new(initial: number): Counter")
        );
        assert!(browser.contents().contains("get(): number"));
        assert!(browser.contents().contains("this._assertNotDisposed();"));
        assert!(browser.contents().contains("this._handle"));
        assert!(
            browser
                .contents()
                .contains("static doubled(value: number): number")
        );
        assert!(browser.contents().contains("duplicate(): Counter"));
        assert!(browser.contents().contains("Counter._fromHandle("));
        assert!(
            browser
                .contents()
                .contains("optional(value: Counter | null): Counter | null")
        );
        assert!(browser.contents().contains("Counter._toHandle(value)"));
        assert!(
            browser
                .contents()
                .contains("export function describeCounter(value: Counter): number")
        );
    }

    #[test]
    fn renders_async_calls_from_the_wasm_execution_protocol() {
        let output = TypeScriptHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&async_bindings())
            .expect("target renders");
        let browser = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.ts"))
            .expect("browser module");

        assert!(browser.contents().contains(
            "export async function asyncAdd(left: number, right: number, options?: { signal?: AbortSignal; cancelId?: number }): Promise<number>"
        ));
        assert!(
            browser
                .contents()
                .contains("const __boltffiSignal = options?.signal;")
        );
        assert!(
            browser
                .contents()
                .contains("const __boltffiCancelId = options?.cancelId;")
        );
        assert!(
            browser
                .contents()
                .contains("await _module.asyncManager.pollAsync(")
        );
        assert!(browser.contents().contains("_module.completeAsync("));
        assert!(
            browser
                .contents()
                .contains("if (__boltffiSignal?.aborted) throw new BoltFFICancelledError();")
        );
        assert!(browser.contents().contains("__boltffiHandle) =>"));
        assert!(
            browser
                .contents()
                .contains(", __boltffiSignal, __boltffiCancelId)")
        );
        assert!(
            browser
                .contents()
                .contains("import { BoltFFICancelledError,")
        );
        assert!(browser.contents().contains(
            "export async function asyncName(value: string, options?: { signal?: AbortSignal; cancelId?: number }): Promise<string>"
        ));
        assert!(browser.contents().contains("_module.takePackedUtf8String("));
        assert!(browser.contents().contains(
            "export async function asyncValues(value: readonly number[] | Int32Array, options?: { signal?: AbortSignal; cancelId?: number }): Promise<Int32Array>"
        ));
        assert!(browser.contents().contains("_module.takeSlotI32Array()"));
        assert!(browser.contents().contains(
            "export async function asyncSize(options?: { signal?: AbortSignal; cancelId?: number }): Promise<number>"
        ));
        assert!(
            !browser
                .contents()
                .contains("return BigInt(_module.completeAsync(")
        );
        assert!(
            browser
                .contents()
                .contains("(__boltffiAwaitedFuture, __boltffiStatus, __boltffiReturnWriter.ptr)")
        );
        assert!(
            browser.contents().contains(
                "AsyncPointCodec.decode(_module.readerFromWriter(__boltffiReturnWriter))"
            )
        );
        assert!(browser.contents().contains(
            "async get(options?: { signal?: AbortSignal; cancelId?: number }): Promise<number>"
        ));
        assert!(browser.contents().contains(
            "async duplicate(options?: { signal?: AbortSignal; cancelId?: number }): Promise<Worker>"
        ));
        assert!(browser.contents().contains("Worker._fromHandle("));
    }

    #[test]
    fn checks_an_aborted_signal_before_any_parameter_setup_runs() {
        // A `String` parameter's setup allocates ownership Rust will take on
        // the native call -- if the pre-abort check ran after that setup
        // instead of before it, an already-aborted call would leak the
        // allocation with no cleanup path.
        let output = TypeScriptHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&async_bindings())
            .expect("target renders");
        let browser = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.ts"))
            .expect("browser module");
        let source = browser.contents();

        let abort_check = source
            .find("if (__boltffiSignal?.aborted) throw new BoltFFICancelledError();")
            .expect("pre-abort check present");
        let param_setup = source
            .find("_module.allocOwnedString(value)")
            .expect("string parameter setup present");
        assert!(
            abort_check < param_setup,
            "pre-abort check must run before parameter setup:\n{source}"
        );
    }

    #[test]
    fn appends_an_internal_options_name_when_the_callable_already_has_one() {
        let output = TypeScriptHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&async_bindings_with_options_param())
            .expect("target renders");
        let browser = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.ts"))
            .expect("browser module");

        assert!(browser.contents().contains(
            "export async function asyncEcho(options: string, __boltffiOptions?: { signal?: AbortSignal; cancelId?: number }): Promise<string>"
        ));
        assert!(
            browser
                .contents()
                .contains("const __boltffiSignal = __boltffiOptions?.signal;")
        );
        assert!(
            browser
                .contents()
                .contains("const __boltffiCancelId = __boltffiOptions?.cancelId;")
        );
    }

    fn async_bindings_with_options_param() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub async fn async_echo(options: String) -> String { options }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    #[test]
    fn falls_back_to_a_double_underscore_options_name_when_the_plain_fallback_also_collides() {
        // A callable declaring both `options` and `boltffi_options` would
        // make a plain `boltffiOptions` fallback collide too -- the
        // generated name must stay collision-free either way.
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub async fn async_echo(options: String, boltffi_options: String) -> String {
                    format!("{options}{boltffi_options}")
                }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        let output = TypeScriptHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&lower::<Wasm32>(&source).expect("source lowers"))
            .expect("target renders");
        let browser = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.ts"))
            .expect("browser module");

        assert!(browser.contents().contains(
            "export async function asyncEcho(options: string, boltffiOptions: string, __boltffiOptions?: { signal?: AbortSignal; cancelId?: number }): Promise<string>"
        ));
    }

    #[test]
    fn renders_fallible_calls_from_split_success_and_error_plans() {
        let output = TypeScriptHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&fallible_bindings())
            .expect("target renders");
        let browser = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.ts"))
            .expect("browser module");

        assert!(
            browser
                .contents()
                .contains("export class ParseErrorException extends Error")
        );
        assert!(
            browser
                .contents()
                .contains("export class AppErrorException extends Error")
        );
        assert!(
            browser
                .contents()
                .contains("export function safeDivide(left: number, right: number): number")
        );
        assert!(browser.contents().contains("__boltffiError !== 0n"));
        assert!(
            browser
                .contents()
                .contains("throw new Error(_module.takePackedWireString(__boltffiError))")
        );
        assert!(
            browser
                .contents()
                .contains("return _module.readerFromWriter(__boltffiReturnWriter).readI32()")
        );
        assert!(
            browser
                .contents()
                .contains("throw new ParseErrorException(")
        );
        assert!(browser.contents().contains(
            "FalliblePointCodec.decode(_module.readerFromWriter(__boltffiReturnWriter))"
        ));
        assert!(
            browser
                .contents()
                .contains("static tryNew(value: number): FallibleCounter | null")
        );
        assert!(
            browser
                .contents()
                .contains("_module.takePackedWireString(__boltffiError);")
        );
        assert!(browser.contents().contains("return null;"));
        assert!(
            browser
                .contents()
                .contains("FallibleCounter._fromHandle(__boltffiReturnHandle)")
        );
        assert!(
            browser
                .contents()
                .contains("export async function asyncParseValue")
        );
        assert!(
            browser
                .contents()
                .contains("(__boltffiAwaitedFuture, __boltffiStatus, __boltffiReturnWriter.ptr)")
        );
        assert!(browser.contents().contains(
            "_module.takePackedBuffer(_module.readerFromWriter(__boltffiReturnWriter).readU64())"
        ));
    }

    #[test]
    fn renders_custom_types_from_their_shared_representation_codec() {
        let output = TypeScriptHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&custom_bindings())
            .expect("target renders");
        let browser = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.ts"))
            .expect("browser module");

        assert!(
            browser
                .contents()
                .contains("export type Timestamp = bigint;")
        );
        assert!(
            browser
                .contents()
                .contains("export function keepTimestamp(value: Timestamp): Timestamp")
        );
    }

    #[test]
    fn renders_custom_type_defaults_through_representations() {
        let output = TypeScriptHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&custom_type_default_bindings())
            .expect("target renders");
        let browser = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.ts"))
            .expect("browser module");

        assert!(
            browser
                .contents()
                .contains("readonly maxRejoinDistance?: Length;"),
            "{}",
            browser.contents()
        );
        assert!(browser.contents().contains(
            "value.maxRejoinDistance === undefined ? { meters: 1500.0 } : value.maxRejoinDistance"
        ));
    }

    /// Only an owned `Vec<u8>` crosses unframed.
    ///
    /// A borrowed slice is written by `borrowed_buffer`, which always frames,
    /// so reading it unframed hands back the four-byte length prefix as the
    /// start of the payload. Nothing downstream would notice: the array is
    /// simply four bytes too long and starts with a little-endian length.
    #[test]
    fn only_owned_byte_returns_skip_the_length_prefix() {
        let output = TypeScriptHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&byte_return_bindings())
            .expect("target renders");

        let browser = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.ts"))
            .expect("browser module");
        let contents = browser.contents();

        assert!(
            contents.contains("takePackedBytes((_exports.boltffi_function_demo_owned"),
            "an owned Vec<u8> return should skip the prefix",
        );
        for framed in [
            "boltffi_function_demo_borrowed",
            "boltffi_const_demo_constant",
        ] {
            assert!(
                contents.contains(&format!("takePackedWireBytes((_exports.{framed}")),
                "`{framed}` is written framed and must be read framed",
            );
        }
    }

    /// A property may be spelled with a reserved word, so a callback method
    /// named `delete` is declared as `delete` — and must then be *invoked* as
    /// `delete`. Escaping only the invocation compiles and renders fine, then
    /// calls `_delete` on an object the same file says has `delete`: the call
    /// yields `undefined`, the `.then` throws, and the failure surfaces to Rust
    /// as a panicked completion instead of anything naming the real cause.
    ///
    /// `new` is the one name that cannot be declared bare: `new(key: string)`
    /// in an interface is a construct signature, so the member would not exist
    /// no matter how it is invoked. It is quoted, and reached by index.
    #[test]
    fn invokes_callback_methods_by_their_declared_reserved_names() {
        let output = TypeScriptHost::new("demo")
            .expect("host constructs")
            .into_target()
            .render(&reserved_member_bindings())
            .expect("target renders");

        let declarations = [
            ("delete", "  delete(key: string)", "callback.delete("),
            ("new", "  \"new\"(key: string)", "callback[\"new\"]("),
        ];

        for file in output.files() {
            let contents = file.contents();
            for (name, declaration, invocation) in declarations {
                if !contents.contains(declaration) {
                    continue;
                }
                assert!(
                    contents.contains(invocation),
                    "{} declares `{name}` as `{declaration}` but does not invoke it as `{invocation}`",
                    file.path().as_path().display(),
                );
                assert!(
                    !contents.contains(&format!("callback._{name}(")),
                    "{} invokes the escaped `_{name}`",
                    file.path().as_path().display(),
                );
            }
        }

        let declared = output
            .files()
            .iter()
            .any(|file| file.contents().contains("  \"new\"(key: string)"));
        assert!(declared, "no file declared the quoted `new` member");
    }
}
