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
        kotlin::{KotlinApiStyle, KotlinDesktopLoader, KotlinHost},
    },
};
use boltffi_bindgen::{generate::Generation, metadata::BindingMetadataBuild};
use boltffi_binding::{Bindings, Decl, Native, SerializedBindings};
use tempfile::TempDir;

struct KotlinRuntime {
    directory: TempDir,
    java_home: PathBuf,
    rust_library: PathBuf,
    bindings: Bindings<Native>,
    kotlin_sources: Vec<PathBuf>,
}

impl KotlinRuntime {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("Kotlin runtime directory");
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
        let selected_bindings = bindings
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
            .render(&selected_bindings)
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
        let contract = header
            .build_contract(&selected_bindings)
            .expect("C header contract");
        Generation::write_output(
            header
                .render_bridge(&selected_bindings, &contract)
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
            bindings,
            kotlin_sources,
        }
    }

    fn run_source_set(&self, source_set: &str) {
        let platform_directory = self.directory.path().join(source_set);
        fs::create_dir_all(&platform_directory).expect("platform runtime directory");
        let native_library = platform_directory.join(format!(
            "{}boltffi{}",
            env::consts::DLL_PREFIX,
            env::consts::DLL_SUFFIX,
        ));
        self.compile_jni(
            &self
                .directory
                .path()
                .join(format!("src/{source_set}/c/jni_glue.c")),
            &self.directory.path().join("include"),
            &native_library,
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

    fn run_constructors(&self, style: KotlinApiStyle) {
        let selected = self
            .bindings
            .decls()
            .iter()
            .filter(|declaration| {
                matches!(declaration, Decl::Class(class)
                    if class.name().source_spelling() == Some("DefaultedWideCounter"))
            })
            .map(Decl::id)
            .collect::<BTreeSet<_>>();
        assert_eq!(selected.len(), 1, "shared wide counter must be present");
        let bindings = self
            .bindings
            .dependency_closed(&selected)
            .expect("selected wide counter");
        let generated = KotlinHost::new("com.boltffi.constructors", "Demo")
            .expect("Kotlin host")
            .api_style(style)
            .desktop_loader(KotlinDesktopLoader::System)
            .desktop_fallback_library("constructors")
            .expect("JNI library name")
            .c_header("jni/demo.h")
            .into_target()
            .expect("Kotlin target")
            .render(&bindings)
            .expect("generate wide counter bindings");
        let generated_directory = self.directory.path().join(format!("{style:?}"));
        let kotlin_sources = generated
            .files()
            .iter()
            .map(|file| generated_directory.join(file.path().as_path()))
            .filter(|path| path.extension().is_some_and(|extension| extension == "kt"))
            .collect::<Vec<_>>();
        Generation::write_output(generated, &generated_directory).expect("write Kotlin bindings");
        let native_library = generated_directory.join(format!(
            "{}constructors{}",
            env::consts::DLL_PREFIX,
            env::consts::DLL_SUFFIX,
        ));
        self.compile_jni(
            &generated_directory.join("jni/jni_glue.c"),
            &generated_directory.join("jni"),
            &native_library,
        );
        let compiler = which::which("kotlinc")
            .expect("Kotlin compiler")
            .canonicalize()
            .expect("Kotlin compiler path");
        let kotlin_home = compiler
            .parent()
            .and_then(Path::parent)
            .expect("Kotlin installation directory");
        let coroutines = ["lib", "libexec/lib"]
            .into_iter()
            .map(|path| {
                kotlin_home
                    .join(path)
                    .join("kotlinx-coroutines-core-jvm.jar")
            })
            .find(|path| path.is_file())
            .expect("Kotlin coroutines library");
        let bindings_jar = generated_directory.join("bindings.jar");
        self.execute(
            Command::new(&compiler)
                .arg("-classpath")
                .arg(&coroutines)
                .args(&kotlin_sources)
                .arg("-d")
                .arg(&bindings_jar),
            "compile the generated class bindings",
        );
        let caller = include_str!("fixtures/kotlin/ClassConsumer.kt");
        let caller = match style {
            KotlinApiStyle::TopLevel => caller.to_owned(),
            KotlinApiStyle::ModuleObject => {
                caller.replace("DefaultedWideCounter", "Demo.DefaultedWideCounter")
            }
            _ => unreachable!("selected Kotlin API styles"),
        };
        let consumer = generated_directory.join("ClassConsumer.kt");
        fs::write(&consumer, caller).expect("write Kotlin caller");
        let consumer_jar = generated_directory.join("consumer.jar");
        self.execute(
            Command::new(&compiler)
                .arg("-classpath")
                .arg(env::join_paths([&bindings_jar, &coroutines]).expect("Kotlin classpath"))
                .arg(&consumer)
                .args(["-include-runtime", "-d"])
                .arg(&consumer_jar),
            "compile a separate consumer of the generated classes",
        );
        self.execute(
            Command::new(self.java_home.join("bin/java"))
                .arg(format!(
                    "-Djava.library.path={}",
                    generated_directory.display()
                ))
                .arg("-classpath")
                .arg(
                    env::join_paths([&consumer_jar, &bindings_jar, &coroutines])
                        .expect("Kotlin runtime classpath"),
                )
                .arg("com.boltffi.constructors.ClassConsumerKt"),
            "run constructors and ownership checks through Rust",
        );
    }

    fn compile_jni(&self, source: &Path, headers: &Path, library: &Path) {
        let rust_directory = self.rust_library.parent().expect("Rust library directory");
        let jni_platform = if cfg!(target_os = "macos") {
            "darwin"
        } else {
            "linux"
        };
        self.execute(
            Command::new("cc")
                .args(["-shared", "-fPIC"])
                .arg("-I")
                .arg(headers)
                .arg("-I")
                .arg(self.java_home.join("include"))
                .arg("-I")
                .arg(self.java_home.join("include").join(jni_platform))
                .arg(source)
                .arg("-L")
                .arg(rust_directory)
                .arg("-ldemo")
                .arg(format!("-Wl,-rpath,{}", rust_directory.display()))
                .arg("-o")
                .arg(library),
            "compile generated JNI against the Rust fixture",
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
    let runtime = KotlinRuntime::new();
    ["jvmMain", "androidMain"]
        .into_iter()
        .for_each(|source_set| runtime.run_source_set(source_set));
}

#[test]
#[ignore = "requires Kotlin and a JDK"]
fn kotlin_long_constructors_reach_rust_and_preserve_handle_ownership() {
    let runtime = KotlinRuntime::new();
    [KotlinApiStyle::TopLevel, KotlinApiStyle::ModuleObject]
        .into_iter()
        .for_each(|style| runtime.run_constructors(style));
}
