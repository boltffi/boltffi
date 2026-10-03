use std::{fs, path::Path, process::Command, sync::Mutex, time::UNIX_EPOCH};

use boltffi_ast::PackageInfo;
use boltffi_backend::{
    FilePath, GeneratedFile, GeneratedOutput, Target, bridge::c::CBridge,
    target::csharp::CSharpHost,
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

fn compile_csharp_with_dotnet_when_available(output: &GeneratedOutput, prefix: &str) {
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
    fs::write(
        directory.join("Smoke.csproj"),
        r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net10.0</TargetFramework>
    <Nullable>enable</Nullable>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
  </PropertyGroup>
</Project>
"#,
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
