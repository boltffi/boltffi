#![cfg(unix)]

use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use boltffi_backend::{
    bridge::c::CBridge,
    core::bridge::BridgeBackend,
    target::{
        jvm::DesktopLoader,
        kmp::{KMP_GENERATED_C_HEADER_DIR, KmpHost},
    },
};
use boltffi_bindgen::{generate::Generation, metadata::BindingMetadataBuild};
use boltffi_binding::{Decl, SerializedBindings};
use tempfile::TempDir;

struct KmpRuntime {
    directory: TempDir,
    java_home: PathBuf,
    rust_library: PathBuf,
    kotlin_sources: Vec<PathBuf>,
}

impl KmpRuntime {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("KMP runtime directory");
        let java_home =
            PathBuf::from(env::var_os("JAVA_HOME").expect("JAVA_HOME must point to a JDK"));
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../examples/demo/Cargo.toml")
            .canonicalize()
            .expect("shared Rust demo manifest");
        let cargo_target = directory.path().join("rust");
        let envelopes = BindingMetadataBuild::new(&manifest)
            .cargo_args([format!(
                "--target-dir={}",
                cargo_target.join("metadata").display()
            )])
            .read()
            .expect("compile the shared Rust fixture and read its metadata");
        let native_build = Command::new(env!("CARGO"))
            .args(["build", "--lib", "--manifest-path"])
            .arg(&manifest)
            .arg("--target-dir")
            .arg(&cargo_target)
            .output()
            .expect("compile the shared Rust runtime library");
        assert!(
            native_build.status.success(),
            "shared Rust runtime build failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&native_build.stdout),
            String::from_utf8_lossy(&native_build.stderr)
        );
        let bindings = envelopes
            .into_iter()
            .find_map(|envelope| match envelope.into_bindings() {
                SerializedBindings::Native(bindings)
                    if bindings.package().name().as_path_string() == "demo" =>
                {
                    Some(bindings)
                }
                _ => None,
            })
            .expect("native demo bindings");
        let selected = bindings
            .decls()
            .iter()
            .filter(|declaration| {
                matches!(declaration, Decl::Function(function)
                if function.name().source_spelling() == Some("add_default"))
            })
            .map(Decl::id)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            selected.len(),
            1,
            "shared defaults function must be present"
        );
        let bindings = bindings
            .dependency_closed(&selected)
            .expect("selected default function");
        let generated = KmpHost::new()
            .package_name("com.boltffi.defaults")
            .module_name("Demo")
            .android_library("boltffi")
            .expect("Android JNI library name")
            .desktop_fallback_library("boltffi")
            .expect("desktop JNI library name")
            .desktop_loader(DesktopLoader::System)
            .into_target()
            .render(&bindings)
            .expect("generate complete KMP bindings for the selected function");
        let kotlin_sources = generated
            .files()
            .iter()
            .map(|file| file.path().as_path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "kt"))
            .map(|path| directory.path().join(path))
            .collect();
        Generation::write_output(generated, directory.path()).expect("write KMP bindings");
        let header = CBridge::new(format!("include/{KMP_GENERATED_C_HEADER_DIR}/demo.h"))
            .expect("C header bridge");
        let contract = header.build_contract(&bindings).expect("C header contract");
        Generation::write_output(
            header
                .render_bridge(&bindings, &contract)
                .expect("C header"),
            directory.path(),
        )
        .expect("write C header");
        fs::write(
            directory.path().join("DefaultConsumer.kt"),
            include_str!("fixtures/kmp/DefaultConsumer.kt"),
        )
        .expect("write Kotlin caller");
        let rust_library = cargo_target.join("debug").join(format!(
            "{}demo{}",
            env::consts::DLL_PREFIX,
            env::consts::DLL_SUFFIX,
        ));
        assert!(
            rust_library.is_file(),
            "compiled Rust library must exist at {}",
            rust_library.display()
        );

        Self {
            directory,
            java_home,
            rust_library,
            kotlin_sources,
        }
    }

    fn run_source_set(&self, source_set: &str) {
        let platform_directory = self.directory.path().join(source_set);
        fs::create_dir_all(&platform_directory).expect("platform runtime directory");
        let rust_directory = self.rust_library.parent().expect("Rust library directory");
        let native_library = platform_directory.join(format!(
            "{}boltffi{}",
            env::consts::DLL_PREFIX,
            env::consts::DLL_SUFFIX,
        ));
        let jni_platform = if cfg!(target_os = "macos") {
            "darwin"
        } else {
            "linux"
        };
        self.execute(
            Command::new("cc")
                .args(["-shared", "-fPIC"])
                .arg("-I")
                .arg(self.directory.path().join("include"))
                .arg("-I")
                .arg(self.java_home.join("include"))
                .arg("-I")
                .arg(self.java_home.join("include").join(jni_platform))
                .arg(
                    self.directory
                        .path()
                        .join(format!("src/{source_set}/c/jni_glue.c")),
                )
                .arg("-L")
                .arg(rust_directory)
                .arg("-ldemo")
                .arg(format!("-Wl,-rpath,{}", rust_directory.display()))
                .arg("-o")
                .arg(&native_library),
            "compile generated JNI against the Rust fixture",
        );
        let consumer = self.directory.path().join("DefaultConsumer.kt");
        let common_directory = self.directory.path().join("src/commonMain/kotlin");
        let platform_source_directory = self
            .directory
            .path()
            .join(format!("src/{source_set}/kotlin"));
        let common_sources = self
            .kotlin_sources
            .iter()
            .filter(|path| path.starts_with(&common_directory))
            .chain([&consumer])
            .collect::<Vec<_>>();
        let platform_sources = self
            .kotlin_sources
            .iter()
            .filter(|path| path.starts_with(&platform_source_directory));
        let jar = platform_directory.join("defaults.jar");
        self.execute(
            Command::new("kotlinc")
                .arg("-Xmulti-platform")
                .arg(format!(
                    "-Xcommon-sources={}",
                    common_sources
                        .iter()
                        .map(|path| path.to_str().expect("Kotlin source path"))
                        .collect::<Vec<_>>()
                        .join(",")
                ))
                .args(&common_sources)
                .args(platform_sources)
                .args(["-include-runtime", "-d"])
                .arg(&jar),
            "compile the caller as common Kotlin code",
        );
        self.execute(
            Command::new(self.java_home.join("bin/java"))
                .arg(format!(
                    "-Djava.library.path={}",
                    platform_directory.display()
                ))
                .arg("-classpath")
                .arg(&jar)
                .arg("com.boltffi.defaults.DefaultConsumerKt"),
            "run common defaults through the Rust fixture",
        );
        let consumer_jar = platform_directory.join("consumer.jar");
        self.execute(
            Command::new("kotlinc")
                .arg("-classpath")
                .arg(&jar)
                .arg(&consumer)
                .args(["-include-runtime", "-d"])
                .arg(&consumer_jar),
            "compile a separate consumer of the generated library",
        );
        self.execute(
            Command::new(self.java_home.join("bin/java"))
                .arg(format!(
                    "-Djava.library.path={}",
                    platform_directory.display()
                ))
                .arg("-classpath")
                .arg(env::join_paths([&consumer_jar, &jar]).expect("Kotlin runtime classpath"))
                .arg("com.boltffi.defaults.DefaultConsumerKt"),
            "run separately compiled default calls through Rust",
        );
        let missing_required = platform_directory.join("MissingRequired.kt");
        fs::write(
            &missing_required,
            "package com.boltffi.defaults\nfun missingRequired() = addDefault()\n",
        )
        .expect("write caller missing a required argument");
        let rejection = Command::new("kotlinc")
            .arg("-classpath")
            .arg(&jar)
            .arg(missing_required)
            .arg("-d")
            .arg(platform_directory.join("invalid.jar"))
            .output()
            .expect("compile caller missing a required argument");
        assert!(!rejection.status.success(), "right must remain required");
        assert!(
            String::from_utf8_lossy(&rejection.stderr).contains("right"),
            "compiler must identify the missing right argument: {}",
            String::from_utf8_lossy(&rejection.stderr)
        );
    }

    fn execute(&self, command: &mut Command, operation: &str) {
        let output = command
            .current_dir(self.directory.path())
            .output()
            .unwrap_or_else(|error| panic!("{operation} could not start: {error}"));
        assert!(
            output.status.success(),
            "{operation} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
#[ignore = "requires Kotlin and a JDK"]
fn kmp_defaults_reach_rust_from_common_and_platform_consumers() {
    let runtime = KmpRuntime::new();
    ["jvmMain", "androidMain"]
        .into_iter()
        .for_each(|source_set| runtime.run_source_set(source_set));
}

#[test]
#[ignore = "requires Kotlin, a JDK, the Android NDK and the arm64 Android Rust target"]
fn android_bindings_load_desktop_natives_from_application_resources() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let fixture = tempfile::Builder::new()
        .prefix(".kotlin-desktop-test-")
        .tempdir_in(workspace)
        .expect("Kotlin desktop fixture");
    let source_directory = fixture.path().join("src");
    fs::create_dir_all(&source_directory).unwrap();
    let source = fs::read_to_string(
        workspace.join("boltffi_backend/tests/fixtures/source/exports/long_initializer.rs"),
    )
    .expect("shared class fixture");
    fs::write(
        source_directory.join("lib.rs"),
        format!("use boltffi::export;\n{source}"),
    )
    .unwrap();
    let manifest = serde_json::json!({
        "package": { "name": "journey-bindings", "version": "0.1.0", "edition": "2024" },
        "lib": { "crate-type": ["staticlib", "cdylib", "rlib"] },
        "dependencies": { "boltffi": { "path": workspace.join("boltffi") } },
        "workspace": {}
    });
    fs::write(
        fixture.path().join("Cargo.toml"),
        toml::to_string(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(
        fixture.path().join("boltffi.toml"),
        r#"
[package]
name = "journey-bindings"

[targets.apple]
enabled = false

[targets.android]
output = "android"
architectures = ["arm64"]

[targets.android.kotlin]
package = "com.boltffi.desktop"
module_name = "Journey"
api_style = "module_object"
factory_style = "companion_methods"

[targets.android.kotlin.desktop_pack]
enabled = true
output = "resources/native"
"#,
    )
    .unwrap();
    let cargo_target = fixture.path().join("rust");
    execute(
        Command::new(env!("CARGO_BIN_EXE_boltffi"))
            .args(["generate", "kotlin"])
            .current_dir(fixture.path())
            .env("CARGO_TARGET_DIR", &cargo_target),
    );
    let compiler = which::which("kotlinc").unwrap().canonicalize().unwrap();
    let kotlin_home = compiler.parent().and_then(Path::parent).unwrap();
    let coroutines = ["lib", "libexec/lib"]
        .into_iter()
        .map(|path| {
            kotlin_home
                .join(path)
                .join("kotlinx-coroutines-core-jvm.jar")
        })
        .find(|path| path.is_file())
        .expect("Kotlin coroutines library");
    let kotlin_sources = walkdir::WalkDir::new(fixture.path().join("android/kotlin"))
        .into_iter()
        .map(|entry| entry.unwrap().into_path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "kt"))
        .collect::<Vec<_>>();
    assert!(!kotlin_sources.is_empty(), "generated Kotlin bindings");
    let consumer = fixture.path().join("DesktopConsumer.kt");
    fs::write(
        &consumer,
        include_str!("fixtures/kotlin/DesktopConsumer.kt"),
    )
    .unwrap();
    let jar = fixture.path().join("consumer.jar");
    execute(
        Command::new(&compiler)
            .arg("-classpath")
            .arg(&coroutines)
            .args(&kotlin_sources)
            .arg(&consumer)
            .args(["-include-runtime", "-d"])
            .arg(&jar),
    );
    let resources = fixture.path().join("resources");
    let system_libraries = fixture.path().join("system-libraries");
    fs::create_dir_all(&system_libraries).unwrap();
    let java_home = PathBuf::from(env::var_os("JAVA_HOME").expect("JAVA_HOME must point to a JDK"));
    let mut java = Command::new(java_home.join("bin/java"));
    java.arg(format!(
        "-Djava.library.path={}",
        system_libraries.display()
    ))
    .arg("-classpath")
    .arg(env::join_paths([&jar, &coroutines, &resources]).unwrap())
    .arg("com.boltffi.desktop.DesktopConsumerKt");
    let missing = java.output().unwrap();
    assert!(
        !missing.status.success(),
        "desktop JNI libraries must be present"
    );
    let failure = String::from_utf8_lossy(&missing.stderr);
    assert!(failure.contains("journey_bindings_jni"), "{failure}");
    assert!(
        failure.contains("targets.android.kotlin.desktop_pack.enabled"),
        "{failure}"
    );
    assert!(
        failure.contains("Caused by: java.lang.UnsatisfiedLinkError: no journey_bindings_jni"),
        "{failure}"
    );

    execute(
        Command::new(env!("CARGO_BIN_EXE_boltffi"))
            .args(["pack", "android"])
            .current_dir(fixture.path())
            .env("CARGO_TARGET_DIR", &cargo_target),
    );
    assert!(
        fixture
            .path()
            .join("android/jniLibs/arm64-v8a/libjourney-bindings.so")
            .is_file()
    );
    let packaged_jni = walkdir::WalkDir::new(resources.join("native"))
        .into_iter()
        .map(|entry| entry.unwrap().into_path())
        .find(|path| {
            path.file_name().is_some_and(|name| {
                name == format!(
                    "{}journey_bindings_jni{}",
                    env::consts::DLL_PREFIX,
                    env::consts::DLL_SUFFIX
                )
                .as_str()
            })
        })
        .expect("desktop JNI must be written into application resources");
    execute(&mut java);

    fs::copy(
        &packaged_jni,
        system_libraries.join(packaged_jni.file_name().unwrap()),
    )
    .unwrap();
    fs::write(&packaged_jni, b"invalid native library").unwrap();
    execute(&mut java);
}

fn execute(command: &mut Command) {
    let output = command.output().expect("run test command");
    assert!(
        output.status.success(),
        "{command:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
