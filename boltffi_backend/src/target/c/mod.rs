//! C host renderer (experimental).
//!
//! This is the C host scaffolding every renderer hangs off. C calls the shared
//! C ABI (`CBridge`) directly, so unlike a runtime bridge there is no extra
//! bridge layer stacked on top. The host's syntax fragments are the C fragments
//! the bridge already emits (`crate::bridge::c`).

mod codec;
pub mod name_style;
mod render;
pub mod syntax;
mod values;

pub use self::syntax::Syntax;

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, DeclarationRef, EnumDecl,
    FunctionDecl, Native, RecordDecl, StreamDecl,
};

use crate::{
    bridge::c::CBridge,
    core::{
        BindingCapability, BridgeCapability, CapabilityRequirements, CoverageMode, CoverageReport,
        DeclarationLabel, Emitted, Error, GeneratedOutput, HostCapabilities, RenderContext,
        RenderedDeclaration, Result, Target, UnsupportedDeclaration, contract::sealed, host,
    },
};

/// C host renderer paired with the shared C ABI bridge.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct CHost;

impl CHost {
    /// Creates a C host renderer.
    pub fn new() -> Self {
        Self
    }

    /// Creates the backend target stack for this C host.
    ///
    /// C calls the C ABI directly, so the stack is just `CBridge` with no
    /// additional bridge layer.
    pub fn into_target(self, _bindings: &Bindings<Native>) -> Result<Target<Self, CBridge>> {
        Ok(Target::new(self, CBridge::default_header()?))
    }
}

impl host::HostBackend for CHost {
    type Surface = Native;
    type Bridge = crate::bridge::c::CBridgeContract;
    type Syntax = Syntax;

    fn name(&self) -> &'static str {
        "c"
    }

    fn binding_capabilities(&self) -> HostCapabilities {
        HostCapabilities::new()
            .stable(BindingCapability::Records)
            .stable(BindingCapability::Enums)
            .stable(BindingCapability::Functions)
            .stable(BindingCapability::Classes)
            .stable(BindingCapability::Callbacks)
            .unsupported(BindingCapability::Streams, "not yet implemented in C host")
            .stable(BindingCapability::Constants)
            .stable(BindingCapability::CustomTypes)
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        // C calls the ABI directly; only the C ABI surface is required.
        CapabilityRequirements::new().require(BridgeCapability::CAbi)
    }

    fn record(
        &self,
        decl: &RecordDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::record::render(decl, bridge, context)
    }

    fn enumeration(
        &self,
        decl: &EnumDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::enumeration::render(decl, bridge, context)
    }

    fn function(
        &self,
        decl: &FunctionDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::function::render(decl, bridge, context)
    }

    fn class(
        &self,
        decl: &ClassDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::class::render(decl, bridge, context)
    }

    fn callback(
        &self,
        decl: &CallbackDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::callback::render(decl, bridge, context)
    }

    fn stream(
        &self,
        _decl: &StreamDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: "c",
            shape: "stream",
        })
    }

    fn constant(
        &self,
        decl: &ConstantDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::constant::render(decl, bridge, context)
    }

    fn custom_type(
        &self,
        _: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(Emitted::primary(""))
    }

    fn assemble<'decl>(
        &self,
        bindings: &Bindings<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
        mut declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        let mut coverage = CoverageReport::new();
        let mut record_definitions = std::collections::BTreeMap::new();
        let mut rendered: std::collections::HashSet<boltffi_binding::DeclarationId> = declarations
            .iter()
            .map(|declaration| declaration.declaration().id())
            .collect();
        declarations.iter().try_for_each(|declaration| {
            let DeclarationRef::Record(RecordDecl::Encoded(record)) = declaration.declaration()
            else {
                return Ok(());
            };
            let definition = render::surface::record_codec(record, context);
            match definition {
                Ok(definition) => {
                    record_definitions.insert(record.id(), definition);
                }
                Err(Error::UnsupportedTarget { shape, .. })
                    if matches!(context.coverage_mode(), CoverageMode::Partial) =>
                {
                    rendered.remove(&declaration.declaration().id());
                    coverage.push(UnsupportedDeclaration::new(
                        DeclarationLabel::from_ref(declaration.declaration()),
                        shape,
                    ));
                }
                Err(error) => return Err(error),
            }
            Ok(())
        })?;
        declarations.retain(|declaration| rendered.contains(&declaration.declaration().id()));
        loop {
            let previous_count = declarations.len();
            declarations = declarations
                .into_iter()
                .filter_map(|declaration| {
                    let mut references = std::collections::BTreeSet::new();
                    declaration
                        .declaration()
                        .append_referenced_declarations(&mut references);
                    if references
                        .iter()
                        .all(|reference| rendered.contains(reference))
                    {
                        return Some(Ok(declaration));
                    }
                    let shape = "depends on a declaration without a C binding";
                    if !matches!(context.coverage_mode(), CoverageMode::Partial) {
                        return Some(Err(Error::UnsupportedTarget { target: "c", shape }));
                    }
                    coverage.push(UnsupportedDeclaration::new(
                        DeclarationLabel::from_ref(declaration.declaration()),
                        shape,
                    ));
                    rendered.remove(&declaration.declaration().id());
                    None
                })
                .collect::<Result<Vec<_>>>()?;
            if declarations.len() == previous_count {
                break;
            }
        }
        declarations.sort_by_key(|declaration| {
            !matches!(declaration.declaration(), DeclarationRef::Callback(_))
        });
        let emitted = declarations
            .into_iter()
            .map(|declaration| declaration.into_parts().1)
            .collect::<Vec<_>>();
        // The host appends its ergonomic wrappers to the same `boltffi.h`
        // header the C ABI bridge produced, so both layers combine into one
        // consumable header. The bridge closes its own C++ linkage block before
        // this appended layer, so open a second block for the facade.
        let file = crate::core::FilePlan::all(crate::core::FilePath::new("boltffi.h")?)
            .with_preamble(format!(
                "\n{}\n#ifdef __cplusplus\nextern \"C\" {{\n#endif\n",
                render::surface::render(bindings, context, &rendered, &record_definitions)?,
            ))
            .with_postamble("\n#ifdef __cplusplus\n}\n#endif\n");
        let output = crate::core::FileLayout::new()
            .with_file(file)
            .assemble(emitted)?;
        Ok(output.with_coverage(coverage))
    }
}

impl sealed::HostBackend for CHost {}

#[cfg(test)]
mod tests {
    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Bindings, Native, lower};

    use crate::{
        core::{BindingCapability, CapabilityStatus, host::HostBackend},
        target::c::CHost,
    };

    fn bindings(source: &str) -> Bindings<Native> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(source).expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source should scan");
        lower::<Native>(&source).expect("source should lower")
    }

    fn render_header(output: &crate::core::GeneratedOutput) -> String {
        output
            .files()
            .iter()
            .find(|file| file.path().as_path() == std::path::Path::new("boltffi.h"))
            .expect("boltffi.h")
            .contents()
            .to_owned()
    }

    fn empty_bindings() -> Bindings<Native> {
        let source = boltffi_scan::scan_file(
            syn::parse_str("").expect("valid empty source"),
            PackageInfo::new("demo", None),
        )
        .expect("empty source should scan");
        lower::<Native>(&source).expect("empty source should lower")
    }

    #[test]
    fn name_is_c() {
        assert_eq!(CHost::new().name(), "c");
    }

    #[test]
    fn sync_value_capabilities_are_stable() {
        let capabilities = CHost::new().binding_capabilities();
        for capability in [
            BindingCapability::Records,
            BindingCapability::Enums,
            BindingCapability::Functions,
            BindingCapability::Constants,
            BindingCapability::Classes,
            BindingCapability::Callbacks,
            BindingCapability::CustomTypes,
        ] {
            assert!(
                capabilities.status(capability).is_stable(),
                "capability {capability:?} should be stable for the C host"
            );
        }
        assert!(
            matches!(
                capabilities.status(BindingCapability::Streams),
                CapabilityStatus::Unsupported { .. }
            ),
            "streams should be unsupported for the C host"
        );
    }

    #[test]
    fn renders_primitive_function_wrapper() {
        let bindings = bindings(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }
            "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let output = target.render(&bindings).expect("render");
        let header = output
            .files()
            .iter()
            .find(|file| file.path().as_path() == std::path::Path::new("boltffi.h"))
            .expect("boltffi.h")
            .contents();
        assert!(header.contains("static inline int32_t demo_add(int32_t left, int32_t right) {"));
        assert!(header.contains("boltffi_function_demo_add(left, right)"));
    }

    #[test]
    fn renders_static_record_and_enum_methods_without_receivers() {
        let bindings = bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point { pub x: f64 }

            #[data]
            pub struct Person { pub name: String }

            #[repr(u8)]
            #[data]
            pub enum Mode { Fast = 1, Slow = 2 }

            #[data(impl)]
            impl Point {
                pub fn dimensions() -> u32 { 1 }
            }

            #[data(impl)]
            impl Person {
                pub fn minimum_age() -> u32 { 0 }
            }

            #[data(impl)]
            impl Mode {
                pub fn count() -> u32 { 2 }
            }
            "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let header = render_header(&target.render(&bindings).expect("render"));
        assert!(header.contains("static inline uint32_t demo_point_dimensions(void)"));
        assert!(header.contains("static inline uint32_t demo_person_minimum_age(void)"));
        assert!(header.contains("static inline uint32_t demo_mode_count(void)"));
    }

    #[test]
    fn nested_record_maps_have_complete_c_types_and_cleanup() {
        let bindings = bindings(
            r#"
            #[data]
            pub struct Settings { pub entries: std::collections::HashMap<String, bool> }

            #[data]
            pub struct Profile { pub settings: Settings }

            #[export]
            pub fn echo_profile(profile: Profile) -> Profile { profile }

            #[export]
            pub fn add(left: i32, right: i32) -> i32 { left + right }
            "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let output = target.render(&bindings).expect("render nested maps");
        let header = render_header(&output);
        assert!(header.contains("static inline int32_t demo_add("));
        assert!(header.contains("DemoSettings"));
        assert!(header.contains("DemoProfile"));
        assert!(header.contains("static inline DemoProfile demo_echo_profile("));
        assert!(header.contains("demo_profile_free"));
        assert!(output.coverage().unsupported().is_empty());
    }

    #[test]
    fn partial_coverage_removes_types_that_depend_on_skipped_declarations() {
        let bindings = bindings(
            r#"
            #[data]
            pub enum Mode { Ready }
            #[data(impl)]
            impl Mode {
                pub async fn next(&self) -> Self { Self::Ready }
            }
            #[data]
            pub struct Event { pub mode: Mode, pub text: String }
            #[export]
            pub fn echo(event: Event) -> Event { event }
            #[export]
            pub fn add(left: i32, right: i32) -> i32 { left + right }
        "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let output = target
            .render_partial(&bindings)
            .expect("partial C bindings");
        let header = render_header(&output);
        assert!(header.contains("static inline int32_t demo_add("));
        assert!(!header.contains("typedef ___Mode DemoMode;"));
        assert!(!header.contains("struct DemoEvent"));
        assert!(!header.contains("static inline DemoEvent demo_echo("));
        assert!(target.render(&bindings).is_err());
    }

    #[test]
    fn options_of_vectors_and_vectors_of_options_have_distinct_types() {
        let bindings = bindings(
            r#"
            #[export]
            pub fn optional(values: Option<Vec<i32>>) -> Option<Vec<i32>> { values }
            #[export]
            pub fn elements(values: Vec<Option<i32>>) -> Vec<Option<i32>> { values }
            #[export]
            pub fn nested(values: Vec<Option<Vec<i32>>>) -> Vec<Option<Vec<i32>>> { values }
        "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let output = target.render(&bindings).expect("render nested containers");
        let header = render_header(&output);
        assert!(header.contains(
            "static inline DemoOptionOfI32Sequence demo_optional(DemoOptionOfI32Slice values)"
        ));
        assert!(header.contains(
            "static inline DemoSequenceOfOptionI32 demo_elements(DemoSliceOfOptionI32 values)"
        ));
        assert!(header.contains("static inline DemoSequenceOfOptionOfI32Sequence demo_nested(DemoSliceOfOptionOfI32Slice values)"));
    }

    #[test]
    fn named_records_cannot_shadow_builtin_c_values() {
        let bindings = bindings(
            r#"
            #[data]
            pub struct StringView { pub count: u32 }
        "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let error = target
            .render(&bindings)
            .expect_err("C names must remain unambiguous");
        assert!(error.to_string().contains("DemoStringView"));
    }

    #[test]
    fn prefixes_type_names_by_complete_name_parts() {
        let bindings = bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Demographic { pub count: u32 }

            #[repr(C)]
            #[data]
            pub struct DemoState { pub value: u32 }
            "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let header = render_header(&target.render(&bindings).expect("render"));
        assert!(header.contains("typedef ___Demographic DemoDemographic;"));
        assert!(header.contains("typedef ___DemoState DemoState;"));
        assert!(!header.contains("DemoDemoState"));
    }

    #[test]
    fn nullable_class_handles_accept_null_and_owned_handles_are_cleared() {
        let bindings = bindings(
            r#"
            pub struct Engine;

            #[export]
            impl Engine {
                pub fn new() -> Self { Self }
            }

            #[export]
            pub fn inspect(engine: Option<Engine>) -> bool { engine.is_some() }

            #[export]
            pub fn consume(engine: Engine) {}
            "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let header = render_header(&target.render(&bindings).expect("render"));
        assert!(header.contains("demo_inspect(DemoEngine *engine)"));
        assert!(header.contains("engine == NULL ? 0 : engine->_boltffi_handle"));
        assert!(header.contains("demo_consume(DemoEngine *engine)"));
        assert!(header.contains("engine->_boltffi_handle = 0;"));
    }

    #[test]
    fn c_header_uses_msvc_intrinsics_without_requiring_c11_atomics() {
        let bindings = empty_bindings();
        let target = CHost::new().into_target(&bindings).expect("target");
        let header = render_header(&target.render(&bindings).expect("render"));
        assert!(header.contains("#if defined(_MSC_VER)\n#include <intrin.h>"));
        assert!(
            header.contains(
                "#elif defined(__cplusplus) && (defined(__clang__) || defined(__GNUC__))"
            )
        );
        assert!(!header.contains("#if defined(__cplusplus) && defined(_MSC_VER)"));
    }

    #[test]
    fn into_target_succeeds() {
        let bindings = empty_bindings();
        let target = CHost::new()
            .into_target(&bindings)
            .expect("C host into_target should succeed");
        assert_eq!(target.host().name(), "c");
    }

    #[test]
    fn renders_record_enum_and_constant_surface() {
        let bindings = bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: f64,
                pub y: f64,
            }

            #[repr(u8)]
            #[data]
            pub enum Mode {
                Fast = 1,
                Slow = 2,
            }

            #[export]
            pub const ANSWER: u32 = 42;

            #[export]
            pub fn add(left: i32, right: i32) -> i32 { left + right }
            "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let output = target.render(&bindings).expect("render");
        let header = output
            .files()
            .iter()
            .find(|file| file.path().as_path() == std::path::Path::new("boltffi.h"))
            .expect("boltffi.h")
            .contents()
            .to_owned();
        eprintln!("===== HEADER =====\n{header}\n===================");
        assert!(header.contains("typedef ___Point DemoPoint;"));
        assert!(header.contains("typedef ___Mode DemoMode;"));
        assert!(header.contains("#define DEMO_ANSWER 42"));
        assert!(header.contains("static inline int32_t demo_add(int32_t left, int32_t right) {"));
    }

    /// Compiles the emitted ergonomic header with the system C toolchain
    /// (skipped when none is available). Proves the host wrappers line up with
    /// the locked C ABI by turning the header into a valid object file.
    #[test]
    fn emitted_header_compiles_with_cc() {
        use std::process::Command;
        if Command::new("cc").arg("--version").output().is_err() {
            eprintln!("skipping: no C toolchain");
            return;
        }
        let bindings = bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: f64,
                pub y: f64,
            }

            #[repr(u8)]
            #[data]
            pub enum Mode {
                Fast = 1,
                Slow = 2,
            }

            #[export]
            pub const ANSWER: u32 = 42;

            #[export]
            pub fn add(left: i32, right: i32) -> i32 { left + right }

            #[export]
            pub fn greet(name: String) -> String { name }

            #[export]
            pub fn reserved_local(text: Option<String>, boltffi_size_0: u32) -> u32 { boltffi_size_0 }

            #[data]
            pub struct KeywordFields { pub r#type: String }

            #[export]
            pub fn echo_keywords(value: KeywordFields) -> KeywordFields { value }

            #[export]
            pub fn try_point(x: f64) -> Result<Point, String> {
                Ok(Point { x, y: 0.0 })
            }

            #[export]
            pub fn try_number(value: f64) -> Result<f64, String> { Ok(value) }

            #[repr(C)]
            #[data]
            pub struct Pair {
                pub a: i32,
                pub b: i32,
            }

            #[export]
            pub fn total(pairs: Vec<crate::Pair>) -> u32 { 0 }

            #[export]
            pub fn measure(pairs: &[crate::Pair]) -> u32 { 0 }

            #[repr(i32)]
            #[data]
            pub enum DivisionError { DivideByZero = 1 }

            #[export]
            pub fn divide(value: i32, b: i32) -> Result<i32, DivisionError> {
                if b == 0 { Err(DivisionError::DivideByZero) } else { Ok(value / b) }
            }

            pub struct Engine;

            #[export(single_threaded)]
            impl Engine {
                pub fn new(seed: u64) -> Self { todo!() }
                pub fn score(&self, point: crate::Point) -> u32 { 0 }
            }

            #[export]
            pub fn has_engine(engine: Option<Engine>) -> bool { engine.is_some() }

            #[export]
            pub fn consume_engine(engine: Engine) {}
            "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let output = target.render(&bindings).expect("render");
        let header = output
            .files()
            .iter()
            .find(|file| file.path().as_path() == std::path::Path::new("boltffi.h"))
            .expect("boltffi.h")
            .contents();
        // "total" takes Vec<direct record>: a packed direct vector, whose
        // pointer crosses the ABI as opaque bytes with a byte count. The facade
        // must cast the typed slice pointer and scale the element count; either
        // half missing fails the compile below. "measure" takes a shared slice
        // of the same record, which packs through an encoded slice instead.
        assert!(header.contains(
            "boltffi_function_demo_total((const uint8_t *)pairs.ptr, pairs.len * sizeof(DemoPair))"
        ));

        let dir = std::env::temp_dir().join(format!("boltffi_c_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(dir.join("boltffi.h"), header).expect("write header");
        let source = "\n#include \"boltffi.h\"\nint main(void) { return demo_add(1, 2); }\n";
        std::fs::write(dir.join("demo.c"), source).expect("write source");
        let out = Command::new("cc")
            .current_dir(&dir)
            .args(["-c", "demo.c", "-o", "demo.o", "-Werror"])
            .output()
            .expect("cc runs");
        assert!(
            out.status.success(),
            "cc failed:\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );

        if Command::new("c++").arg("--version").output().is_err() {
            eprintln!("skipping: no C++ toolchain");
            return;
        }
        std::fs::write(
            dir.join("demo.cpp"),
            "\n#include \"boltffi.h\"\nint main() {\n    uint8_t state = 0;\n    uint64_t slot = 0;\n    (void)boltffi_atomic_u8_cas(&state, 0, 1);\n    (void)boltffi_atomic_u64_exchange(&slot, 2);\n    (void)boltffi_atomic_u64_cas(&slot, 2, 3);\n    (void)boltffi_atomic_u64_load(&slot);\n    return FFI_STATUS_INTERNAL_ERROR.code == 100 && demo_add(1, 2) == 3 ? 0 : 1;\n}\n",
        )
        .expect("write C++ source");
        let out = Command::new("c++")
            .current_dir(&dir)
            .args([
                "-std=c++03",
                "-pedantic-errors",
                "-Werror",
                "-c",
                "demo.cpp",
                "-o",
                "demo_cpp.o",
            ])
            .output()
            .expect("c++ runs");
        assert!(
            out.status.success(),
            "c++ failed:\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn renders_class_handle_and_wrappers() {
        let bindings = bindings(
            r#"
            pub struct Engine;

            #[export(single_threaded)]
            impl Engine {
                pub fn new(seed: u64) -> Self { todo!() }
                pub fn score(&self, point: crate::Point) -> u32 { 0 }
                pub fn advance(&mut self, delta: u32) {}
            }

            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: f64,
                pub y: f64,
            }
            "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let output = target.render(&bindings).expect("render");
        let header = render_header(&output);
        eprintln!("===== CLASS HEADER =====\n{header}\n===================");
        assert!(header.contains("typedef struct { uint64_t _boltffi_handle; } DemoEngine;"));
        assert!(header.contains("static inline DemoEngine demo_engine_new(uint64_t seed)"));
        assert!(header.contains(
            "boltffi_result._boltffi_handle = boltffi_init_class_demo_engine_new(seed);"
        ));
        assert!(header.contains(
            "static inline uint32_t demo_engine_score(const DemoEngine *receiver, DemoPoint point)"
        ));
        assert!(header.contains("static inline void demo_engine_free(DemoEngine *value)"));
        assert!(header.contains(
            "static inline void demo_engine_advance(DemoEngine *receiver, uint32_t delta)"
        ));
        assert!(
            header.contains(
                "boltffi_method_class_demo_engine_score(receiver->_boltffi_handle, point)"
            )
        );
        assert!(header.contains("value->_boltffi_handle=0;"));
    }

    #[test]
    fn renders_and_runs_sync_callback_interface() {
        use std::process::Command;

        let bindings = bindings(
            r#"
            #[export]
            pub trait Listener {
                fn notify(&self, code: u32);
                fn on_value(&self, value: u32) -> i64;
            }

            #[export]
            pub fn install(listener: impl Listener, code: u32, value: u32) -> i64 {
                listener.notify(code);
                listener.on_value(value)
            }

            #[export]
            pub fn try_listener() -> Result<Box<dyn Listener>, String> { panic!() }
            "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let output = target.render(&bindings).expect("render");
        let header = render_header(&output);
        assert!(header.contains("typedef ___ListenerVTable DemoListener;"));
        assert!(
            header.contains(
                "typedef struct {\n    BoltFFICallbackHandle raw;\n} DemoListenerHandle;"
            )
        );
        assert!(header.contains("boltffi_register_callback_demo_listener"));
        assert!(header.contains(
            "static inline DemoListenerHandle demo_listener_create(const DemoListener *vtable, uint64_t identity)"
        ));
        assert!(header.contains(
            "int64_t boltffi_function_demo_install(BoltFFICallbackHandle listener, uint32_t code, uint32_t value)"
        ));
        assert!(header.contains(
            "static inline int64_t demo_install(DemoListenerHandle *listener, uint32_t code, uint32_t value)"
        ));
        assert!(header.contains("boltffi_function_demo_install(listener->raw, code, value)"));

        if Command::new("cc").arg("--version").output().is_err() {
            eprintln!("skipping: no C toolchain");
            return;
        }
        let dir =
            std::env::temp_dir().join(format!("boltffi_c_callback_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(dir.join("boltffi.h"), header).expect("write header");
        // This file is link-only ABI plumbing. The caller below deliberately
        // never includes or invokes an unergonomic callback API spelling.
        let abi_shim = r#"
#include "boltffi.h"

static const ___ListenerVTable *registered_listener;

void boltffi_register_callback_demo_listener(const ___ListenerVTable *vtable) {
    registered_listener = vtable;
}

BoltFFICallbackHandle boltffi_create_callback_demo_listener(uint64_t handle) {
    BoltFFICallbackHandle callback = {handle, registered_listener};
    return callback;
}

int64_t boltffi_function_demo_install(BoltFFICallbackHandle listener, uint32_t code, uint32_t value) {
    const ___ListenerVTable *vtable = (const ___ListenerVTable *)listener.vtable;
    vtable->notify(listener.handle, code);
    return vtable->on_value(listener.handle, value);
}
"#;
        // This is the consumer-facing C program. It uses only ergonomic types
        // and helpers; the raw callback carrier stays confined to abi_shim.c.
        let source = r#"
#include "boltffi.h"
#include <stdio.h>

static void listener_free(uint64_t handle) { (void)handle; }
static uint64_t listener_clone(uint64_t handle) { return handle; }
static void listener_notify(uint64_t handle, uint32_t code) {
    (void)handle;
    printf("notify:%u\n", code);
}
static int64_t listener_on_value(uint64_t handle, uint32_t value) {
    (void)handle;
    printf("on_value:%u\n", value);
    return (int64_t)value;
}

int main(void) {
    DemoListener listener = {
        .free = listener_free,
        .clone = listener_clone,
        .notify = listener_notify,
        .on_value = listener_on_value,
    };
    DemoListenerHandle callback = demo_listener_create(&listener, 42);
    return demo_install(&callback, 7, 9) == 9 && callback.raw.handle == 0 ? 0 : 1;
}
"#;
        std::fs::write(dir.join("abi_shim.c"), abi_shim).expect("write ABI shim");
        std::fs::write(dir.join("demo.c"), source).expect("write ergonomic caller");
        let out = Command::new("cc")
            .current_dir(&dir)
            .args(["demo.c", "abi_shim.c", "-o", "demo", "-Werror"])
            .output()
            .expect("cc runs");
        assert!(
            out.status.success(),
            "cc failed:\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let out = Command::new(dir.join("demo"))
            .output()
            .expect("run generated C callback test");
        assert!(
            out.status.success(),
            "callback program failed:\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            "notify:7\non_value:9\n"
        );
    }

    #[test]
    fn rejects_async_callback_interface() {
        let bindings = bindings(
            r#"
            #[export]
            #[allow(async_fn_in_trait)]
            pub trait Listener {
                async fn load(&self, key: u32) -> String;
            }
            "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let error = target
            .render(&bindings)
            .expect_err("async callback trait should be unsupported");
        assert!(format!("{error:?}").contains("async callback"));
    }

    #[test]
    fn renders_typed_result_for_c_style_error() {
        let bindings = bindings(
            r#"
            #[repr(i32)]
            #[data]
            pub enum DivisionError { DivideByZero = 1 }
            #[export]
            pub fn divide(value: i32, b: i32) -> Result<i32, DivisionError> {
                if b == 0 { Err(DivisionError::DivideByZero) } else { Ok(value / b) }
            }
        "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let header = render_header(&target.render(&bindings).expect("render"));
        assert!(header.contains("int32_t value; DemoDivisionError error;"));
        assert!(header.contains("} DemoDivideResult;"));
        assert!(
            header.contains("static inline DemoDivideResult demo_divide(int32_t value, int32_t b)")
        );
        assert!(header.contains(
            "FfiBuf_u8 boltffi_error = boltffi_function_demo_divide(value, b, &boltffi_success);"
        ));
        assert!(!header.contains("boltffi_status_t"));
    }

    #[test]
    fn renders_owned_string_errors_for_fallible_functions() {
        let bindings = bindings(
            r#"
            #[export]
            pub fn parse(value: i32) -> Result<i32, String> {
                if value >= 0 { Ok(value) } else { Err("negative".to_owned()) }
            }
            "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let header = render_header(&target.render(&bindings).expect("render"));

        assert!(header.contains("DemoString error;"));
        assert!(header.contains("&(boltffi_result.data.error)"));
        assert!(header.contains("static inline DemoParseResult demo_parse(int32_t value)"));
    }

    #[test]
    fn renders_fallible_class_initializers_with_owned_string_errors() {
        let bindings = bindings(
            r#"
            pub struct Engine;

            #[export(single_threaded)]
            impl Engine {
                pub fn new(seed: u32) -> Result<Self, String> {
                    if seed == 0 { Err("zero".to_owned()) } else { Ok(Self) }
                }
            }
            "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let header = render_header(&target.render(&bindings).expect("render"));

        assert!(header.contains("} DemoEngineNewResult;"));
        assert!(header.contains("DemoEngine value;"));
        assert!(header.contains("DemoString error;"));
        assert!(
            header.contains("static inline DemoEngineNewResult demo_engine_new(uint32_t seed)")
        );
        assert!(header.contains("boltffi_result.data.value._boltffi_handle=boltffi_success;"));
        assert!(header.contains("&(boltffi_result.data.error)"));
    }

    #[test]
    fn encoded_record_facade_compiles_and_round_trips_at_runtime() {
        use std::process::Command;
        if Command::new("cc").arg("--version").output().is_err() {
            return;
        }
        let bindings = bindings(
            r#"
            #[repr(u8)]
            #[data]
            pub enum Mode { Fast = 1, Slow = 2 }

            #[data]
            pub struct Payload {
                pub name: String,
                pub bytes: Vec<u8>,
                pub count: Option<u32>,
                pub values: Vec<f32>,
                pub mode: Mode,
            }

            #[export]
            pub fn echo_payload(value: Payload) -> Payload { value }

            #[export]
            pub fn echo_string(value: String) -> String { value }

            #[export]
            pub fn replace_text(value: &mut String) { *value = "updated".to_owned(); }

            #[export]
            pub fn fill_bytes(value: &mut [u8]) { value.fill(7); }

            #[data(impl)]
            impl Payload {
                pub fn rename(&mut self) { self.name = "updated".to_owned(); }
            }
        "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let header = render_header(&target.render(&bindings).expect("render"));
        assert!(
            header.contains("static inline DemoPayload demo_echo_payload(DemoPayloadView value)")
        );
        assert!(header.contains("static inline DemoString demo_echo_string(DemoStringView value)"));
        for signature in header
            .split('{')
            .filter(|part| part.contains("static inline") && part.contains("demo_"))
        {
            assert!(
                !signature.contains("FfiBuf_u8"),
                "raw buffer leaked: {signature}"
            );
            assert!(
                !signature.contains("___"),
                "raw typedef leaked: {signature}"
            );
        }

        let dir = std::env::temp_dir().join(format!("boltffi_c_codec_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(dir.join("boltffi.h"), header).expect("header");
        std::fs::write(dir.join("test.c"),r#"
#include "boltffi.h"
#include <math.h>
#include <stdlib.h>
#include <string.h>

FfiBuf_u8 boltffi_buf_with_len(uintptr_t len) {
    FfiBuf_u8 b = {(uint8_t *)malloc(len), len, len, 1}; return b;
}
void boltffi_free_buf(FfiBuf_u8 b) { free(b.ptr); }
FfiBuf_u8 boltffi_function_demo_echo_payload(const uint8_t *ptr, uintptr_t len) {
    FfiBuf_u8 b=boltffi_buf_with_len(len); if (len) memcpy(b.ptr,ptr,len); return b;
}
FfiBuf_u8 boltffi_function_demo_echo_string(const uint8_t *ptr, uintptr_t len) {
    FfiBuf_u8 b=boltffi_buf_with_len(len); if (len) memcpy(b.ptr,ptr,len); return b;
}
FfiStatus boltffi_function_demo_replace_text(const uint8_t *ptr, uintptr_t len, FfiBuf_u8 *out) {
    (void)ptr; (void)len;
    *out = boltffi_buf_with_len(11);
    uint8_t encoded[] = {7, 0, 0, 0, 'u', 'p', 'd', 'a', 't', 'e', 'd'};
    memcpy(out->ptr, encoded, sizeof(encoded));
    return FFI_STATUS_OK;
}
FfiStatus boltffi_function_demo_fill_bytes(uint8_t *ptr, uintptr_t len) {
    memset(ptr, 7, len);
    return FFI_STATUS_OK;
}
FfiStatus boltffi_method_record_demo_payload_rename(const uint8_t *ptr, uintptr_t len, FfiBuf_u8 *out) {
    (void)ptr; (void)len;
    DemoPayloadView replacement = {0};
    replacement.name = demo_string_view("updated", 7);
    *out = boltffi_buf_with_len(boltffi_c_demo_size_payload(&replacement));
    BoltFFICWireWriter writer = {out->ptr, out->len, 0, true};
    boltffi_c_demo_encode_payload(&writer, &replacement);
    return FFI_STATUS_OK;
}
int main(void) {
    uint8_t bytes[] = {1,2,3}; float values[] = {1.5f,2.5f};
    DemoPayloadView input;
    memset(&input,0,sizeof(input));
    input.name=demo_string_view("hello",5);
    input.bytes=demo_bytes_view(bytes,3);
    input.count.has_value=true; input.count.value=42;
    input.values.ptr=values; input.values.len=2;
    input.mode=(DemoMode)1;
    DemoPayload output=demo_echo_payload(input);
    if (output.name.len != 5 || memcmp(output.name.ptr,"hello",5) || output.bytes.len != 3 || output.count.value != 42 || output.values.len != 2 || fabsf(output.values.ptr[1]-2.5f)>.001f) return 1;
    demo_payload_rename(&output);
    if (output.name.len != 7 || memcmp(output.name.ptr, "updated", 7) || output.bytes.len != 0 || output.values.len != 0) return 3;
    demo_payload_free(&output); demo_payload_free(&output);
    DemoString text=demo_echo_string(demo_string_view("wire",4));
    if (text.len != 4 || memcmp(text.ptr,"wire",4)) return 2;
    demo_replace_text(&text);
    if (text.len != 7 || memcmp(text.ptr, "updated", 7)) return 4;
    demo_string_free(&text); demo_string_free(&text);
    demo_fill_bytes((DemoU8MutSlice){bytes, 3});
    if (bytes[0] != 7 || bytes[2] != 7) return 5;
    return 0;
}
"#).expect("source");
        let compile = Command::new("cc")
            .current_dir(&dir)
            .args([
                "-std=c11", "-Wall", "-Wextra", "-Werror", "test.c", "-lm", "-o", "test",
            ])
            .output()
            .expect("cc");
        assert!(
            compile.status.success(),
            "cc failed:\n{}\n{}",
            String::from_utf8_lossy(&compile.stdout),
            String::from_utf8_lossy(&compile.stderr)
        );
        assert!(
            Command::new(dir.join("test"))
                .status()
                .expect("run")
                .success()
        );
    }

    #[test]
    fn c_target_snapshot_primitive_function() {
        let bindings = bindings(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 { left + right }
            "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let output = target.render(&bindings).expect("render");
        insta::assert_snapshot!("c_primitive_function", render_header(&output));
    }

    #[test]
    fn c_target_snapshot_direct_record_and_enum() {
        let bindings = bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: f64,
                pub y: f64,
            }

            #[repr(u8)]
            #[data]
            pub enum Mode {
                Fast = 1,
                Slow = 2,
            }
            "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let output = target.render(&bindings).expect("render");
        insta::assert_snapshot!("c_direct_record_and_enum", render_header(&output));
    }

    #[test]
    fn c_target_snapshot_class() {
        let bindings = bindings(
            r#"
            pub struct Engine;

            #[export(single_threaded)]
            impl Engine {
                pub fn new(seed: u64) -> Self { todo!() }
                pub fn score(&self, point: crate::Point) -> u32 { 0 }
            }

            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: f64,
                pub y: f64,
            }
            "#,
        );
        let target = CHost::new().into_target(&bindings).expect("target");
        let output = target.render(&bindings).expect("render");
        insta::assert_snapshot!("c_class", render_header(&output));
    }
}
