use std::{fs, path::Path, process::Command, sync::Mutex, time::UNIX_EPOCH};

use boltffi_ast::PackageInfo;
use boltffi_backend::{
    FilePath, GeneratedFile, GeneratedOutput, Target,
    bridge::c::CBridge,
    target::csharp::{CSharpHost, CSharpProfile},
};
use boltffi_binding::{Bindings, Native, lower};

// NuGet first-run setup uses a process-global migration mutex.
static DOTNET_BUILD_LOCK: Mutex<()> = Mutex::new(());

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

const LEGACY_RUNTIME_VALUES: &str =
    include_str!("fixtures/source/exports/legacy_runtime_values.rs");
const CUSTOM_TYPE_DEFAULT: &str = include_str!("fixtures/source/records/custom_type_default.rs");
const PARAMETER_DEFAULTS: &str = include_str!("fixtures/source/exports/parameter_defaults.rs");

#[test]
fn csharp_parameter_defaults_use_optional_arguments_and_runtime_overloads() {
    let output = target(CSharpHost::new().native_library("demo_native"))
        .render(&bindings(PARAMETER_DEFAULTS))
        .expect("parameter defaults render");
    let sources = output
        .files()
        .iter()
        .map(|file| file.contents())
        .collect::<String>();
    [
        "string greeting = \"world\"",
        "uint times = 3U",
        "long offset = -1L",
        "bool shout = true",
        "float ratio = 0.5F",
        "ushort? limit = (ushort)7",
        "public Server(ushort port = (ushort)8080)",
        "public DefaultCounter(int start = 10)",
        "Offset(int step = 1)",
        "DefaultParameterValue(20)] int start, int offset",
        "AsyncDefault(uint value = 9U, global::System.Threading.CancellationToken",
        "Span(long start = -9223372036854775808L, ulong end = 18446744073709551615UL)",
        "DefaultAmount()\n            => DefaultAmount(new global::Demo.DefaultAmount(5));",
    ]
    .into_iter()
    .for_each(|signature| assert!(sources.contains(signature), "missing {signature}"));
    compile_csharp_with_dotnet_when_available(&output, "csharp-parameter-defaults");
}

#[test]
fn csharp_rejects_runtime_defaults_with_identical_overload_signatures() {
    [
        r#"
        #[export]
        pub fn ambiguous(
            #[boltffi::default(3)] first: AmountRust,
            #[boltffi::default(4)] second: AmountRust,
        ) {}
        "#,
        r#"
        #[data]
        pub enum Choice { Empty, Value(i32) }

        #[export]
        pub fn ambiguous(
            #[boltffi::default(Choice::Empty)] first: Choice,
            middle: Option<Choice>,
            #[boltffi::default(Choice::Empty)] last: Choice,
        ) {}
        "#,
    ]
    .into_iter()
    .for_each(|declaration| {
        let source = format!("{PARAMETER_DEFAULTS}{declaration}");
        let error = target(CSharpHost::new().native_library("demo_native"))
            .render(&bindings(&source))
            .expect_err("identical C# overload signatures must be rejected");
        assert!(
            error.to_string().contains("indistinguishable C# overloads"),
            "{error}"
        );
    });
}

#[test]
fn csharp_bounds_runtime_default_overloads_without_limiting_constant_defaults() {
    [8, 9, 20].into_iter().for_each(|runtime_default_count| {
        let records = (0..runtime_default_count)
            .map(|index| {
                format!(
                    r#"
                    #[data]
                    pub struct Amount{index} {{ pub value: i32 }}

                    custom_type!(
                        pub CustomAmount{index},
                        remote = AmountRust{index},
                        repr = Amount{index},
                        into_ffi = amount_into_ffi{index},
                        try_from_ffi = amount_from_ffi{index}
                    );
                    "#
                )
            })
            .collect::<String>();
        let parameters = (0..runtime_default_count)
            .map(|index| format!("#[boltffi::default(3)] amount{index}: AmountRust{index}"))
            .chain((0..16).map(|index| format!("#[boltffi::default(7)] constant{index}: i32")))
            .chain(["required: i32".to_owned()])
            .collect::<Vec<_>>()
            .join(",");
        let source = format!("{records} #[export] pub fn many_defaults({parameters}) {{}}");
        let result =
            target(CSharpHost::new().native_library("demo_native")).render(&bindings(&source));
        if runtime_default_count > 8 {
            let error = result.expect_err("excessive runtime defaults must be rejected");
            assert!(
                error.to_string().contains("more than 8 runtime defaults"),
                "{error}"
            );
            return;
        }

        let mut output = result.expect("eight runtime defaults should render");
        let module = output
            .files()
            .iter()
            .find(|file| file.path().as_path() == Path::new("Demo.cs"))
            .map(|file| file.contents())
            .expect("generated Demo.cs");
        assert_eq!(
            module.matches("public static void ManyDefaults(").count(),
            256
        );
        output.append(GeneratedOutput::new(
            vec![GeneratedFile::new(
                FilePath::new("RuntimeDefaultCalls.cs").expect("call-site path"),
                r#"
                using Demo;
                using static Demo.Demo;

                public static class RuntimeDefaultCalls
                {
                    public static void Compile()
                    {
                        ManyDefaults(required: 1);
                        ManyDefaults(required: 1, amount7: new Amount7(7));
                        ManyDefaults(required: 1, amount0: new Amount0(5), constant15: 2);
                    }
                }
                "#,
            )],
            Vec::new(),
        ));
        compile_csharp_with_dotnet_when_available(&output, "csharp-runtime-default-limit");
    });
}

#[test]
fn csharp_non_trailing_and_constructed_defaults_compile_at_call_sites() {
    let source = format!(
        "{PARAMETER_DEFAULTS}{}",
        r#"
        #[export]
        pub fn non_trailing_defaults(
            #[boltffi::default(-1)] signed_byte: i8,
            #[boltffi::default(2)] byte: u8,
            #[boltffi::default(-3)] short: i16,
            #[boltffi::default(7)] limit: Option<u16>,
            #[boltffi::default(0.5)] ratio: Option<f32>,
            #[boltffi::default(DefaultMode::Quiet)] mode: DefaultMode,
            #[boltffi::default(3)] native_signed: isize,
            #[boltffi::default(4)] native_unsigned: usize,
            required: i32,
        ) {}

        #[export]
        pub fn native_boundaries(
            #[boltffi::default(-9223372036854775808)] lower: isize,
            #[boltffi::default(18446744073709551615)] upper: usize,
            required: i32,
        ) {}

        #[export]
        pub fn optional_closure(
            #[boltffi::default(None)] callback: Option<Box<dyn Fn(i32) -> i32>>,
            value: i32,
        ) -> i32 { value }

        pub struct RuntimeDefaults;

        #[export]
        impl RuntimeDefaults {
            pub fn new(#[boltffi::default(5)] amount: AmountRust) -> Self { Self }

            pub fn amount(&self, #[boltffi::default(5)] amount: AmountRust) -> i32 { 0 }

            pub async fn start(#[boltffi::default(5)] amount: AmountRust) -> Self { Self }

            pub async fn async_amount(&self, #[boltffi::default(5)] amount: AmountRust) -> i32 { 0 }
        }

        #[repr(u8)]
        #[data]
        pub enum RuntimeMode { Low, High }

        #[data(impl)]
        impl RuntimeMode {
            pub fn amount(&self, #[boltffi::default(5)] amount: AmountRust) -> i32 { 0 }

            pub async fn async_amount(&self, #[boltffi::default(5)] amount: AmountRust) -> i32 { 0 }
        }

        #[data]
        pub enum Choice { Empty, Value(i32) }

        #[export]
        pub fn mixed_defaults(
            #[boltffi::default(5)] amount: AmountRust,
            #[boltffi::default(Choice::Empty)] choice: Choice,
            #[boltffi::default(7)] limit: Option<u16>,
            required: i32,
        ) {}

        #[export]
        pub fn optional_record_defaults(
            #[boltffi::default(5)] amount: AmountRust,
            #[boltffi::default(6)] optional_amount: Option<AmountRust>,
        ) {}
        "#
    );
    let mut output = target(CSharpHost::new().native_library("demo_native"))
        .render(&bindings(&source))
        .expect("non-trailing and constructed defaults render");
    output.append(GeneratedOutput::new(
        vec![GeneratedFile::new(
            FilePath::new("DefaultArguments.cs").expect("call-site path"),
            include_str!("fixtures/csharp/default_arguments.cs"),
        )],
        Vec::new(),
    ));
    compile_csharp_with_dotnet_when_available(&output, "csharp-default-call-sites");
}

#[test]
fn csharp_module_class_collides_with_exported_record() {
    let source = r#"
        #[data]
        pub struct Demo {
            pub value: i32,
        }

        #[export]
        pub fn make_demo() -> Demo {
            Demo { value: 1 }
        }
        "#;
    let control = target(CSharpHost::new().native_library("demo_native"))
        .render(&bindings(&source.replace("Demo", "Widget")))
        .expect("noncolliding C# output should render");
    compile_csharp_with_dotnet_when_available(&control, "csharp-module-record-control");

    let bindings = bindings(source);
    let error = target(CSharpHost::new()).render(&bindings).unwrap_err();
    assert!(
        matches!(error, boltffi_backend::BackendError::CSharpModuleClassCollision { ref name, .. } if name == "Demo")
    );
    assert!(error.to_string().contains("targets.csharp.module_class"));

    let output = target(CSharpHost::new().module_class("DemoApi").unwrap())
        .render(&bindings)
        .expect("distinct module class should render");
    let module = output
        .files()
        .iter()
        .find(|file| file.path().as_path() == Path::new("DemoApi.cs"))
        .expect("generated DemoApi.cs");
    assert!(module.contents().contains("public static class DemoApi"));
    assert!(
        output
            .files()
            .iter()
            .any(|file| file.path().as_path() == Path::new("Demo.cs"))
    );
    compile_csharp_with_dotnet_when_available(&output, "csharp-module-record-collision");
}

#[test]
fn csharp_module_class_cannot_shadow_callback_companions() {
    let bindings = bindings(
        r#"
        #[export]
        pub trait Handler {
            fn invoke(&self, value: String) -> String;
        }
        "#,
    );
    for name in ["HandlerProxy", "HandlerBridge", "BoltFFICallbackHandle"] {
        let output = target(CSharpHost::new().module_class(name).unwrap()).render(&bindings);
        assert!(
            matches!(output, Err(boltffi_backend::BackendError::CSharpModuleClassCollision { name: ref collision, .. }) if collision == name),
            "module class {name} must not shadow a callback companion"
        );
    }
    let output = target(CSharpHost::new().module_class("handlerProxy").unwrap())
        .render(&bindings)
        .expect("case-sensitive C# type names with distinct filenames should render");
    compile_csharp_with_dotnet_when_available(&output, "csharp-module-callback-companions");
}

#[test]
fn csharp_module_class_cannot_shadow_stream_companions() {
    let bindings = bindings(include_str!("fixtures/source/stream/protocol_functions.rs"));
    for name in [
        "EnginePointsStreamRuntime",
        "EnginePointsSubscription",
        "EngineNamesStreamRuntime",
        "EngineTicksStreamRuntime",
        "EngineTicksCancellable",
    ] {
        let output = target(CSharpHost::new().module_class(name).unwrap()).render(&bindings);
        assert!(
            matches!(
                output,
                Err(boltffi_backend::BackendError::CSharpModuleClassCollision { .. })
            ),
            "module class {name} must not shadow a stream companion"
        );
    }
}

#[test]
fn csharp_module_class_cannot_shadow_enum_companions() {
    let bindings = bindings(include_str!("fixtures/source/constant/associated.rs"));
    let methods = self::bindings(include_str!("fixtures/source/associated/enum_callables.rs"));
    for (name, bindings) in [("ModeConstants", &bindings), ("ModeMethods", &methods)] {
        let output = target(CSharpHost::new().module_class(name).unwrap()).render(bindings);
        assert!(
            matches!(
                output,
                Err(boltffi_backend::BackendError::CSharpModuleClassCollision { .. })
            ),
            "module class {name} must not shadow an enum companion"
        );
    }
}

#[test]
fn csharp_module_class_cannot_shadow_exception_types() {
    let bindings = bindings(include_str!("fixtures/source/enums/error_message.rs"));
    for name in ["ServiceErrorException", "RecordErrorException"] {
        let output = target(CSharpHost::new().module_class(name).unwrap()).render(&bindings);
        assert!(
            matches!(
                output,
                Err(boltffi_backend::BackendError::CSharpModuleClassCollision { .. })
            ),
            "module class {name} must not shadow an exception type"
        );
    }
}

#[test]
fn csharp_module_class_cannot_shadow_emitted_runtime_types() {
    let bindings = bindings("#[export] pub async fn text(value: String) -> String { value }");
    for name in [
        "FfiStatus",
        "FfiBuf",
        "WireReader",
        "WireWriter",
        "BoltException",
        "BoltFFIAsync",
    ] {
        let output = target(CSharpHost::new().module_class(name).unwrap()).render(&bindings);
        assert!(
            matches!(
                output,
                Err(boltffi_backend::BackendError::CSharpModuleClassCollision { .. })
            ),
            "module class {name} must not shadow an emitted runtime type"
        );
    }
    let closure = self::bindings(include_str!("fixtures/source/exports/closure_parameter.rs"));
    let output = target(
        CSharpHost::new()
            .module_class("BoltFFIOwnedClosure")
            .unwrap(),
    )
    .render(&closure);
    assert!(matches!(
        output,
        Err(boltffi_backend::BackendError::CSharpModuleClassCollision { .. })
    ));
    let output = target(CSharpHost::new().module_class("WireReader").unwrap())
        .render(&self::bindings("#[export] pub fn ping() {}"))
        .expect("a runtime name is available when that runtime is not emitted");
    compile_csharp_with_dotnet_when_available(&output, "csharp-module-unused-runtime");
}

#[test]
fn csharp_module_class_rejects_case_insensitive_output_collision() {
    let output = target(CSharpHost::new().module_class("demo").unwrap())
        .render(&bindings("#[data] pub struct Demo { pub value: i32 }"));
    assert!(matches!(
        output,
        Err(boltffi_backend::BackendError::CSharpModuleClassCollision { .. })
    ));
}

#[test]
fn csharp_module_class_cannot_shadow_native_methods() {
    let output = target(CSharpHost::new().module_class("NativeMethods").unwrap())
        .render(&bindings("#[export] pub fn ping() {}"));
    assert!(
        matches!(output, Err(boltffi_backend::BackendError::CSharpModuleClassCollision { name, .. }) if name == "NativeMethods")
    );
}

#[test]
fn csharp_generated_helpers_do_not_shadow_exported_parameters() {
    let bindings = bindings(
        r#"
        #[export]
        pub fn notify(status: i32, boltffi_status: i32) {
            let _ = (status, boltffi_status);
        }

        #[export]
        pub fn notify_single(status: i32) {
            let _ = status;
        }

        #[export]
        pub fn notify_text(status: i32, boltffi_status: i32, value: String) {
            let _ = (status, boltffi_status, value);
        }

        #[export]
        pub async fn fetch(
            cancellation_token: i32,
            boltffi_cancellation_token: i32,
            boltffi_status: i32,
            boltffi_future: i32,
            boltffi_result: i32,
        ) -> i32 {
            cancellation_token + boltffi_cancellation_token + boltffi_status
                + boltffi_future + boltffi_result
        }


        #[export]
        pub async fn fetch_single(cancellation_token: i32) -> i32 {
            cancellation_token
        }
        "#,
    );
    let output = target(CSharpHost::new().native_library("demo_native"))
        .render(&bindings)
        .expect("colliding parameter names should render");
    let module = output
        .files()
        .iter()
        .find(|file| file.path().as_path() == Path::new("Demo.cs"))
        .map(|file| file.contents())
        .expect("generated Demo.cs");

    assert!(
        module.contains(
            "FfiStatus boltffiStatus2 = NativeMethods.NativeNotify(status, boltffiStatus)"
        ),
        "{module}"
    );
    assert!(
        module.contains("FfiStatus boltffiStatus = NativeMethods.NativeNotifySingle(status)"),
        "{module}"
    );
    assert!(
        module.contains(
            "FfiStatus boltffiStatus2 = NativeMethods.NativeNotifyText(status, boltffiStatus"
        ),
        "{module}"
    );
    assert!(
        module.contains("CancellationToken boltffiCancellationToken2 = default"),
        "{module}"
    );
    assert!(
        module.contains("CancellationToken boltffiCancellationToken = default"),
        "{module}"
    );
    assert!(module.contains("boltffiFuture2 =>"), "{module}");
    assert!(module.contains("out FfiStatus boltffiStatus2"), "{module}");
    assert!(module.contains("return boltffiResult2;"), "{module}");
    compile_csharp_with_dotnet_when_available(&output, "csharp-helper-name-collisions");
}

#[test]
fn csharp_single_element_tuples_compile_and_use_item_one() {
    let bindings = bindings(
        r#"
        #[export]
        pub const ONLY: (u32,) = (1,);
        #[data]
        pub struct TupleRecord {
            pub number: (u32,),
            pub text: (String,),
        }
        #[export]
        pub fn echo_only(value: (u32,)) -> (u32,) { value }
        #[export]
        pub fn echo_nested(value: ((u32,), String)) -> ((u32,), String) { value }
    "#,
    );
    let output = target(CSharpHost::new().native_library("demo_native"))
        .render(&bindings)
        .expect("single-element tuples should render");
    compile_csharp_with_dotnet_when_available(&output, "csharp-single-element-tuples");
    let module = output
        .files()
        .iter()
        .find(|file| file.path().as_path() == Path::new("Demo.cs"))
        .map(|file| file.contents())
        .expect("generated Demo.cs");
    assert!(
        module.contains("global::System.ValueTuple<uint>"),
        "{module}"
    );
    assert!(module.contains("value.Item1.Item1"), "{module}");
}

#[test]
fn csharp_target_compiles_a_regular_vec_u8_wire_api() {
    let bindings = bindings(
        r#"
        #[export]
        pub fn echo_bytes(value: Vec<u8>) -> Vec<u8> { value }
        "#,
    );
    let output = target(
        CSharpHost::new()
            .namespace("Company.Bindings")
            .expect("valid namespace")
            .native_library("demo_native"),
    )
    .render(&bindings)
    .expect("a regular Vec<u8> API should render");

    let module = output
        .files()
        .iter()
        .find(|file| file.path().as_path() == Path::new("Demo.cs"))
        .map(|file| file.contents())
        .expect("generated Demo.cs");
    assert!(module.contains("internal static extern FfiBuf BufFromBytes"));

    compile_csharp_with_dotnet_when_available(&output, "csharp-vec-u8-wire-runtime");
}

#[test]
fn csharp_target_compiles_standalone_wire_types() {
    let bindings = bindings(
        r#"
        #[data]
        pub struct Profile {
            pub name: String,
        }

        #[data]
        pub enum Shape {
            Empty,
            Label(String),
        }
        "#,
    );
    let output = target(
        CSharpHost::new()
            .namespace("Company.Bindings")
            .expect("valid namespace")
            .native_library("demo_native"),
    )
    .render(&bindings)
    .expect("standalone wire types should render");

    let module = output
        .files()
        .iter()
        .find(|file| file.path().as_path() == Path::new("Demo.cs"))
        .map(|file| file.contents())
        .expect("generated Demo.cs");
    assert!(module.contains("internal static extern FfiBuf BufFromBytes"));

    compile_csharp_with_dotnet_when_available(&output, "csharp-standalone-wire-types");
}

#[test]
fn csharp_target_qualifies_a_class_method_named_after_its_return_record() {
    let bindings = bindings(
        r#"
        #[data]
        pub struct ServerInfo {
            pub version: String,
        }

        pub struct ParseClient {
            id: i32,
        }

        #[export]
        impl ParseClient {
            pub fn new(id: i32) -> Self { Self { id } }

            pub fn server_info(&self) -> Result<ServerInfo, String> {
                Ok(ServerInfo { version: "1".to_string() })
            }
        }

        #[export]
        pub fn apply_server_info(
            f: impl Fn(ServerInfo) -> ServerInfo,
            value: ServerInfo,
        ) -> ServerInfo {
            f(value)
        }
        "#,
    );
    let output = target(
        CSharpHost::new()
            .namespace("Company.Bindings")
            .expect("valid namespace")
            .native_library("demo_native"),
    )
    .render(&bindings)
    .expect("a class method named after its return record should still render");

    assert!(
        output.diagnostics().is_empty(),
        "unexpected diagnostics: {:?}",
        output.diagnostics()
    );

    let class = output
        .files()
        .iter()
        .find(|file| file.path().as_path() == Path::new("ParseClient.cs"))
        .map(|file| file.contents())
        .expect("generated ParseClient.cs");

    assert!(
        class.contains("return global::Company.Bindings.ServerInfo.Decode(resultReader);"),
        "expected the self-named ServerInfo() method to qualify its own Decode call:\n{class}"
    );
    assert!(
        !class.contains("return ServerInfo.Decode(resultReader);"),
        "an unqualified Decode call inside ServerInfo() would resolve to the method group, not \
         the type (CS0119):\n{class}"
    );

    compile_csharp_with_dotnet_when_available(&output, "csharp-sibling-shadow-smoke");
}

#[test]
fn csharp_target_qualifies_a_free_function_named_after_its_return_record() {
    let bindings = bindings(
        r#"
        #[data]
        pub struct ServerInfo {
            pub version: String,
        }

        #[export]
        pub fn server_info() -> Result<ServerInfo, String> {
            Ok(ServerInfo { version: "1".to_string() })
        }

        #[export]
        pub fn apply_server_info(
            f: impl Fn(ServerInfo) -> ServerInfo,
            value: ServerInfo,
        ) -> ServerInfo {
            f(value)
        }
        "#,
    );
    let output = target(
        CSharpHost::new()
            .namespace("Company.Bindings")
            .expect("valid namespace")
            .native_library("demo_native"),
    )
    .render(&bindings)
    .expect("a free function named after its return record should still render");

    assert!(
        output.diagnostics().is_empty(),
        "unexpected diagnostics: {:?}",
        output.diagnostics()
    );

    let module = output
        .files()
        .iter()
        .find(|file| file.path().as_path() == Path::new("Demo.cs"))
        .map(|file| file.contents())
        .expect("generated Demo.cs");

    assert!(
        module.contains("return global::Company.Bindings.ServerInfo.Decode(resultReader);"),
        "expected the self-named ServerInfo() free function to qualify its own Decode call:\n{module}"
    );

    compile_csharp_with_dotnet_when_available(&output, "csharp-free-function-shadow-smoke");
}

#[test]
fn csharp_target_renders_custom_type_defaults_through_representations() {
    let bindings = bindings(CUSTOM_TYPE_DEFAULT);
    let output = target(
        CSharpHost::new()
            .namespace("Company.Bindings")
            .expect("valid namespace")
            .native_library("demo_native"),
    )
    .render(&bindings)
    .expect("custom type defaults should render");
    let config = output
        .files()
        .iter()
        .find(|file| file.path().as_path() == Path::new("DeviationConfig.cs"))
        .map(|file| file.contents())
        .expect("generated DeviationConfig.cs");

    assert!(
        config.contains("public DeviationConfig()\n            : this(new LengthFfi(1500.0))"),
        "{config}"
    );
    compile_csharp_with_dotnet_when_available(&output, "csharp-custom-type-default");
}

const NET_FRAMEWORK_MISSING_APIS: &[&str] = &[
    "PtrToStringUTF8",
    "DateTime.UnixEpoch",
    "bigEndian",
    "SingleToInt32Bits",
    "Int32BitsToSingle",
    "stackalloc",
    "[..",
    "..]",
    "[^",
];

fn net_framework_host(framework: &str) -> CSharpHost {
    CSharpHost::new()
        .native_library("demo_native")
        .target_framework(framework)
        .expect("supported target framework")
}

fn all_sources(output: &GeneratedOutput) -> String {
    output
        .files()
        .iter()
        .map(|file| file.contents())
        .collect::<String>()
}

fn file<'output>(output: &'output GeneratedOutput, name: &str) -> Option<&'output str> {
    output
        .files()
        .iter()
        .find(|file| file.path().as_path() == Path::new(name))
        .map(|file| file.contents())
}

#[test]
fn csharp_net_framework_runtime_avoids_apis_missing_from_net_framework() {
    let output = target(net_framework_host("net48"))
        .render(&bindings(LEGACY_RUNTIME_VALUES))
        .expect("legacy runtime values render");
    let sources = all_sources(&output);
    for api in NET_FRAMEWORK_MISSING_APIS {
        assert!(!sources.contains(api), "net48 output uses {api}");
    }
    insta::assert_snapshot!(
        "csharp_net_framework_runtime",
        file(&output, "Demo.cs").expect("module file")
    );
    compile_csharp_for_net_framework_when_enabled(&output, "csharp-net48-runtime", "net48");
    compile_csharp_for_net_framework_when_enabled(&output, "csharp-net461-runtime", "net461");
}

#[test]
fn csharp_net_framework_compiles_generated_bindings() {
    let fixtures = [
        (
            "async",
            include_str!("fixtures/source/exports/async_handles_and_callbacks.rs"),
        ),
        (
            "async-class",
            include_str!("fixtures/source/exports/async_class_methods.rs"),
        ),
        (
            "tuple",
            include_str!("fixtures/source/exports/tuple_functions.rs"),
        ),
        (
            "map",
            include_str!("fixtures/source/exports/map_functions.rs"),
        ),
        (
            "result",
            include_str!("fixtures/source/exports/result_values.rs"),
        ),
        (
            "closure",
            include_str!("fixtures/source/exports/closure_parameter.rs"),
        ),
        ("defaults", PARAMETER_DEFAULTS),
        (
            "direct-record",
            include_str!("fixtures/source/exports/direct_records_and_c_style_enums.rs"),
        ),
        (
            "callback-vector",
            include_str!("fixtures/source/callback/callback_direct_vector_parameter.rs"),
        ),
        (
            "callback-record",
            include_str!("fixtures/source/callback/callback_record_return.rs"),
        ),
        (
            "stream",
            include_str!("fixtures/source/stream/protocol_functions.rs"),
        ),
    ];
    for (name, source) in fixtures {
        let output = target(net_framework_host("net461"))
            .render(&bindings(source))
            .unwrap_or_else(|error| panic!("{name} renders: {error}"));
        let sources = all_sources(&output);
        for api in NET_FRAMEWORK_MISSING_APIS {
            assert!(!sources.contains(api), "{name} output uses {api}");
        }
        compile_csharp_for_net_framework_when_enabled(
            &output,
            &format!("csharp-net461-{name}"),
            "net461",
        );
    }
}

#[test]
fn csharp_native_entry_points_use_the_cdecl_calling_convention() {
    let fixtures = [
        include_str!("fixtures/source/exports/async_handles_and_callbacks.rs"),
        include_str!("fixtures/source/exports/async_class_methods.rs"),
        include_str!("fixtures/source/exports/class_handles_and_methods.rs"),
        include_str!("fixtures/source/exports/closure_parameter.rs"),
        include_str!("fixtures/source/exports/result_values.rs"),
        include_str!("fixtures/source/callback/async_callback_return_shapes.rs"),
        include_str!("fixtures/source/callback/callback_direct_vector_parameter.rs"),
        include_str!("fixtures/source/callback/callback_record_return.rs"),
        include_str!("fixtures/source/stream/protocol_functions.rs"),
        LEGACY_RUNTIME_VALUES,
    ];
    let mut imports = 0;
    let mut delegates = 0;
    for host in [CSharpHost::new(), net_framework_host("net48")] {
        for source in fixtures {
            let output = target(host.clone().native_library("demo_native"))
                .render(&bindings(source))
                .expect("fixture renders");
            for file in output.files() {
                let lines = file.contents().lines().collect::<Vec<_>>();
                for (index, line) in lines.iter().enumerate() {
                    if line.contains("DllImport(") {
                        imports += 1;
                        assert!(
                            line.contains("CallingConvention = global::System.Runtime.InteropServices.CallingConvention.Cdecl"),
                            "{line}"
                        );
                    }
                    if line.trim_start().starts_with("internal delegate ") {
                        delegates += 1;
                        let attribute = lines[..index]
                            .iter()
                            .rev()
                            .find(|line| !line.trim_start().starts_with("[return:"))
                            .expect("delegate attribute");
                        assert!(
                            attribute.contains("UnmanagedFunctionPointer(")
                                && attribute.contains("CallingConvention.Cdecl"),
                            "{line}"
                        );
                    }
                }
            }
        }
    }
    assert!(imports > 0 && delegates > 0);
}

#[test]
fn csharp_direct_records_have_one_layout_on_every_runtime_identifier() {
    let bindings = bindings(
        r#"
        #[data]
        pub struct Narrow {
            pub tag: u16,
            pub value: i32,
            pub flag: u8,
        }

        #[data]
        pub struct Wide {
            pub first: u64,
            pub second: f64,
        }

        #[data]
        pub struct Misaligned {
            pub tag: u8,
            pub wide: u64,
        }

        #[data]
        pub struct PointerSized {
            pub count: usize,
        }

        #[export]
        pub fn echo_narrow(value: Narrow) -> Narrow { value }

        #[export]
        pub fn echo_wide(value: Wide) -> Wide { value }

        #[export]
        pub fn echo_misaligned(value: Misaligned) -> Misaligned { value }

        #[export]
        pub fn echo_pointer_sized(value: PointerSized) -> PointerSized { value }
        "#,
    );
    let output = target(CSharpHost::new().native_library("demo_native"))
        .render(&bindings)
        .expect("records render");
    let direct = |name: &str| {
        file(&output, name)
            .expect("record file")
            .contains("[StructLayout(LayoutKind.Sequential)]")
    };
    assert!(direct("Narrow.cs"));
    assert!(direct("Wide.cs"));
    assert!(!direct("Misaligned.cs"));
    assert!(!direct("PointerSized.cs"));
}

#[test]
fn csharp_modern_profile_keeps_modern_runtime_apis() {
    let modern = target(CSharpHost::new().native_library("demo_native"))
        .render(&bindings(LEGACY_RUNTIME_VALUES))
        .expect("modern runtime values render");
    let explicit = target(
        CSharpHost::new()
            .native_library("demo_native")
            .profile(CSharpProfile::Modern),
    )
    .render(&bindings(LEGACY_RUNTIME_VALUES))
    .expect("explicit modern profile renders");
    let sources = all_sources(&modern);
    assert_eq!(sources, all_sources(&explicit));
    for api in [
        "Marshal.PtrToStringUTF8",
        "BitConverter.Int32BitsToSingle",
        "BitConverter.SingleToInt32Bits",
        "DateTime.UnixEpoch",
        "TryWriteBytes(bytes, bigEndian: true",
    ] {
        assert!(sources.contains(api), "modern output lost {api}");
    }
    assert!(file(&modern, "Polyfills.cs").is_none());
    compile_csharp_with_dotnet_when_available(&modern, "csharp-modern-runtime");
}

#[test]
fn csharp_batch_stream_handles_follow_the_profile() {
    let bindings = bindings(include_str!("fixtures/source/stream/protocol_functions.rs"));
    let modern = all_sources(
        &target(CSharpHost::new().native_library("demo_native"))
            .render(&bindings)
            .expect("modern streams render"),
    );
    assert!(modern.contains("private ulong handle;"));
    assert!(modern.contains("Interlocked.Exchange(ref handle, 0);"));
    let legacy = all_sources(
        &target(net_framework_host("net48"))
            .render(&bindings)
            .expect("legacy streams render"),
    );
    assert!(legacy.contains("private long handle;"));
    assert!(legacy.contains(
        "unchecked((ulong)global::System.Threading.Interlocked.Exchange(ref handle, 0))"
    ));
}

#[test]
fn csharp_polyfills_are_emitted_only_for_legacy_profiles() {
    let bindings = bindings(LEGACY_RUNTIME_VALUES);
    for framework in ["net5.0", "net10.0", "netcoreapp3.1", "netstandard2.1"] {
        let output = target(net_framework_host(framework))
            .render(&bindings)
            .expect("modern profile renders");
        assert!(file(&output, "Polyfills.cs").is_none(), "{framework}");
    }
    for framework in ["net461", "net48", "netstandard2.0"] {
        let output = target(net_framework_host(framework))
            .render(&bindings)
            .expect("legacy profile renders");
        let polyfills = file(&output, "Polyfills.cs").expect("legacy polyfills");
        assert!(polyfills.contains("namespace System.Runtime.CompilerServices"));
        assert!(polyfills.contains("internal static class IsExternalInit { }"));
    }
}

#[test]
fn csharp_rejects_unsupported_target_frameworks() {
    for framework in ["net40", "net46", "netcoreapp2.1", "monoandroid13.0"] {
        let error = CSharpHost::new().target_framework(framework).unwrap_err();
        assert!(
            matches!(error, boltffi_backend::BackendError::CSharpTargetFramework { framework: ref name, .. } if name == framework)
        );
        assert!(
            error
                .to_string()
                .contains("targets.csharp.target_framework")
        );
    }
}

fn compile_csharp_with_dotnet_when_available(output: &GeneratedOutput, prefix: &str) {
    compile_csharp_with_dotnet(output, prefix, "net10.0", &[]);
}

// Restoring the .NET Framework reference assemblies and compatibility packages needs NuGet access.
fn compile_csharp_for_net_framework_when_enabled(
    output: &GeneratedOutput,
    prefix: &str,
    framework: &str,
) {
    if std::env::var_os("BOLTFFI_TEST_NET_FRAMEWORK").is_none() {
        return;
    }
    compile_csharp_with_dotnet(
        output,
        prefix,
        framework,
        &[
            ("Microsoft.Bcl.AsyncInterfaces", "6.0.0"),
            ("System.Memory", "4.5.5"),
            ("System.Threading.Tasks.Extensions", "4.5.4"),
            ("System.ValueTuple", "4.5.0"),
        ],
    );
}

fn compile_csharp_with_dotnet(
    output: &GeneratedOutput,
    prefix: &str,
    framework: &str,
    packages: &[(&str, &str)],
) {
    let _dotnet_build_guard = DOTNET_BUILD_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if Command::new("dotnet").arg("--version").output().is_err() {
        return;
    }

    let directory = std::env::temp_dir().join(format!(
        "{prefix}-{}",
        UNIX_EPOCH.elapsed().expect("system clock").as_nanos()
    ));
    let src = directory.join("src");
    fs::create_dir_all(&src).expect("create dotnet smoke src directory");
    for file in output.files().iter().filter(|file| {
        file.path()
            .as_path()
            .extension()
            .is_some_and(|ext| ext == "cs")
    }) {
        let path = src.join(file.path().as_path());
        fs::create_dir_all(path.parent().expect("generated C# parent"))
            .expect("create generated C# parent directory");
        fs::write(&path, file.contents()).expect("write generated C# source");
    }
    let references = packages
        .iter()
        .map(|(name, version)| {
            format!("    <PackageReference Include=\"{name}\" Version=\"{version}\" />\n")
        })
        .collect::<String>();
    fs::write(
        directory.join("Smoke.csproj"),
        format!(
            r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>{framework}</TargetFramework>
    <Nullable>enable</Nullable>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
    <LangVersion>latest</LangVersion>
  </PropertyGroup>
  <ItemGroup>
{references}  </ItemGroup>
</Project>
"#
        ),
    )
    .expect("write smoke csproj");

    let build = Command::new("dotnet")
        .arg("build")
        .arg(directory.join("Smoke.csproj"))
        .arg("--nologo")
        .env("APPDATA", directory.join("appdata"))
        .output()
        .expect("dotnet build should execute");
    assert!(
        build.status.success(),
        "generated C# failed to build:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    fs::remove_dir_all(&directory).expect("remove dotnet smoke directory");
}

const PADDED_DIRECT_RECORD: &str = include_str!("fixtures/source/records/padded_direct_record.rs");

#[test]
fn csharp_wire_codecs_step_over_direct_record_padding() {
    let output = target(CSharpHost::new().native_library("demo_native"))
        .render(&bindings(PADDED_DIRECT_RECORD))
        .expect("padded direct record renders");
    let source = output
        .files()
        .iter()
        .map(|file| file.contents())
        .collect::<String>();
    for expected in [
        "reader.ReadU16(),",
        "reader.Skip(2).ReadI32(),",
        "reader.ReadU8()",
        "reader.Skip(3);",
        "writer.Pad(2);",
        "writer.Pad(3);",
        "internal WireReader Skip(int count)",
        "internal void Pad(int count)",
    ] {
        assert!(
            source.contains(expected),
            "missing `{expected}` in:\n{source}"
        );
    }
}
