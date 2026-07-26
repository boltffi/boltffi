use std::{fs, path::Path, process::Command, time::UNIX_EPOCH};

use boltffi_ast::PackageInfo;
use boltffi_backend::{GeneratedOutput, Target, bridge::c::CBridge, target::csharp::CSharpHost};
use boltffi_binding::{Bindings, Native, lower};

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

fn compile_csharp_with_dotnet_when_available(output: &GeneratedOutput, prefix: &str) {
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
