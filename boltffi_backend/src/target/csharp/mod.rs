//! C# target rendered through .NET P/Invoke over the C ABI bridge.

mod name_style;
mod render;
mod syntax;

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    Native, RecordDecl, StreamDecl,
};

use crate::{
    bridge::c::{CBridge, CBridgeContract},
    core::{
        BindingCapability, BridgeCapability, CapabilityRequirements, Emitted, Error,
        GeneratedOutput, HostCapabilities, RenderContext, RenderedDeclaration, Result, Target,
        contract::sealed, host,
    },
};

use name_style::{Name, Namespace};
use syntax::Literal;

pub use syntax::{ArgumentList, Expression, Identifier, Statement, Syntax, TypeFragment};

/// C# host renderer for direct P/Invoke calls into the BoltFFI C ABI.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct CSharpHost {
    namespace: Option<Namespace>,
    library: Option<String>,
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

    /// Creates the backend target stack for this C# host.
    pub fn into_target(self) -> Result<Target<Self, CBridge>> {
        Ok(Target::new(self, CBridge::default_header()?))
    }

    fn namespace_for<'bindings>(&'bindings self, bindings: &Bindings<Native>) -> Result<Namespace> {
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
            .unsupported(BindingCapability::Classes, "C# classes have not migrated")
            .unsupported(
                BindingCapability::Callbacks,
                "C# callbacks have not migrated",
            )
            .unsupported(BindingCapability::Streams, "C# streams have not migrated")
            .unsupported(
                BindingCapability::Constants,
                "C# constants have not migrated",
            )
            .unsupported(
                BindingCapability::CustomTypes,
                "C# custom types have not migrated",
            )
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        CapabilityRequirements::new().require(BridgeCapability::CAbi)
    }

    fn record(
        &self,
        decl: &RecordDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Record::from_declaration(decl, self.namespace_for(context.bindings())?, bridge)?
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
        _decl: &ClassDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("classes")
    }

    fn callback(
        &self,
        _decl: &CallbackDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("callbacks")
    }

    fn stream(
        &self,
        _decl: &StreamDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("streams")
    }

    fn constant(
        &self,
        _decl: &ConstantDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("constants")
    }

    fn custom_type(
        &self,
        _decl: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("custom types")
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

    use super::CSharpHost;

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

    fn file<'output>(output: &'output GeneratedOutput, path: impl AsRef<Path>) -> &'output str {
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
    fn csharp_partial_coverage_keeps_supported_functions() {
        let bindings = bindings(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 { left + right }

            #[export]
            pub fn greet(name: String) -> String { name }
            "#,
        );
        let output = target(CSharpHost::new())
            .render_partial(&bindings)
            .expect("partial C# render");

        let source = file(&output, "Demo.cs");
        assert!(source.contains("public static int Add(int left, int right)"));
        assert!(!source.contains("Greet"));
        let [unsupported] = output.coverage().unsupported() else {
            panic!("expected one unsupported declaration")
        };
        assert_eq!(unsupported.declaration().name(), "greet");
        assert_eq!(unsupported.reason(), "non-primitive function parameters");
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
            );
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
}
