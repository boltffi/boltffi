use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::UNIX_EPOCH,
};

use boltffi_backend::{
    Error,
    target::kotlin::{
        KotlinApiStyle, KotlinCustomMapping, KotlinDesktopLoader, KotlinFactoryStyle, KotlinHost,
    },
};

use super::{
    files_with_host, fixture, rendered_files, rendered_fixture, rendered_fixture_with_host,
    rendered_fixture_with_runtime, rendered_source, source::SourceFixture,
};

#[test]
fn kotlin_target_renders_primitive_function_stack() {
    insta::assert_snapshot!(rendered_fixture("exports/primitive_functions"));
}

#[test]
fn kotlin_target_renders_shared_runtime_support() {
    insta::assert_snapshot!(rendered_fixture_with_runtime("exports/primitive_functions"));
}

#[test]
fn kotlin_target_closes_native_loader_if_body_when_desktop_loader_is_none() {
    let host = KotlinHost::new("com.boltffi.demo", "Demo")
        .expect("Kotlin host")
        .desktop_loader(KotlinDesktopLoader::None);

    let files = files_with_host(&fixture("exports/primitive_functions"), host);
    let (_, contents) = files
        .iter()
        .find(|(path, _)| path.ends_with(".kt"))
        .expect("Kotlin target should render a Kotlin source file");

    let open_braces = contents.matches('{').count();
    let close_braces = contents.matches('}').count();
    assert_eq!(
        open_braces, close_braces,
        "unbalanced braces ({open_braces} open, {close_braces} close) when desktop_loader is \
         none:\n{contents}"
    );
}

#[test]
fn kotlin_target_renders_module_object_api_style() {
    let host = KotlinHost::new("com.boltffi.demo", "Demo")
        .expect("Kotlin host")
        .api_style(KotlinApiStyle::ModuleObject);

    insta::assert_snapshot!(rendered_fixture_with_host(
        "exports/primitive_functions",
        host
    ));
}

#[test]
fn kotlin_target_preserves_unsigned_public_api_and_native_carriers() {
    insta::assert_snapshot!(rendered_fixture("exports/unsigned_functions"));
}

#[test]
fn kotlin_target_renders_string_functions_as_byte_arrays() {
    insta::assert_snapshot!(rendered_fixture("exports/string_functions"));
}

#[test]
fn kotlin_target_renders_direct_records_and_function_bridges() {
    insta::assert_snapshot!(rendered_fixture("exports/direct_records_and_c_style_enums"));
}

#[test]
fn kotlin_target_renders_kdoc_for_documented_records_and_enum_methods() {
    insta::assert_snapshot!(rendered_fixture(
        "associated/direct_record_and_enum_callables"
    ));
}

#[test]
fn kotlin_target_renders_kdoc_for_documented_classes_and_data_enums() {
    insta::assert_snapshot!(rendered_fixture("exports/documented_class_and_data_enum"));
}

#[test]
fn kotlin_target_returns_mutated_direct_record_receivers_from_the_shared_buffer() {
    insta::assert_snapshot!(rendered_fixture("associated/mutable_record_receiver"));
}

#[test]
fn kotlin_target_renders_encoded_records_through_codec_methods() {
    insta::assert_snapshot!(rendered_source(SourceFixture::many([
        "enums/role",
        "records/encoded_user",
        "exports/encoded_record_functions",
    ])));
}

#[test]
fn kotlin_target_sizes_collections_without_numeric_conversions() {
    let rendered = rendered_fixture("records/collection_sizes");

    assert!(!rendered.contains(").toInt()"));

    insta::assert_snapshot!(rendered);
}

#[test]
fn kotlin_target_encodes_direct_records_nested_in_wire_values() {
    let rendered = rendered_fixture("records/encoded_with_direct_record");

    assert!(rendered.contains("internal fun wireSize(): Int {\n        return 12"));
    assert!(rendered.contains("writer.writeI32(x)"));
    assert!(rendered.contains("writer.writeBool(active)\n        writer.pad(3)"));
    assert!(rendered.contains("reader.readBool()\n            ).also { reader.skip(3) }"));
    assert!(rendered.contains("reader.readI32()"));
    assert!(rendered.contains("buffer.`get`(offset + 8)"));
    assert!(rendered.contains("start.writeTo(writer)"));
    assert!(rendered.contains("Point.fromReader(reader)"));
}

#[test]
fn kotlin_target_renders_data_enums_through_codec_methods() {
    insta::assert_snapshot!(rendered_source(SourceFixture::many([
        "records/person",
        "enums/shape",
        "enums/message",
        "exports/encoded_functions",
    ])));
}

#[test]
fn kotlin_target_renders_fallible_returns_as_throwing_functions() {
    insta::assert_snapshot!(rendered_fixture("exports/fallible_returns"));
}

#[test]
fn kotlin_target_overrides_exception_messages() {
    insta::assert_snapshot!(rendered_fixture("enums/error_message"));
}

fn kotlin_compiler() -> Option<PathBuf> {
    let compiler = if cfg!(windows) {
        "kotlinc.bat"
    } else {
        "kotlinc"
    };
    env::split_paths(&env::var_os("PATH")?)
        .find_map(|directory| directory.join(compiler).canonicalize().ok())
        .filter(|compiler| {
            Command::new(compiler)
                .arg("-version")
                .output()
                .is_ok_and(|output| output.status.success())
        })
}

fn run_with_generated_kotlin(
    compiler: &Path,
    label: &str,
    files: Vec<(String, String)>,
    caller_file: &str,
    caller: &str,
) {
    let kotlin_home = env::var_os("KOTLIN_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            compiler
                .parent()
                .and_then(Path::parent)
                .expect("Kotlin compiler installation directory")
                .to_path_buf()
        });
    let coroutines = ["lib", "libexec/lib"]
        .into_iter()
        .map(|directory| {
            kotlin_home
                .join(directory)
                .join("kotlinx-coroutines-core-jvm.jar")
        })
        .find(|path| path.is_file())
        .expect("Kotlin installation must include kotlinx-coroutines-core-jvm.jar");
    let directory = env::temp_dir().join(format!(
        "boltffi-kotlin-{label}-{}-{}",
        std::process::id(),
        UNIX_EPOCH.elapsed().expect("system clock").as_nanos()
    ));
    fs::create_dir_all(&directory).expect("create Kotlin test directory");
    let source_paths = files
        .into_iter()
        .filter(|(path, _)| path.ends_with(".kt"))
        .map(|(path, source)| {
            let path = directory.join(path);
            fs::create_dir_all(path.parent().expect("generated Kotlin directory"))
                .expect("create generated Kotlin directory");
            fs::write(&path, source).expect("write generated Kotlin");
            path
        })
        .collect::<Vec<_>>();
    let caller_path = directory.join(caller_file);
    fs::write(&caller_path, caller).expect("write Kotlin caller");
    let jar = directory.join(format!("{label}.jar"));
    let compilation = Command::new(compiler)
        .arg("-classpath")
        .arg(&coroutines)
        .args(&source_paths)
        .arg(caller_path)
        .args(["-include-runtime", "-d"])
        .arg(&jar)
        .output()
        .expect("run Kotlin compiler");
    assert!(
        compilation.status.success(),
        "generated Kotlin failed to compile in {}:\n{}\n{}",
        directory.display(),
        String::from_utf8_lossy(&compilation.stdout),
        String::from_utf8_lossy(&compilation.stderr)
    );
    let main_class = format!(
        "com.boltffi.demo.{}Kt",
        Path::new(caller_file)
            .file_stem()
            .and_then(|name| name.to_str())
            .expect("Kotlin caller file name")
    );
    let execution = Command::new("java")
        .arg("-classpath")
        .arg(env::join_paths([&jar, &coroutines]).expect("Kotlin runtime classpath"))
        .arg(main_class)
        .output()
        .expect("run Kotlin caller");
    assert!(
        execution.status.success(),
        "Kotlin {label} assertions failed:\n{}\n{}",
        String::from_utf8_lossy(&execution.stdout),
        String::from_utf8_lossy(&execution.stderr)
    );
    fs::remove_dir_all(directory).expect("remove Kotlin test directory");
}

#[test]
fn kotlin_exception_messages_compile_and_preserve_payloads() {
    let Some(compiler) = kotlin_compiler() else {
        eprintln!("Kotlin compiler is unavailable; exception runtime coverage runs in the demo");
        return;
    };

    run_with_generated_kotlin(
        &compiler,
        "error-messages",
        super::files(&fixture("enums/error_message")),
        "ErrorMessages.kt",
        include_str!("../fixtures/kotlin/error_messages.kt"),
    );
}

#[test]
fn kotlin_target_rejects_incompatible_exception_properties() {
    let target = KotlinHost::new("com.boltffi.demo", "Demo")
        .expect("Kotlin host")
        .into_target()
        .expect("Kotlin target");

    [
        (
            "pub enum ServiceError { Failed { message: u32 } }",
            "ServiceError.Failed",
            "message",
        ),
        (
            "pub enum ServiceError { Failed { message: Option<u32> } }",
            "ServiceError.Failed",
            "message",
        ),
        (
            "pub enum ServiceError { Failed { cause: String } }",
            "ServiceError.Failed",
            "cause",
        ),
        (
            "#[repr(C)] pub struct ServiceError { pub message: u32 }",
            "ServiceError",
            "message",
        ),
        (
            "pub struct ServiceError { pub cause: String }",
            "ServiceError",
            "cause",
        ),
        (
            "pub enum ServiceError { Failed { localized_message: String } }",
            "ServiceError.Failed",
            "localizedMessage",
        ),
        (
            "pub enum ServiceError { Failed { localized_message: Option<String> } }",
            "ServiceError.Failed",
            "localizedMessage",
        ),
        (
            "pub struct ServiceError { pub localized_message: String }",
            "ServiceError",
            "localizedMessage",
        ),
    ]
    .into_iter()
    .for_each(|(declaration, expected_scope, expected_name)| {
        let source = format!(
            "#[error] {declaration}\n\
             #[export] pub fn fail() -> Result<(), ServiceError> {{ Ok(()) }}"
        );
        let error = target
            .render(&super::bindings(&source))
            .expect_err("incompatible exception properties must not render");

        assert!(
            matches!(
                &error,
                Error::KotlinNameCollision { scope, name }
                    if scope == expected_scope && name == expected_name
            ),
            "{error:?}"
        );
    });
}

#[test]
fn kotlin_target_renders_custom_types_through_representations() {
    insta::assert_snapshot!(rendered_fixture("exports/custom_type_functions"));
}

#[test]
fn kotlin_target_renders_custom_type_defaults_through_representations() {
    insta::assert_snapshot!(rendered_fixture("records/custom_type_default"));
}

#[test]
fn kotlin_target_renders_custom_type_mappings() {
    let host = KotlinHost::new("com.boltffi.demo", "Demo")
        .expect("Kotlin host")
        .custom_mapping("Email", KotlinCustomMapping::url_string("URI"));

    insta::assert_snapshot!(rendered_fixture_with_host(
        "exports/custom_string_type_functions",
        host
    ));
}

#[test]
fn kotlin_target_qualifies_shadowed_data_enum_payloads() {
    insta::assert_snapshot!(rendered_fixture("enums/error_payload_shadow"));
}

#[test]
fn kotlin_target_qualifies_kotlin_primitive_names_shadowed_by_a_sibling_variant() {
    insta::assert_snapshot!(rendered_fixture("enums/primitive_shadow"));
}

#[test]
fn kotlin_target_renders_result_values_through_shared_codec() {
    insta::assert_snapshot!(rendered_fixture("exports/result_values"));
}

#[test]
fn kotlin_target_renders_map_values_through_shared_codec() {
    insta::assert_snapshot!(rendered_fixture("exports/map_functions"));
}

#[test]
fn kotlin_target_renders_tuples_through_shared_codec() {
    insta::assert_snapshot!(rendered_fixture("exports/tuple_functions"));
}

#[test]
fn kotlin_target_renders_builtin_values_through_shared_codec() {
    insta::assert_snapshot!(rendered_fixture("exports/builtin_functions"));
}

#[test]
fn kotlin_target_encodes_nullable_primitives_as_compact_wire() {
    insta::assert_snapshot!(rendered_fixture("exports/nullable_primitive_functions"));
}

#[test]
fn kotlin_target_renders_class_handles_and_associated_callables() {
    let rendered = rendered_fixture("exports/kotlin_class_handles");

    assert!(rendered.contains("internal fun boltffiHandle(): Long {"));
    assert!(rendered.contains("check(handle != 0L) { \"Engine is closed\" }"));
    assert!(
        rendered.contains("Native.boltffi_method_class_demo_engine_value(this.boltffiHandle())")
    );
    assert!(!rendered.contains("this.handle"));
    assert!(rendered.contains("other.__boltffiTakeHandle()"));
    assert!(rendered.contains("other?.__boltffiTakeHandle() ?: 0L"));
    assert!(!rendered.contains("other.handle"));
    assert!(!rendered.contains("other?.handle"));

    insta::assert_snapshot!(rendered);
}

#[test]
fn kotlin_target_rejects_methods_shadowing_generated_class_members() {
    let render = |source: &str| {
        KotlinHost::new("com.boltffi.demo", "Demo")
            .expect("Kotlin host")
            .into_target()
            .expect("Kotlin target")
            .render(&super::bindings(source))
    };

    let error = render(
        r#"
        pub struct Engine {
            value: i64,
        }

        #[export]
        impl Engine {
            pub fn new() -> Self {
                Self { value: 0 }
            }

            pub fn boltffi_handle(&self) -> i64 {
                self.value
            }
        }
        "#,
    )
    .expect_err("a method shadowing the generated handle accessor must not render");
    assert!(
        matches!(
            &error,
            Error::KotlinNameCollision { scope, name }
                if scope == "Engine" && name == "boltffiHandle()"
        ),
        "{error:?}"
    );

    let error = render(
        r#"
        pub struct Engine {
            value: i64,
        }

        #[export]
        impl Engine {
            pub fn new() -> Self {
                Self { value: 0 }
            }

            pub fn close(&self) {}
        }
        "#,
    )
    .expect_err("a method shadowing the generated close() must not render");
    assert!(
        matches!(
            &error,
            Error::KotlinNameCollision { scope, name }
                if scope == "Engine" && name == "close()"
        ),
        "{error:?}"
    );

    render(
        r#"
        pub struct Engine {
            value: i64,
        }

        #[export]
        impl Engine {
            pub fn new() -> Self {
                Self { value: 0 }
            }

            pub fn close(&self, force: bool) {
                let _ = force;
            }
        }
        "#,
    )
    .expect("close overloads taking parameters remain valid");
}

#[test]
fn kotlin_target_preserves_rust_pascal_type_spelling() {
    insta::assert_snapshot!(rendered_fixture("exports/acronym_class"));
}

#[test]
fn kotlin_target_renders_companion_factory_style() {
    let host = KotlinHost::new("com.boltffi.demo", "Demo")
        .expect("Kotlin host")
        .factory_style(KotlinFactoryStyle::CompanionMethods);

    insta::assert_snapshot!(rendered_fixture_with_host(
        "exports/kotlin_class_handles",
        host
    ));
}

#[test]
fn kotlin_target_renders_async_complete_return_shapes() {
    insta::assert_snapshot!(rendered_fixture("exports/async_complete_return_shapes"));
}

#[test]
fn kotlin_target_renders_async_class_methods() {
    insta::assert_snapshot!(rendered_fixture("exports/async_class_methods"));
}

#[test]
fn kotlin_target_renders_async_initializers_as_suspend_factories() {
    insta::assert_snapshot!(rendered_fixture("exports/async_initializer"));
}

#[test]
fn kotlin_target_renders_closure_parameters() {
    insta::assert_snapshot!(rendered_fixture("exports/closure_parameter"));
}

#[test]
fn kotlin_target_renders_multi_argument_closure_parameters() {
    insta::assert_snapshot!(rendered_fixture("exports/multi_argument_closure_parameter"));
}

#[test]
fn kotlin_target_renders_encoded_closure_parameters() {
    insta::assert_snapshot!(rendered_fixture("exports/encoded_closure_parameter"));
}

#[test]
fn kotlin_target_renders_direct_vector_closure_parameters() {
    insta::assert_snapshot!(rendered_fixture("exports/direct_vector_closure_parameter"));
}

#[test]
fn kotlin_target_renders_closure_result_returns() {
    insta::assert_snapshot!(rendered_fixture("exports/closure_result_return"));
}

#[test]
fn kotlin_target_renders_closure_record_returns() {
    insta::assert_snapshot!(rendered_fixture("exports/closure_direct_record_return"));
}

#[test]
fn kotlin_target_qualifies_module_object_closure_payload_types() {
    let host = KotlinHost::new("com.boltffi.demo", "Demo")
        .expect("Kotlin host")
        .api_style(KotlinApiStyle::ModuleObject);
    let rendered = rendered_fixture_with_host("exports/closure_direct_record_return", host);

    assert!(rendered.contains("fun interface ClosureToDemoPoint"));
    assert!(rendered.contains("fun invoke(): Demo.Point"));
    assert!(rendered.contains("object Demo {"));

    insta::assert_snapshot!(rendered);
}

#[test]
fn kotlin_target_renders_closure_handle_returns() {
    insta::assert_snapshot!(rendered_fixture("exports/closure_callback_handle_return"));
}

#[test]
fn kotlin_target_uses_configured_c_header_in_jni_bridge() {
    let host = KotlinHost::new("com.boltffi.demo", "Demo")
        .expect("Kotlin host")
        .c_header("jni/demo.h");
    let files = files_with_host(&fixture("exports/single_function"), host);
    let jni_source = files
        .iter()
        .find(|(path, _)| path.ends_with("jni_glue.c"))
        .map(|(_, contents)| contents)
        .expect("Kotlin target should render JNI glue");

    assert!(jni_source.contains("#include \"demo.h\""));
    assert!(!jni_source.contains("#include \"boltffi.h\""));

    insta::assert_snapshot!(rendered_files(&files));
}

#[test]
fn kotlin_target_renders_parameter_defaults_as_default_arguments() {
    let rendered = rendered_fixture("exports/parameter_defaults");

    assert!(rendered.contains("fun greet(name: String, greeting: String = \"world\", times: UInt = 3.toUInt(), offset: Long = -1, shout: Boolean = true, ratio: Float = 0.5f, mode: Mode = Mode.SLOW, suffix: String? = null, limit: UShort? = 7.toUShort()): String"));
    assert!(rendered.contains("constructor(port: UShort = 8080.toUShort())"));
    assert!(rendered.contains("private fun new(port: UShort): Server"));
    assert!(!rendered.contains("fun new(port: UShort ="));
    assert!(rendered.contains(
        "suspend fun start(port: UShort, first: Handler? = null, second: Handler? = null)"
    ));
    assert!(rendered.contains("fun port(mapped: Boolean = false)"));
    assert!(rendered.contains("val value: Int = 3"));
    assert!(!rendered.contains("fun new(value: Int"));
    assert!(rendered.contains("fun withValue(value: Int = 5): NamedAmount"));
    assert!(!rendered.contains("external fun boltffi_greet(name: String, greeting: String ="));

    insta::assert_snapshot!(rendered);
}

#[test]
fn kotlin_companion_factory_style_preserves_parameter_defaults() {
    let host = KotlinHost::new("com.boltffi.demo", "Demo")
        .expect("Kotlin host")
        .factory_style(KotlinFactoryStyle::CompanionMethods);
    let rendered = rendered_fixture_with_host("exports/parameter_defaults", host);

    assert!(rendered.contains("fun new(port: UShort = 8080.toUShort()): Server"));
    assert!(!rendered.contains("constructor(port: UShort"));
    assert!(!rendered.contains("private fun new"));
}

#[test]
fn kotlin_defaults_use_mapped_custom_types() {
    let host = KotlinHost::new("com.boltffi.demo", "Demo")
        .expect("Kotlin host")
        .custom_mapping("Email", KotlinCustomMapping::url_string("URI"))
        .custom_mapping("Identifier", KotlinCustomMapping::uuid_string("UUID"));
    let rendered = rendered_fixture_with_host("exports/parameter_defaults", host);

    assert!(
        rendered.contains("email: java.net.URI = java.net.URI.create(\"mailto:ada@example.com\")")
    );
    assert!(
        rendered.contains("email: java.net.URI? = java.net.URI.create(\"mailto:ada@example.com\")")
    );
    assert!(rendered.contains("optionalEmail: java.net.URI? = null"));
    assert!(rendered.contains("identifier: java.util.UUID = java.util.UUID.fromString(\"01234567-89ab-cdef-0123-456789abcdef\")"));
}

#[test]
fn kotlin_target_preserves_long_and_unsigned_long_constructors() {
    let rendered = rendered_fixture("exports/long_initializer");

    assert!(rendered.contains("constructor(balance: Long)"));
    assert!(rendered.contains("private fun new(balance: Long): Ledger"));
    assert!(rendered.contains("constructor(count: ULong)"));
    assert!(rendered.contains("private fun new(count: ULong): Tally"));
    assert!(!rendered.contains("constructor(internal val handle: Long)"));
}

#[test]
fn kotlin_target_writes_integer_limit_defaults_as_literals_kotlin_accepts() {
    let rendered = rendered_fixture("exports/parameter_defaults");

    assert!(rendered.contains("val floor: Long = Long.MIN_VALUE"));
    assert!(rendered.contains("val ceiling: ULong = 18446744073709551615uL"));
    assert!(rendered.contains("val start: Long = Long.MIN_VALUE,"));
    assert!(rendered.contains("val end: ULong = 18446744073709551615uL"));
    assert!(rendered.contains(
        "fun span(start: Long = Long.MIN_VALUE, end: ULong = 18446744073709551615uL): ULong"
    ));
}

#[test]
fn kotlin_target_defaults_a_custom_type_represented_as_an_option_to_null() {
    let rendered = rendered_fixture("exports/parameter_defaults");

    assert!(rendered.contains("val limit: UInt? = null"));
    assert!(rendered.contains("fun throttle(limit: UInt? = null): UInt?"));
}

#[test]
fn kotlin_generated_defaults_and_long_initializers_compile() {
    let Some(compiler) = kotlin_compiler() else {
        eprintln!("Kotlin compiler is unavailable; default-argument coverage runs in the demo");
        return;
    };

    let source =
        SourceFixture::many(["exports/parameter_defaults", "exports/long_initializer"]).read();
    [
        KotlinHost::new("com.boltffi.demo", "Demo").expect("Kotlin host"),
        KotlinHost::new("com.boltffi.demo", "Demo")
            .expect("Kotlin host")
            .custom_mapping("Email", KotlinCustomMapping::url_string("URI"))
            .custom_mapping("Identifier", KotlinCustomMapping::uuid_string("UUID")),
    ]
    .into_iter()
    .for_each(|host| {
        run_with_generated_kotlin(
            &compiler,
            "defaults",
            files_with_host(&source, host),
            "DefaultsAndInitializers.kt",
            include_str!("../fixtures/kotlin/defaults_and_initializers.kt"),
        );
    });
}
