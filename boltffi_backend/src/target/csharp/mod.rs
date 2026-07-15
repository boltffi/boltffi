//! C# target rendered through .NET P/Invoke over the C ABI bridge.

mod codec;
mod name_style;
mod render;
mod syntax;
mod type_name;

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

use name_style::{Name, Namespace};
use syntax::Literal;

pub use crate::core::{
    CustomTypeConversion as CSharpCustomConversion, CustomTypeMapping as CSharpCustomMapping,
};
pub use syntax::{ArgumentList, Expression, Identifier, Statement, Syntax, TypeFragment};

/// C# host renderer for direct P/Invoke calls into the BoltFFI C ABI.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct CSharpHost {
    namespace: Option<Namespace>,
    library: Option<String>,
    custom_mappings: crate::core::CustomTypeMappingSet,
}

impl CSharpHost {
    /// Creates a C# host renderer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Selects the namespace used by generated C# source.
    pub fn namespace(mut self, namespace: impl AsRef<str>) -> Result<Self> {
        self.namespace = Some(Namespace::parse(namespace.as_ref())?);
        Ok(self)
    }

    /// Selects the native library name used by generated DllImport declarations.
    pub fn native_library(mut self, library: impl Into<String>) -> Self {
        self.library = Some(library.into());
        self
    }

    /// Registers a C# API mapping for one custom type.
    pub fn custom_mapping(
        mut self,
        custom_type: impl Into<String>,
        mapping: CSharpCustomMapping,
    ) -> Self {
        self.custom_mappings.insert(custom_type, mapping);
        self
    }

    /// Creates the backend target stack for this C# host.
    pub fn into_target(self) -> Result<Target<Self, CBridge>> {
        Ok(Target::new(self, CBridge::default_header()?))
    }

    fn namespace_for(&self, bindings: &Bindings<Native>) -> Result<Namespace> {
        self.namespace
            .clone()
            .map(Ok)
            .unwrap_or_else(|| Namespace::from_canonical(bindings.package().name()))
    }

    fn library_for(&self, bindings: &Bindings<Native>) -> String {
        self.library
            .clone()
            .unwrap_or_else(|| Name::new(bindings.package().name()).snake())
    }
}

impl host::HostBackend for CSharpHost {
    type Surface = Native;
    type Bridge = CBridgeContract;
    type Syntax = Syntax;

    fn name(&self) -> &'static str {
        "csharp"
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
        bindings: &Bindings<Self::Surface>,
    ) -> Result<ResolvedCustomTypeMappings> {
        self.custom_mappings
            .resolve(bindings, "csharp", |declaration| {
                Name::new(declaration.name())
                    .pascal()
                    .map(|name| name.to_string())
                    .unwrap_or_else(|_| declaration.name().as_path_string())
            })
    }

    fn record(
        &self,
        decl: &RecordDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Record::from_declaration(
            decl,
            self.namespace_for(context.bindings())?,
            bridge,
            context,
        )?
        .render()
    }

    fn enumeration(
        &self,
        decl: &EnumDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Enumeration::from_declaration(
            decl,
            self.namespace_for(context.bindings())?,
            bridge,
            context,
        )?
        .render()
    }

    fn function(
        &self,
        decl: &FunctionDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Function::from_declaration(decl, bridge, context)?.render()
    }

    fn class(
        &self,
        decl: &ClassDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Class::from_declaration(
            decl,
            self.namespace_for(context.bindings())?,
            bridge,
            context,
        )?
        .render()
    }

    fn callback(
        &self,
        decl: &CallbackDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Callback::from_declaration(
            decl,
            self.namespace_for(context.bindings())?,
            bridge,
            context,
        )?
        .render()
    }

    fn stream(
        &self,
        decl: &StreamDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Stream::from_declaration(decl, bridge, context)?.render()
    }

    fn constant(
        &self,
        decl: &ConstantDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Constant::from_declaration(decl, bridge, context)?.render()
    }

    fn custom_type(
        &self,
        _decl: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(Emitted::primary(""))
    }

    fn assemble<'decl>(
        &self,
        bindings: &Bindings<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
        declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        let namespace = self.namespace_for(bindings)?;
        render::Module::new(
            &namespace,
            Name::new(bindings.package().name()).pascal()?,
            Literal::string(&self.library_for(bindings)),
        )
        .render(declarations)
    }
}

impl sealed::HostBackend for CSharpHost {}

fn unsupported<T>(shape: &'static str) -> Result<T> {
    Err(Error::UnsupportedTarget {
        target: "csharp",
        shape,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Bindings, Native, lower};

    use crate::{GeneratedOutput, Target, bridge::c::CBridge};

    use super::{CSharpCustomMapping, CSharpHost};

    fn bindings(source: &str) -> Bindings<Native> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(source).expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source should scan");
        lower::<Native>(&source).expect("source should lower")
    }

    fn target(host: CSharpHost) -> Target<CSharpHost, CBridge> {
        host.into_target().expect("C# target")
    }

    fn file(output: &GeneratedOutput, path: impl AsRef<Path>) -> &str {
        output
            .files()
            .iter()
            .find(|file| file.path().as_path() == path.as_ref())
            .map(|file| file.contents())
            .expect("generated file")
    }

    #[test]
    fn csharp_target_renders_primitive_functions_through_pinvoke() {
        let bindings = bindings(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 { left + right }

            #[export]
            pub fn negate(enabled: bool) -> bool { !enabled }

            #[export]
            pub fn notify(value: u64) {}
            "#,
        );
        let output = target(
            CSharpHost::new()
                .namespace("Company.Bindings")
                .unwrap()
                .native_library("demo_native"),
        )
        .render(&bindings)
        .expect("primitive functions should render");

        insta::assert_snapshot!(file(&output, "Demo.cs"), @r###"
        // <auto-generated>
        // This file was generated by BoltFFI. Do not edit.
        // </auto-generated>
        #nullable enable

        using System.Runtime.InteropServices;

        namespace Company.Bindings
        {
            [StructLayout(LayoutKind.Sequential)]
            internal struct FfiStatus
            {
                internal int code;
            }

            public static class Demo
            {
                public static int Add(int left, int right)
                    => NativeMethods.NativeAdd(left, right);

                public static bool Negate(bool enabled)
                    => NativeMethods.NativeNegate(enabled);

                public static void Notify(ulong value)
                {
                    FfiStatus status = NativeMethods.NativeNotify(value);
                    if (status.code != 0)
                    {
                        throw new global::System.InvalidOperationException($"BoltFFI call failed with status code {status.code}");
                    }
                }

            }

            internal static class NativeMethods
            {
                internal const string LibName = "demo_native";

                [DllImport(LibName, EntryPoint = "boltffi_function_demo_add")]
                internal static extern int NativeAdd(int left, int right);

                [DllImport(LibName, EntryPoint = "boltffi_function_demo_negate")]
                [return: MarshalAs(UnmanagedType.I1)]
                internal static extern bool NativeNegate([MarshalAs(UnmanagedType.I1)] bool enabled);

                [DllImport(LibName, EntryPoint = "boltffi_function_demo_notify")]
                internal static extern FfiStatus NativeNotify(ulong value);

            }
        }
        "###);
        assert!(
            output
                .files()
                .iter()
                .any(|file| file.path().as_path() == Path::new("boltffi.h"))
        );
    }

    #[test]
    fn csharp_target_renders_empty_void_functions_without_status() {
        let bindings = bindings(
            r#"
            #[export]
            pub fn ping() {}
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("empty void function should render");

        let source = file(&output, "Demo.cs");
        assert!(source.contains("public static void Ping()"));
        assert!(source.contains("=> NativeMethods.NativePing();"));
        assert!(source.contains("internal static extern void NativePing();"));
        assert!(!source.contains("FfiStatus"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_async_poll_handle_functions() {
        let bindings = bindings(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 { left + right }

            #[export]
            pub async fn fetch(value: i32) -> i32 { value }

            #[export]
            pub async fn greet(value: String) -> String { value }

            #[export]
            pub async fn checked(value: i32) -> Result<i32, String> { Ok(value) }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("async C# render");

        let source = file(&output, "Demo.cs");
        assert!(source.contains("public static int Add(int left, int right)"));
        assert!(source.contains(
            "global::System.Threading.Tasks.Task<int> Fetch(int value, global::System.Threading.CancellationToken cancellationToken = default)"
        ));
        assert!(source.contains("return BoltFFIAsync.CallAsync<int>("));
        assert!(source.contains("NativeMethods.NativeFetchPoll"));
        assert!(source.contains("NativeMethods.NativeFetchComplete"));
        assert!(source.contains("BoltFFIAsync.ThrowIfStatus(boltffiStatus, cancellationToken);"));
        assert!(source.contains("global::System.Threading.Tasks.Task<string> Greet"));
        assert!(source.contains("global::System.Threading.Tasks.Task<int> Checked"));
        assert!(source.contains("throw new BoltException(boltffiErrorReader.ReadString());"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_callback_interfaces_and_handles() {
        let bindings = bindings(
            r#"
            #[export]
            pub trait ValueCallback {
                fn on_value(&self, value: i32) -> i32;
            }

            #[export]
            pub fn invoke(callback: impl ValueCallback, value: i32) -> i32 {
                callback.on_value(value)
            }

            #[export]
            pub fn make_callback(delta: i32) -> Box<dyn ValueCallback> {
                unimplemented!()
            }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("callbacks should render");

        let callback = file(&output, "ValueCallback.cs");
        assert!(callback.contains("public interface ValueCallback"));
        assert!(callback.contains("int OnValue(int value);"));
        assert!(callback.contains("internal static BoltFFICallbackHandle Create"));
        assert!(callback.contains("internal static ValueCallbackProxy Wrap"));
        assert!(callback.contains("return implementation.OnValue(value);"));

        let module = file(&output, "Demo.cs");
        assert!(module.contains("ValueCallback callback"));
        assert!(module.contains("ValueCallbackBridge.Create(callback)"));
        assert!(module.contains("return ValueCallbackBridge.Wrap(boltffiHandle);"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_callback_parameter_and_return_shapes() {
        let bindings = bindings(
            r#"
            #[export]
            pub trait Child {
                fn on_value(&self, value: u32) -> u32;
            }

            #[export]
            pub trait Listener {
                fn update(&self, value: Option<u32>);
                fn process(&self, values: Vec<i32>) -> Vec<i32>;
                fn on_child(&self, child: Box<dyn Child>);
                fn child(&self) -> Box<dyn Child>;
            }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("callback shapes should render");

        let listener = file(&output, "Listener.cs");
        assert!(listener.contains("void Update(uint? value);"));
        assert!(listener.contains("int[] Process(int[] values);"));
        assert!(listener.contains("void OnChild(Child child);"));
        assert!(listener.contains("Child Child();"));
        assert!(listener.contains("WriteU32(value.Value);"));
        assert!(listener.contains("ReadRawArray<int>()"));
        assert!(listener.contains("ChildBridge.Wrap(child)"));
        assert!(listener.contains("ChildBridge.Create(boltffiValue)"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_fallible_callback_methods() {
        let bindings = bindings(
            r#"
            #[error]
            pub enum MathError {
                Invalid,
            }

            #[export]
            pub trait Calculator {
                fn compute(&self, value: i32) -> Result<i32, MathError>;
                fn enabled(&self) -> Result<bool, MathError>;
                fn label(&self, value: i32) -> Result<String, String>;
            }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("fallible callbacks should render");

        let callback = file(&output, "Calculator.cs");
        assert!(callback.contains("int Compute(int value);"));
        assert!(callback.contains("string Label(int value);"));
        assert!(callback.contains("catch (MathErrorException boltffiError)"));
        assert!(callback.contains("catch (global::System.Exception boltffiError)"));
        assert!(callback.contains("return_out = implementation.Compute(value);"));
        assert!(callback.contains(
            "[global::System.Runtime.InteropServices.MarshalAs(global::System.Runtime.InteropServices.UnmanagedType.I1)] out bool return_out"
        ));
        assert!(callback.contains("out var return_out"));
        assert!(callback.contains("throw new MathErrorException("));
        assert!(callback.contains("throw new BoltException("));
        assert!(!callback.contains("This callback method shape has not migrated"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_async_callback_methods() {
        let bindings = bindings(
            r#"
            #[error]
            pub enum FetchError {
                Missing,
            }

            #[export]
            #[allow(async_fn_in_trait)]
            pub trait Fetcher {
                async fn fetch_count(&self, key: i32) -> i32;
                async fn enabled(&self) -> bool;
                async fn fetch_name(&self, key: String) -> String;
                async fn try_fetch(&self, key: i32) -> Result<String, FetchError>;
                async fn try_label(&self, key: i32) -> Result<String, String>;
            }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("async callbacks should render");

        let callback = file(&output, "Fetcher.cs");
        assert!(callback.contains("global::System.Threading.Tasks.Task<int> FetchCount"));
        assert!(callback.contains("private static async void FetchCount"));
        assert!(callback.contains("await implementation.FetchCount(key).ConfigureAwait(false)"));
        assert!(callback.contains(
            "[global::System.Runtime.InteropServices.MarshalAs(global::System.Runtime.InteropServices.UnmanagedType.I1)] bool arg2"
        ));
        assert!(callback.contains("TaskCompletionSource<int>"));
        assert!(callback.contains("GCHandle.Alloc(boltffiCompletion)"));
        assert!(callback.contains("boltffiComplete(1, FfiBuf.FromBytes"));
        assert!(callback.contains("boltffiStatus.code == 1"));
        assert!(callback.contains("catch (global::System.Exception boltffiError)"));
        assert!(!callback.contains("This callback method shape has not migrated"));

        let module = file(&output, "Demo.cs");
        assert!(module.contains("internal static extern void FreeBuf(FfiBuf buffer);"));
        assert!(module.contains("internal static extern FfiBuf BufFromBytes"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_encoded_functions_through_wire_runtime() {
        let bindings = bindings(
            r#"
            #[export]
            pub fn greet(name: String) -> String { name }

            #[export]
            pub fn echo_bytes(value: Vec<u8>) -> Vec<u8> { value }

            #[export]
            pub fn echo_maybe(value: Option<String>) -> Option<String> { value }

            #[export]
            pub fn echo_all(values: Vec<String>) -> Vec<String> { values }

            #[export]
            pub fn echo_count(value: Option<i32>) -> Option<i32> { value }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("encoded functions should render");

        let source = file(&output, "Demo.cs");
        assert!(source.contains("public static string Greet(string name)"));
        assert!(source.contains("nameWriter.WriteString(name);"));
        assert!(source.contains("return resultReader.ReadString();"));
        assert!(source.contains("public static byte[] EchoBytes(byte[] value)"));
        assert!(source.contains("public static string? EchoMaybe(string? value)"));
        assert!(source.contains("if (value is { } boltffiValue0)"));
        assert!(source.contains("public static string[] EchoAll(string[] values)"));
        assert!(source.contains("foreach (var boltffiValue0 in values)"));
        assert!(source.contains("public static int? EchoCount(int? value)"));
        assert!(source.contains("valueWriter.WriteI32(value.Value);"));
        assert!(
            source.contains("resultReader.ReadU8() == 0 ? default(int?) : resultReader.ReadI32()")
        );
        assert!(source.contains("internal sealed class WireReader"));
        assert!(source.contains("internal sealed class WireWriter"));
        assert!(source.contains("internal static extern void FreeBuf(FfiBuf buffer);"));
        assert!(source.contains("[In] byte[] nameBytes, nuint nameLength"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_direct_vectors_as_arrays() {
        let bindings = bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
                pub y: i32,
            }

            #[export]
            pub fn echo_numbers(values: Vec<i32>) -> Vec<i32> { values }

            #[export]
            pub fn echo_flags(values: Vec<bool>) -> Vec<bool> { values }

            #[export]
            pub fn echo_points(values: Vec<Point>) -> Vec<Point> { values }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("direct vectors should render");

        let source = file(&output, "Demo.cs");
        assert!(source.contains("public static int[] EchoNumbers(int[] values)"));
        assert!(source.contains("NativeEchoNumbers(values, (nuint)values.Length)"));
        assert!(source.contains("return resultReader.ReadRawArray<int>();"));
        assert!(source.contains("public static bool[] EchoFlags(bool[] values)"));
        assert!(source.contains(
            "[MarshalAs(UnmanagedType.LPArray, ArraySubType = UnmanagedType.U1)] [In] bool[] values"
        ));
        assert!(source.contains("return resultReader.ReadRawBoolArray();"));
        assert!(source.contains("public static Point[] EchoPoints(Point[] values)"));
        assert!(source.contains(
            "NativeEchoPoints(values, (nuint)(values.Length * Marshal.SizeOf<Point>()))"
        ));
        assert!(source.contains("return resultReader.ReadRawArray<Point>();"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_passes_mutable_direct_vectors_inout() {
        let bindings = bindings(
            r#"
            #[export]
            pub fn increment(values: &mut [u64]) {
                if let Some(first) = values.first_mut() { *first += 1; }
            }

            #[export]
            pub fn invert(values: &mut [bool]) {
                values.iter_mut().for_each(|value| *value = !*value);
            }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("mutable direct vectors should render");

        let source = file(&output, "Demo.cs");
        assert!(source.contains("public static void Increment(ulong[] values)"));
        assert!(source.contains("[In, Out] ulong[] values"));
        assert!(source.contains("NativeIncrement(values, (nuint)values.Length)"));
        assert!(source.contains("public static void Invert(bool[] values)"));
        assert!(source.contains(
            "[MarshalAs(UnmanagedType.LPArray, ArraySubType = UnmanagedType.U1)] [In, Out] bool[] values"
        ));
        assert!(!source.contains("valuesOut"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_direct_closure_parameters() {
        let bindings = bindings(
            r#"
            #[repr(u8)]
            #[data]
            pub enum Mode { Fast = 1, Slow = 2 }

            #[export]
            pub fn apply(f: impl Fn(i32) -> i32, value: i32) -> i32 { f(value) }

            #[export]
            pub fn notify(f: impl Fn(bool), value: bool) { f(value) }

            #[export]
            pub fn map_mode(f: impl Fn(Mode) -> Mode, value: Mode) -> Mode { f(value) }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("direct closures should render");

        let source = file(&output, "Demo.cs");
        assert!(source.contains("public static int Apply(global::System.Func<int, int> f"));
        assert!(source.contains("public static void Notify(global::System.Action<bool> f"));
        assert!(source.contains("global::System.Func<Mode, Mode> f"));
        assert!(source.contains("GCHandle.Alloc(f)"));
        assert!(source.contains("GCHandle.FromIntPtr(context).Target!"));
        assert!(source.contains("GCHandle.FromIntPtr(context).Free();"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_direct_vector_closure_parameters() {
        let bindings = bindings(
            r#"
            #[export]
            pub fn apply_values(f: impl Fn(Vec<i32>) -> i32, values: Vec<i32>) -> i32 {
                f(values)
            }

            #[export]
            pub fn apply_flags(f: impl Fn(Vec<bool>) -> bool, values: Vec<bool>) -> bool {
                f(values)
            }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("direct-vector closures should render");

        let source = file(&output, "Demo.cs");
        assert!(source.contains("global::System.Func<int[], int> f"));
        assert!(source.contains("global::System.Func<bool[], bool> f"));
        assert!(source.contains("ReadRawArray<int>()"));
        assert!(source.contains("ReadRawBoolArray()"));
        assert!(source.contains("Unsafe.SizeOf<int>()"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_encoded_closure_parameters() {
        let bindings = bindings(
            r#"
            #[data]
            pub struct Point { pub x: i32, pub y: i32 }

            #[export]
            pub fn apply_point(f: impl Fn(Point) -> Point, value: Point) -> Point { f(value) }

            #[export]
            pub fn apply_string(f: impl Fn(String) -> String, value: String) -> String { f(value) }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("encoded closures should render");

        let source = file(&output, "Demo.cs");
        assert!(source.contains("global::System.Func<Point, Point> f"));
        assert!(source.contains("global::System.Func<string, string> f"));
        assert!(source.contains("Point.Decode(boltffi"));
        assert!(source.contains("return FfiBuf.FromBytes(boltffiReturnWriter.ToArray());"));
        assert!(source.contains("internal static extern FfiBuf BufFromBytes"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_fallible_closure_parameters() {
        let bindings = bindings(
            r#"
            #[error]
            pub enum MathError { Invalid }

            #[export]
            pub fn apply(
                f: impl Fn(i32) -> Result<i32, MathError>,
                value: i32,
            ) -> Result<i32, MathError> {
                f(value)
            }

            #[export]
            pub fn apply_bool(
                f: impl Fn() -> Result<bool, MathError>,
            ) -> Result<bool, MathError> {
                f()
            }

            #[export]
            pub fn apply_message(
                f: impl Fn(i32) -> Result<i32, String>,
                value: i32,
            ) -> Result<i32, String> {
                f(value)
            }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("fallible closures should render");

        let source = file(&output, "Demo.cs");
        assert!(source.contains("global::System.Func<int, int> f"));
        assert!(source.contains("out int return_out"));
        assert!(source.contains(
            "[global::System.Runtime.InteropServices.MarshalAs(global::System.Runtime.InteropServices.UnmanagedType.I1)] out bool return_out"
        ));
        assert!(source.contains("catch (MathErrorException boltffiError)"));
        assert!(source.contains("catch (global::System.Exception boltffiError)"));
        assert!(source.contains("return FfiBuf.FromBytes(boltffiErrorWriter.ToArray());"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_stream_delivery_modes() {
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
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("streams should render");

        let source = file(&output, "Demo.cs");
        assert!(source.contains("IAsyncEnumerable<int> Values(this Engine self"));
        assert!(source.contains("MessagesSubscription Messages(this Engine self)"));
        assert!(source.contains("TicksCancellable Ticks(this Engine self"));
        assert!(source.contains("EnumeratorCancellation"));
        assert!(source.contains("NativeValuesPopBatch"));
        assert!(source.contains("NativeMethods.FreeBuf(buffer);"));
        assert!(source.contains(
            "await foreach (var item in ReadAll(subscription, cancellation.Token)) callback(item);"
        ));
        let callback_subscribe = source
            .find("ulong subscription = NativeMethods.NativeTicksSubscribe(receiver);")
            .expect("callback stream should subscribe synchronously");
        let callback_task = source[callback_subscribe..]
            .find("global::System.Threading.Tasks.Task.Run(async () =>")
            .expect("callback stream should start its delivery task")
            + callback_subscribe;
        assert!(callback_subscribe < callback_task);
        assert!(
            !source.contains("ReadAll(subscription, cancellation.Token).ConfigureAwait(false)")
        );
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_inline_and_accessor_constants() {
        let bindings = bindings(
            r#"
            #[repr(u8)]
            #[data]
            pub enum Mode { Fast = 1, Slow = 2 }

            #[export]
            pub const ENABLED: bool = true;
            #[export]
            pub const LIMIT: u32 = 1024;
            #[export]
            pub const HALF: f64 = 0.5;
            #[export]
            pub const GREETING: &'static str = "hello";
            #[export]
            pub const DEFAULT_MODE: Mode = Mode::Fast;

            #[data]
            pub enum State { Idle, Busy(u32) }

            #[export]
            pub const DEFAULT_STATE: State = State::Idle;
            #[export]
            pub const NATIVE_OFFSET: isize = -7;
            #[export]
            pub const NATIVE_LIMIT: usize = 9;
            #[export]
            pub const MAGIC: &'static [u8] = b"ffi";
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("constants should render");

        let source = file(&output, "Demo.cs");
        assert!(source.contains("public const bool Enabled = true;"));
        assert!(source.contains("public const uint Limit = 1024U;"));
        assert!(source.contains("public const double Half = 0.5;"));
        assert!(source.contains("public const string Greeting = \"hello\";"));
        assert!(source.contains("public const Mode DefaultMode = Mode.Fast;"));
        assert!(source.contains("public static readonly State DefaultState = new State.Idle();"));
        assert!(
            source.contains("public static readonly nint NativeOffset = unchecked((nint)-7L);")
        );
        assert!(
            source.contains("public static readonly nuint NativeLimit = unchecked((nuint)9UL);")
        );
        assert!(source.contains("public static byte[] Magic"));
        assert!(source.contains("get"));
        assert!(source.contains("NativeMethods.NativeMagic"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_fallible_calls_as_exceptions() {
        let bindings = bindings(
            r#"
            #[error]
            pub enum MathError {
                DivisionByZero,
                Overflow,
            }

            #[error]
            pub struct AppError {
                pub code: i32,
                pub message: String,
            }

            #[export]
            pub fn safe_divide(a: i32, b: i32) -> Result<i32, String> {
                if b == 0 { Err("division by zero".to_string()) } else { Ok(a / b) }
            }

            #[export]
            pub fn checked_add(a: i32, b: i32) -> Result<i32, MathError> {
                a.checked_add(b).ok_or(MathError::Overflow)
            }

            #[export]
            pub fn load_name(valid: bool) -> Result<String, AppError> {
                if valid {
                    Ok("ok".to_string())
                } else {
                    Err(AppError { code: 1, message: "bad".to_string() })
                }
            }

            #[export]
            pub fn validate(valid: bool) -> Result<(), String> {
                if valid { Ok(()) } else { Err("bad".to_string()) }
            }

            #[export]
            pub fn is_enabled() -> Result<bool, String> {
                Ok(true)
            }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("fallible functions should render");

        let source = file(&output, "Demo.cs");
        assert!(source.contains("public static int SafeDivide(int a, int b)"));
        assert!(source.contains("out int boltffiResult"));
        assert!(source.contains("FfiBuf boltffiErrorBuffer = NativeMethods.NativeSafeDivide"));
        assert!(source.contains("throw new BoltException(boltffiErrorReader.ReadString());"));
        assert!(source.contains("return boltffiResult;"));
        assert!(source.contains("public static string LoadName(bool valid)"));
        assert!(source.contains("out FfiBuf boltffiResultBuffer"));
        assert!(
            source.contains("throw new AppErrorException(AppError.Decode(boltffiErrorReader));")
        );
        assert!(source.contains("return resultReader.ReadString();"));
        assert!(source.contains("public static void Validate(bool valid)"));
        assert!(source.contains("[MarshalAs(UnmanagedType.I1)] out bool boltffiResult"));

        let math_error = file(&output, "MathError.cs");
        assert!(math_error.contains("public sealed class MathErrorException"));
        assert!(math_error.contains("public MathError Error { get; }"));
        let app_error = file(&output, "AppError.cs");
        assert!(app_error.contains("public sealed class AppErrorException"));
        assert!(app_error.contains("base(error.Message)"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_class_handles_and_methods() {
        let bindings = bindings(
            r#"
            pub struct Counter {
                value: i32,
            }

            #[export]
            impl Counter {
                pub fn new(value: i32) -> Self { Self { value } }
                pub fn with_default() -> Self { Self { value: 10 } }
                pub fn try_new(value: i32) -> Result<Self, String> {
                    if value < 0 { Err("negative".to_string()) } else { Ok(Self { value }) }
                }
                pub fn get(&self) -> i32 { self.value }
                pub fn increment(&self) { }
                pub fn add(a: i32, b: i32) -> i32 { a + b }
            }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("classes should render");

        let class = file(&output, "Counter.cs");
        assert!(class.contains("public sealed class Counter : global::System.IDisposable"));
        assert!(class.contains("public Counter(int value)"));
        assert!(class.contains(": this(BoltFfiNew(value).TakeHandle())"));
        assert!(class.contains("private static Counter BoltFfiNew(int value)"));
        assert!(class.contains("public static Counter WithDefault()"));
        assert!(class.contains("public static Counter TryNew(int value)"));
        assert!(class.contains("public int Get()"));
        assert!(class.contains("public void Increment()"));
        assert!(class.contains("public static int Add(int a, int b)"));
        assert!(class.contains("ThrowIfDisposed();"));
        assert!(class.contains("~Counter() => Release();"));

        let module = file(&output, "Demo.cs");
        assert!(module.contains("NativeCounterRelease(ulong handle)"));
        assert!(module.contains("NativeCounterNew(int value)"));
        assert!(module.contains("NativeCounterGet(ulong receiver)"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_async_class_new_as_static_factory() {
        let bindings = bindings(
            r#"
            pub struct AsyncClient {
                endpoint: String,
            }

            #[export]
            impl AsyncClient {
                pub async fn new(endpoint: String) -> Self { Self { endpoint } }
                pub fn endpoint(&self) -> String { self.endpoint.clone() }
            }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("async class initializer should render");

        let class = file(&output, "AsyncClient.cs");
        assert!(class.contains(
            "public static global::System.Threading.Tasks.Task<AsyncClient> New(string endpoint, global::System.Threading.CancellationToken cancellationToken = default)"
        ));
        assert!(!class.contains("public AsyncClient(string endpoint)"));
        assert!(!class.contains("BoltFfiNew"));
        assert!(!class.contains(".TakeHandle()"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_encoded_records_from_codec_plans() {
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
                pub aliases: Vec<String>,
                pub location: Point,
                pub outcome: Result<i32, String>,
            }

            #[data(impl)]
            impl Profile {
                pub fn alias_count(&self) -> usize { self.aliases.len() }
                pub fn clear_aliases(&mut self) { self.aliases.clear(); }
            }

            #[export]
            pub fn echo_profile(profile: Profile) -> Profile { profile }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("encoded records should render");

        let profile = file(&output, "Profile.cs");
        assert!(profile.contains("public readonly record struct Profile("));
        assert!(profile.contains("string Name,"));
        assert!(profile.contains("string[] Aliases,"));
        assert!(profile.contains("Point Location"));
        assert!(profile.contains("BoltFFIResult<int, string> Outcome"));
        assert!(profile.contains("internal static Profile Decode(WireReader reader)"));
        assert!(profile.contains("reader.ReadString()"));
        assert!(profile.contains("reader.ReadArray(reader => reader.ReadString())"));
        assert!(profile.contains("Point.Decode(reader)"));
        assert!(profile.contains(
            "reader.ReadResult(reader => reader.ReadI32(), reader => reader.ReadString())"
        ));
        assert!(profile.contains("writer.WriteString(this.Name);"));
        assert!(profile.contains("this.Location.Encode(writer);"));
        assert!(profile.contains("if (this.Outcome.IsOk)"));
        assert!(profile.contains("public nuint AliasCount()"));
        assert!(profile.contains("this.Encode(boltffiReceiverWriter);"));
        assert!(profile.contains("public Profile ClearAliases()"));
        assert!(profile.contains("out FfiBuf boltffiReceiverOut"));
        assert!(profile.contains("return Profile.Decode(boltffiReceiverReader);"));

        let point = file(&output, "Point.cs");
        assert!(point.contains("[StructLayout(LayoutKind.Sequential)]"));
        assert!(point.contains("internal static Point Decode(WireReader reader)"));
        assert!(point.contains("writer.WriteI32(this.X);"));

        let module = file(&output, "Demo.cs");
        assert!(module.contains("public static Profile EchoProfile(Profile profile)"));
        assert!(module.contains("profile.Encode(profileWriter);"));
        assert!(module.contains("return Profile.Decode(resultReader);"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_data_enums_from_codec_plans() {
        let bindings = bindings(
            r#"
            #[data]
            pub enum Shape {
                Empty,
                Circle { radius: f64 },
                Label(String),
            }

            #[data(impl)]
            impl Shape {
                pub fn is_empty(&self) -> bool { matches!(self, Self::Empty) }
                pub fn reset(&mut self) { *self = Self::Empty; }
            }

            #[export]
            pub fn echo_shape(shape: Shape) -> Shape { shape }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("data enums should render");

        let shape = file(&output, "Shape.cs");
        assert!(shape.contains("public abstract record Shape"));
        assert!(shape.contains("internal static Shape Decode(WireReader reader)"));
        assert!(shape.contains("0 => new Empty()"));
        assert!(shape.contains("1 => new Circle(reader.ReadF64())"));
        assert!(shape.contains("2 => new Label(reader.ReadString())"));
        assert!(shape.contains("case Circle value:"));
        assert!(shape.contains("writer.WriteF64(value.Radius);"));
        assert!(shape.contains("public sealed record Circle(double Radius) : Shape;"));
        assert!(shape.contains("public sealed record Label(string Field0) : Shape;"));
        assert!(shape.contains("public bool IsEmpty()"));
        assert!(shape.contains("this.Encode(boltffiReceiverWriter);"));
        assert!(shape.contains("public global::Demo.Shape Reset()"));
        assert!(shape.contains("return global::Demo.Shape.Decode(boltffiReceiverReader);"));

        let module = file(&output, "Demo.cs");
        assert!(module.contains("public static Shape EchoShape(Shape shape)"));
        assert!(module.contains("shape.Encode(shapeWriter);"));
        assert!(module.contains("return Shape.Decode(resultReader);"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_xml_documentation() {
        let bindings = bindings(
            r#"
            use boltffi::EventSubscription;
            use std::sync::Arc;

            /// A profile wrapping Vec<String> & friends.
            #[data]
            pub struct Profile {
                /// The display name.
                pub name: String,
            }

            #[data(impl)]
            impl Profile {
                /// Returns the display-name length.
                pub fn name_len(&self) -> usize { self.name.len() }
            }

            /// Available modes.
            #[repr(u8)]
            #[data]
            pub enum Mode {
                /// Runs quickly.
                Fast = 1,
                /// Runs carefully.
                Slow = 2,
            }

            /// A job state.
            #[data]
            pub enum State {
                /// No work is active.
                Idle,
                /// Work is active.
                Busy {
                    /// Number of active jobs.
                    jobs: u32,
                },
            }

            pub struct Counter { value: i32 }

            /// Mutable counter held over FFI.
            #[export]
            impl Counter {
                /// Creates a counter.
                pub fn new(value: i32) -> Self { Self { value } }

                /// Returns the current value.
                pub fn get(&self) -> i32 { self.value }

                /// Streams observed values.
                #[ffi_stream(item = i32)]
                pub fn values(&self) -> Arc<EventSubscription<i32>> { loop {} }
            }

            /// Echoes a profile.
            #[export]
            pub fn echo_profile(profile: Profile) -> Profile { profile }

            /// The default answer.
            #[export]
            pub const ANSWER: u32 = 42;
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("C# XML documentation should render");

        let profile = file(&output, "Profile.cs");
        assert!(profile.contains(
            "/// A profile wrapping Vec&lt;String&gt; &amp; friends.\n    /// </summary>"
        ));
        assert!(profile.contains(
            "/// <param name=\"Name\">The display name.</param>\n    public readonly record struct Profile"
        ));
        assert!(profile.contains(
            "/// Returns the display-name length.\n        /// </summary>\n        public nuint NameLen()"
        ));

        let mode = file(&output, "Mode.cs");
        assert!(mode.contains("/// Available modes.\n    /// </summary>\n    public enum Mode"));
        assert!(mode.contains("/// Runs quickly.\n        /// </summary>\n        Fast = 1"));

        let state = file(&output, "State.cs");
        assert!(state.contains("/// A job state.\n    /// </summary>"));
        assert!(state.contains("/// No work is active.\n        /// </summary>"));
        assert!(state.contains(
            "/// <param name=\"Jobs\">Number of active jobs.</param>\n        public sealed record Busy(uint Jobs)"
        ));

        let counter = file(&output, "Counter.cs");
        assert!(counter.contains("/// Mutable counter held over FFI.\n    /// </summary>"));
        assert!(counter.contains(
            "/// Creates a counter.\n        /// </summary>\n        public Counter(int value)"
        ));
        assert!(counter.contains(
            "/// Returns the current value.\n        /// </summary>\n        public int Get()"
        ));

        let module = file(&output, "Demo.cs");
        assert!(module.contains(
            "/// Echoes a profile.\n        /// </summary>\n        public static Profile EchoProfile"
        ));
        assert!(module.contains(
            "/// The default answer.\n        /// </summary>\n        public const uint Answer = 42U;"
        ));
        assert!(module.contains(
            "/// Streams observed values.\n        /// </summary>\n        public static global::System.Collections.Generic.IAsyncEnumerable<int> Values"
        ));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_builtins_and_custom_mappings() {
        let bindings = bindings(
            r#"
            custom_type!(
                pub Email,
                remote = EmailRust,
                repr = String,
                into_ffi = email_into_ffi,
                try_from_ffi = email_from_ffi
            );

            #[export]
            pub fn keep_email(value: EmailRust) -> EmailRust { value }

            #[export]
            pub fn keep_duration(value: std::time::Duration) -> std::time::Duration { value }

            #[export]
            pub fn keep_time(value: std::time::SystemTime) -> std::time::SystemTime { value }

            #[export]
            pub fn keep_uuid(value: uuid::Uuid) -> uuid::Uuid { value }

            #[export]
            pub fn keep_url(value: url::Url) -> url::Url { value }
            "#,
        );
        let output = target(CSharpHost::new().custom_mapping(
            "Email",
            CSharpCustomMapping::url_string("global::System.Uri"),
        ))
        .render(&bindings)
        .expect("builtins and custom mappings should render");

        let source = file(&output, "Demo.cs");
        assert!(
            source.contains("public static global::System.Uri KeepEmail(global::System.Uri value)")
        );
        assert!(source.contains("valueWriter.WriteString(value.ToString());"));
        assert!(source.contains("return new global::System.Uri(resultReader.ReadString());"));
        assert!(source.contains("global::System.TimeSpan KeepDuration"));
        assert!(source.contains("valueWriter.WriteDuration(value);"));
        assert!(source.contains("resultReader.ReadDuration()"));
        assert!(source.contains("global::System.DateTime KeepTime"));
        assert!(source.contains("global::System.Guid KeepUuid"));
        assert!(source.contains("global::System.Uri KeepUrl"));
        assert!(output.diagnostics().is_empty());
    }

    #[test]
    fn csharp_target_renders_direct_records_and_c_style_enums() {
        let bindings = bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
                pub y: i32,
            }

            #[repr(u8)]
            #[data]
            pub enum Mode {
                Fast = 1,
                Slow = 2,
            }

            #[export]
            pub fn echo_point(point: Point) -> Point { point }

            #[export]
            pub fn echo_mode(mode: Mode) -> Mode { mode }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("direct declarations should render");

        insta::assert_snapshot!(file(&output, "Point.cs"), @r###"
        // <auto-generated>
        // This file was generated by BoltFFI. Do not edit.
        // </auto-generated>
        #nullable enable

        using System.Runtime.InteropServices;

        namespace Demo
        {
            [StructLayout(LayoutKind.Sequential)]
            public readonly record struct Point(
                int X,
                int Y
            )
            {
                internal static Point Decode(WireReader reader) =>
                    new Point(
                        reader.ReadI32(),
                        reader.ReadI32()
                    );

                internal void Encode(WireWriter writer)
                {
                    {
                        writer.WriteI32(this.X);
                    }
                    {
                        writer.WriteI32(this.Y);
                    }
                }

            }
        }
        "###);
        insta::assert_snapshot!(file(&output, "Mode.cs"), @r###"
        // <auto-generated>
        // This file was generated by BoltFFI. Do not edit.
        // </auto-generated>
        #nullable enable

        namespace Demo
        {
            public enum Mode : byte
            {
                Fast = 1,
                Slow = 2
            }
        }
        "###);
        let module = file(&output, "Demo.cs");
        assert!(module.contains("public static Point EchoPoint(Point point)"));
        assert!(module.contains("internal static extern Point NativeEchoPoint(Point point);"));
        assert!(module.contains("public static Mode EchoMode(Mode mode)"));
        assert!(module.contains("internal static extern Mode NativeEchoMode(Mode mode);"));
    }

    #[test]
    fn csharp_target_renders_direct_value_initializers_and_methods() {
        let bindings = bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: f64,
                pub y: f64,
            }

            #[data(impl)]
            impl Point {
                pub fn new(x: f64, y: f64) -> Self { Self { x, y } }
                pub fn origin() -> Self { Self { x: 0.0, y: 0.0 } }
                pub fn distance(&self) -> f64 { 0.0 }
                pub fn scale(&mut self, factor: f64) { self.x *= factor; self.y *= factor; }
                pub fn add(&self, other: Point) -> Point { other }
                pub fn copy_from(&self, other: &Point) -> Point { *other }
                pub fn dimensions() -> u32 { 2 }
            }

            #[repr(u8)]
            #[data]
            pub enum Mode {
                Fast = 1,
                Slow = 2,
            }

            #[data(impl)]
            impl Mode {
                pub fn new(raw: u8) -> Self { if raw == 1 { Self::Fast } else { Self::Slow } }
                pub fn count() -> u32 { 2 }
                pub fn opposite(&self) -> Self { Self::Fast }
                pub fn is_fast(&self) -> bool { true }
            }
            "#,
        );
        let output = target(CSharpHost::new())
            .render(&bindings)
            .expect("direct value methods should render");

        let point = file(&output, "Point.cs");
        assert!(point.contains("public static Point New(double x, double y)"));
        assert!(point.contains("public static Point Origin()"));
        assert!(point.contains("public double Distance()"));
        assert!(point.contains("public Point Scale(double factor)"));
        assert!(point.contains("out Point receiverOut"));
        assert!(point.contains("return receiverOut;"));
        assert!(point.contains("public Point Add(Point other)"));
        assert!(point.contains("public Point CopyFrom(Point other)"));
        assert!(point.contains("public static uint Dimensions()"));

        let mode = file(&output, "Mode.cs");
        assert!(mode.contains("public static class ModeMethods"));
        assert!(mode.contains("public static Mode New(byte raw)"));
        assert!(mode.contains("public static uint Count()"));
        assert!(mode.contains("public static Mode Opposite(this Mode self)"));
        assert!(mode.contains("public static bool IsFast(this Mode self)"));

        let module = file(&output, "Demo.cs");
        assert!(module.contains("NativePointDistance(Point receiver)"));
        assert!(
            module
                .contains("NativePointScale(Point receiver, out Point receiverOut, double factor)")
        );
        assert!(module.contains("NativePointCopyFrom(Point receiver, in Point other)"));
        assert!(module.contains("NativeModeOpposite(Mode receiver)"));
        assert!(output.diagnostics().is_empty());
    }
}
