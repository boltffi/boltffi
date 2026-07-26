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
fn kotlin_target_renders_custom_types_through_representations() {
    insta::assert_snapshot!(rendered_fixture("exports/custom_type_functions"));
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
    assert!(rendered.contains("check(!__boltffi_closed.get()) { \"Engine is closed\" }"));
    assert!(
        rendered.contains("Native.boltffi_method_class_demo_engine_value(this.boltffiHandle())")
    );
    assert!(!rendered.contains("this.handle"));
    assert!(rendered.contains("other.boltffiHandle()"));
    assert!(rendered.contains("other?.boltffiHandle() ?: 0L"));
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
