use super::{DOTNET_BUILD_LOCK, bindings, target};
use std::{fs, path::Path, process::Command, time::UNIX_EPOCH};

use boltffi_backend::target::csharp::CSharpHost;

#[test]
fn owned_class_arguments_are_dropped_once_across_csharp_and_rust() {
    let guard = DOTNET_BUILD_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if Command::new("dotnet").arg("--version").output().is_err() {
        eprintln!("skipping C# ownership runtime test: dotnet is unavailable");
        return;
    }

    let source = include_str!("../fixtures/csharp/ownership/lib.rs");
    let output = target(
        CSharpHost::new()
            .namespace("Ownership")
            .expect("namespace")
            .native_library("demo"),
    )
    .render(&bindings(source))
    .expect("ownership bindings");
    assert!(
        output.diagnostics().is_empty(),
        "{:?}",
        output.diagnostics()
    );
    let directory = std::env::temp_dir().join(format!(
        "boltffi-csharp-ownership-{}",
        UNIX_EPOCH.elapsed().expect("system clock").as_nanos()
    ));
    fs::create_dir_all(directory.join("src")).expect("create native fixture directory");
    fs::write(directory.join("src/lib.rs"), source).expect("write native fixture");
    let dependency = Path::new(env!("CARGO_MANIFEST_DIR")).join("../boltffi");
    fs::write(
        directory.join("Cargo.toml"),
        format!(
            "[workspace]\n[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[lib]\ncrate-type = [\"cdylib\"]\n[dependencies]\nboltffi = {{ path = {:?} }}\n",
            dependency.canonicalize().expect("boltffi dependency")
        ),
    )
    .expect("write native fixture manifest");
    let native = Command::new(env!("CARGO"))
        .args(["build", "--offline", "--quiet"])
        .current_dir(&directory)
        .env("CARGO_TARGET_DIR", directory.join("target"))
        .output()
        .expect("build native fixture");
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );

    output
        .files()
        .iter()
        .filter(|file| {
            file.path()
                .as_path()
                .extension()
                .is_some_and(|extension| extension == "cs")
        })
        .for_each(|file| {
            let path = directory.join(file.path().as_path());
            fs::create_dir_all(path.parent().expect("generated source parent"))
                .expect("create generated source directory");
            let contents = file.contents().replace(
                "EntryPoint = \"boltffi_function_demo_missing_entry\"",
                "EntryPoint = \"boltffi_missing_entry\"",
            );
            fs::write(path, contents).expect("write generated C# source");
        });
    fs::write(
        directory.join("Program.cs"),
        include_str!("../fixtures/csharp/ownership/Program.cs"),
    )
    .expect("write C# ownership assertions");
    let library = format!(
        "{}demo{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    fs::write(
        directory.join("Ownership.csproj"),
        format!(
            r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net10.0</TargetFramework>
    <Nullable>enable</Nullable>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
  </PropertyGroup>
  <ItemGroup>
    <None Include="target/debug/{library}" Link="{library}" CopyToOutputDirectory="Always" />
  </ItemGroup>
</Project>
"#
        ),
    )
    .expect("write C# ownership project");
    let managed = Command::new("dotnet")
        .args(["run", "--project", "Ownership.csproj", "--nologo"])
        .current_dir(&directory)
        .env("APPDATA", directory.join("appdata"))
        .output()
        .expect("run C# ownership assertions");
    assert!(
        managed.status.success(),
        "fixture: {}\n{}\n{}",
        directory.display(),
        String::from_utf8_lossy(&managed.stdout),
        String::from_utf8_lossy(&managed.stderr)
    );
    fs::remove_dir_all(directory).expect("remove ownership fixture");
    drop(guard);
}
