use boltffi_ast::PackageInfo;
use boltffi_backend::{
    core::{GeneratedFile, GeneratedOutput},
    target::swift::{SwiftCustomMapping, SwiftHost},
};
use boltffi_binding::{Native, lower};

mod source;

use source::SourceFixture;

fn bindings(source: &str) -> boltffi_binding::Bindings<Native> {
    let file = syn::parse_str(source).expect("valid source fixture");
    let source =
        boltffi_scan::scan_file(file, PackageInfo::new("demo", None)).expect("fixture should scan");
    lower::<Native>(&source).expect("fixture should lower")
}

fn rendered_fixture(name: &str) -> String {
    rendered_source(SourceFixture::one(name))
}

fn rendered_fixture_with_host(name: &str, host: SwiftHost) -> String {
    rendered_source_with_host(SourceFixture::one(name), host)
}

fn rendered_partial_fixture(name: &str) -> String {
    rendered_partial_source(SourceFixture::one(name))
}

fn rendered_source(fixture: SourceFixture) -> String {
    let host = SwiftHost::new("DemoFFI").expect("Swift host");
    rendered_source_with_host(fixture, host)
}

fn rendered_source_with_host(fixture: SourceFixture, host: SwiftHost) -> String {
    rendered_swift_file(&rendered_output_with_host(fixture, host))
}

fn rendered_output(fixture: SourceFixture) -> GeneratedOutput {
    let host = SwiftHost::new("DemoFFI").expect("Swift host");
    rendered_output_with_host(fixture, host)
}

fn rendered_output_with_host(fixture: SourceFixture, host: SwiftHost) -> GeneratedOutput {
    let bindings = bindings(&fixture.read());
    let target = host.into_target().expect("Swift target");
    target.render(&bindings).expect("Swift target renders")
}

fn rendered_partial_source(fixture: SourceFixture) -> String {
    let host = SwiftHost::new("DemoFFI").expect("Swift host");
    let bindings = bindings(&fixture.read());
    let target = host.into_target().expect("Swift target");
    let output = target
        .render_partial(&bindings)
        .expect("Swift target renders partially");
    let coverage = output
        .coverage()
        .unsupported()
        .iter()
        .map(|unsupported| {
            format!(
                "{} {}: {}",
                unsupported.declaration().kind(),
                unsupported.declaration().name(),
                unsupported.reason()
            )
        })
        .collect::<Vec<_>>();
    match coverage.is_empty() {
        true => rendered_swift_file(&output),
        false => format!(
            "{}\n===== coverage =====\n{}",
            rendered_swift_file(&output),
            coverage.join("\n")
        ),
    }
}

fn rendered_swift_file(output: &GeneratedOutput) -> String {
    let swift_file = swift_file(output);
    rendered_snapshot(swift_file, declaration_source(swift_file.contents()))
}

fn rendered_swift_runtime(fixture: SourceFixture) -> String {
    let output = rendered_output(fixture);
    let swift_file = swift_file(&output);
    assert_eq!(
        swift_file
            .contents()
            .matches("@usableFromInline struct WireReader")
            .count(),
        1
    );
    rendered_snapshot(
        swift_file,
        runtime_source(swift_file.contents()).expect("Swift runtime helper"),
    )
}

fn swift_file(output: &GeneratedOutput) -> &GeneratedFile {
    output
        .files()
        .iter()
        .find(|file| {
            file.path()
                .as_path()
                .extension()
                .is_some_and(|extension| extension == "swift")
        })
        .expect("Swift target should render a Swift source file")
}

fn rendered_snapshot(swift_file: &GeneratedFile, contents: &str) -> String {
    format!(
        "===== {} =====\n{}",
        swift_file.path().as_path().display(),
        contents
    )
}

fn declaration_source(contents: &str) -> &str {
    runtime_start(contents).map_or(contents, |start| contents[..start].trim_end())
}

fn runtime_source(contents: &str) -> Option<&str> {
    runtime_start(contents).map(|start| contents[start..].trim_start())
}

fn runtime_start(contents: &str) -> Option<usize> {
    [
        "private final class BoltFFIFutureState",
        "private func boltffiReadDirectStreamBatch",
        "@usableFromInline struct WireReader",
    ]
    .into_iter()
    .filter_map(|marker| contents.find(marker))
    .min()
}

#[test]
fn swift_target_renders_primitive_function_stack() {
    insta::assert_snapshot!(rendered_fixture("exports/single_function"));
}

#[test]
fn swift_target_emits_wire_runtime_once() {
    insta::assert_snapshot!(rendered_swift_runtime(SourceFixture::one(
        "exports/string_functions"
    )));
}

#[test]
fn swift_target_splits_long_parameter_lists() {
    insta::assert_snapshot!(rendered_source(SourceFixture::many([
        "exports/unsigned_functions",
        "exports/long_parameter_class_methods",
    ])));
}

#[test]
fn swift_target_renders_string_function_stack() {
    insta::assert_snapshot!(rendered_fixture("exports/string_functions"));
}

#[test]
fn swift_target_renders_encoded_function_stack() {
    insta::assert_snapshot!(rendered_source(SourceFixture::many([
        "records/person",
        "enums/shape",
        "enums/message",
        "exports/encoded_functions",
    ])));
}

#[test]
fn swift_target_renders_encoded_record_stack() {
    insta::assert_snapshot!(rendered_source(SourceFixture::many([
        "enums/role",
        "records/encoded_user",
        "exports/encoded_record_functions",
    ])));
}

#[test]
fn swift_target_renders_nullable_primitive_function_stack() {
    insta::assert_snapshot!(rendered_fixture("exports/nullable_primitive_functions"));
}

#[test]
fn swift_target_renders_builtin_values_through_shared_codec() {
    insta::assert_snapshot!(rendered_fixture("exports/builtin_functions"));
}

#[test]
fn swift_target_renders_maps_through_shared_codec() {
    insta::assert_snapshot!(rendered_fixture("exports/map_functions"));
}

#[test]
fn swift_target_renders_result_values_through_shared_codec() {
    insta::assert_snapshot!(rendered_fixture("exports/result_values"));
}

#[test]
fn swift_target_renders_async_complete_return_shapes() {
    insta::assert_snapshot!(rendered_source(SourceFixture::many([
        "records/person",
        "exports/async_complete_return_shapes",
    ])));
}

#[test]
fn swift_target_renders_async_class_methods() {
    insta::assert_snapshot!(rendered_fixture("exports/async_class_methods"));
}

#[test]
fn swift_target_renders_stream_protocols() {
    insta::assert_snapshot!(rendered_fixture("stream/protocol_functions"));
}

#[test]
fn swift_target_renders_stream_runtime() {
    insta::assert_snapshot!(rendered_swift_runtime(SourceFixture::one(
        "stream/protocol_functions"
    )));
}

#[test]
fn swift_target_renders_callback_handle_parameters() {
    insta::assert_snapshot!(rendered_partial_fixture(
        "callback/foreign_callback_parameter"
    ));
}

#[test]
fn swift_target_renders_callback_enum_parameters() {
    insta::assert_snapshot!(rendered_partial_fixture("callback/callback_enum_parameter"));
}

#[test]
fn swift_target_renders_callback_record_parameters() {
    insta::assert_snapshot!(rendered_partial_fixture(
        "callback/callback_record_parameter"
    ));
}

#[test]
fn swift_target_renders_callback_record_returns() {
    insta::assert_snapshot!(rendered_partial_fixture("callback/callback_record_return"));
}

#[test]
fn swift_target_renders_callback_encoded_parameters() {
    insta::assert_snapshot!(rendered_partial_fixture(
        "callback/callback_byte_slice_parameter"
    ));
}

#[test]
fn swift_target_renders_callback_encoded_returns() {
    insta::assert_snapshot!(rendered_partial_fixture("callback/callback_encoded_return"));
}

#[test]
fn swift_target_renders_callback_encoded_result_returns() {
    insta::assert_snapshot!(rendered_partial_fixture(
        "callback/callback_encoded_status_result"
    ));
}

#[test]
fn swift_target_renders_callback_direct_result_returns() {
    insta::assert_snapshot!(rendered_partial_fixture("callback/callback_status_result"));
}

#[test]
fn swift_target_renders_callback_optional_scalar_returns() {
    insta::assert_snapshot!(rendered_partial_fixture(
        "callback/callback_optional_scalar_return"
    ));
}

#[test]
fn swift_target_renders_callback_optional_scalar_parameters() {
    insta::assert_snapshot!(rendered_partial_fixture(
        "callback/callback_optional_scalar_parameter"
    ));
}

#[test]
fn swift_target_renders_callback_direct_vectors() {
    insta::assert_snapshot!(rendered_partial_fixture(
        "callback/callback_direct_vector_parameter"
    ));
}

#[test]
fn swift_target_renders_closure_parameters() {
    insta::assert_snapshot!(rendered_fixture("exports/closure_parameter"));
}

#[test]
fn swift_target_renders_direct_vector_closure_parameters() {
    insta::assert_snapshot!(rendered_fixture("exports/direct_vector_closure_parameter"));
}

#[test]
fn swift_target_renders_encoded_closure_return_shapes() {
    insta::assert_snapshot!(rendered_fixture("exports/encoded_closure_return_shapes"));
}

#[test]
fn swift_target_renders_callback_handle_closure_returns() {
    insta::assert_snapshot!(rendered_fixture("exports/closure_callback_handle_return"));
}

#[test]
fn swift_target_renders_returned_closure_functions() {
    insta::assert_snapshot!(rendered_fixture("exports/returned_closure_function"));
}

#[test]
fn swift_target_renders_returned_callback_handles() {
    insta::assert_snapshot!(rendered_partial_fixture("callback/callback_handle_return"));
}

#[test]
fn swift_target_renders_returned_callback_direct_vector_parameters() {
    insta::assert_snapshot!(rendered_partial_fixture(
        "callback/callback_handle_direct_vector_parameter"
    ));
}

#[test]
fn swift_target_renders_returned_callback_direct_vector_returns() {
    insta::assert_snapshot!(rendered_partial_fixture(
        "callback/callback_handle_direct_vector_return"
    ));
}

#[test]
fn swift_target_renders_nullable_returned_callback_handles() {
    insta::assert_snapshot!(rendered_partial_fixture(
        "callback/nullable_callback_handle_return"
    ));
}

#[test]
fn swift_target_renders_callback_method_callback_handle_parameters() {
    insta::assert_snapshot!(rendered_partial_fixture(
        "callback/callback_method_callback_handle_parameter"
    ));
}

#[test]
fn swift_target_renders_callback_method_callback_handle_returns() {
    insta::assert_snapshot!(rendered_partial_fixture(
        "callback/callback_method_callback_handle_return"
    ));
}

#[test]
fn swift_target_renders_async_callback_return_shapes() {
    insta::assert_snapshot!(rendered_fixture("callback/async_callback_return_shapes"));
}

#[test]
fn swift_target_renders_async_callback_unit_results() {
    let rendered = rendered_fixture("callback/async_callback_unit_result");

    assert!(rendered.contains("completion?(completionContext, FfiStatus(code: 0), FfiBuf_u8())"));

    insta::assert_snapshot!(rendered);
}

#[test]
fn swift_target_renders_async_callback_handle_returns() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/async_callback_returning_callback_handle"
    ));
}

#[test]
fn swift_target_renders_fallible_functions_as_throwing_functions() {
    insta::assert_snapshot!(rendered_fixture("exports/fallible_returns"));
}

#[test]
fn swift_target_renders_direct_records_and_c_style_enums() {
    insta::assert_snapshot!(rendered_fixture("exports/direct_records_and_c_style_enums"));
}

#[test]
fn swift_target_renders_documented_record_and_enum_methods() {
    insta::assert_snapshot!(rendered_fixture(
        "associated/direct_record_and_enum_callables"
    ));
}

#[test]
fn swift_target_allocates_scoped_optional_initializer_locals() {
    insta::assert_snapshot!(rendered_fixture(
        "associated/scoped_optional_enum_initializer"
    ));
}

#[test]
fn swift_target_renders_class_handles_and_methods() {
    insta::assert_snapshot!(rendered_fixture("exports/kotlin_class_handles"));
}

#[test]
fn swift_target_preserves_rust_pascal_type_spelling() {
    insta::assert_snapshot!(rendered_fixture("exports/acronym_class"));
}

#[test]
fn swift_target_renders_constants() {
    insta::assert_snapshot!(rendered_fixture("constant/literals_and_accessors"));
}

#[test]
fn swift_target_passes_primitive_direct_vectors() {
    insta::assert_snapshot!(rendered_fixture("direct_vector/primitive_vector_parameter"));
}

#[test]
fn swift_target_passes_mutable_primitive_slices_inout() {
    insta::assert_snapshot!(rendered_fixture(
        "direct_vector/mutable_primitive_slice_parameter"
    ));
}

#[test]
fn swift_target_returns_primitive_direct_vectors() {
    insta::assert_snapshot!(rendered_fixture("direct_vector/primitive_vector_return"));
}

#[test]
fn swift_target_passes_record_direct_vectors() {
    insta::assert_snapshot!(rendered_fixture("direct_vector/record_vector_parameter"));
}

#[test]
fn swift_target_returns_record_direct_vectors() {
    insta::assert_snapshot!(rendered_fixture("direct_vector/record_vector_return"));
}

#[test]
fn swift_target_renders_custom_types_through_representations() {
    insta::assert_snapshot!(rendered_fixture("exports/custom_type_functions"));
}

#[test]
fn swift_target_renders_string_custom_types_through_representations() {
    insta::assert_snapshot!(rendered_fixture("exports/custom_string_type_functions"));
}

#[test]
fn swift_target_renders_custom_type_mappings() {
    let host = SwiftHost::new("DemoFFI")
        .expect("Swift host")
        .custom_mapping("Email", SwiftCustomMapping::url_string("URL"));

    insta::assert_snapshot!(rendered_fixture_with_host(
        "exports/custom_string_type_functions",
        host
    ));
}
