use boltffi_ast::PackageInfo;
use boltffi_backend::{
    CoverageMode, GeneratedOutput, Result,
    target::{
        csharp::CSharpHost, dart::DartHost, dart_web::DartWebHost, java::JavaHost, kmp::KmpHost,
        swift::SwiftHost, typescript::TypeScriptHost,
    },
};
use boltffi_binding::{Bindings, Native, SurfaceLower, Wasm32, lower};

const SOURCE: &str = include_str!("fixtures/source/enums/transparent.rs");

#[test]
fn swift_keeps_native_enum_cases() {
    let target = SwiftHost::new("Demo")
        .expect("Swift host")
        .into_target()
        .expect("Swift target");
    assert_hint_is_optional::<Native>(|bindings| target.render(bindings));
}

#[test]
fn java_keeps_native_variant_classes() {
    let host = JavaHost::new("com.boltffi.demo", "Demo").expect("Java host");
    assert_hint_is_optional::<Native>(|bindings| {
        host.render_with_coverage(bindings, CoverageMode::Complete)
    });
}

#[test]
fn csharp_keeps_native_variant_records() {
    let target = CSharpHost::new().into_target().expect("C# target");
    assert_hint_is_optional::<Native>(|bindings| target.render(bindings));
}

#[test]
fn dart_keeps_native_variant_classes() {
    let target = DartHost::new().into_target().expect("Dart target");
    assert_hint_is_optional::<Native>(|bindings| target.render(bindings));
}

#[test]
fn typescript_keeps_tagged_unions() {
    let target = TypeScriptHost::new("Demo")
        .expect("TypeScript host")
        .into_target();
    assert_hint_is_optional::<Wasm32>(|bindings| target.render(bindings));
}

#[test]
fn dart_web_keeps_tagged_variant_classes() {
    let target = DartWebHost::new("demo")
        .expect("Dart web host")
        .into_target();
    assert_hint_is_optional::<Wasm32>(|bindings| target.render(bindings));
}

#[test]
fn kmp_keeps_its_existing_partial_coverage() {
    let target = KmpHost::new().into_target();
    assert_hint_is_optional::<Native>(|bindings| target.render_partial(bindings));
}

fn assert_hint_is_optional<S: SurfaceLower>(
    render: impl Fn(&Bindings<S>) -> Result<GeneratedOutput>,
) {
    let source = SOURCE
        .split("#[data(impl)]")
        .next()
        .expect("enum declarations");
    let scan = |source: &str| {
        boltffi_scan::scan_file(
            syn::parse_str(source).expect("valid Rust fixture"),
            PackageInfo::new("demo", None),
        )
        .expect("fixture should scan")
    };
    let annotated = lower::<S>(&scan(source)).expect("annotated fixture should lower");
    let ordinary = lower::<S>(&scan(&source.replace("#[boltffi::transparent]", "")))
        .expect("ordinary fixture should lower");
    let annotated = render(&annotated).expect("transparency must not prevent binding generation");
    let ordinary = render(&ordinary).expect("ordinary enums should render");
    assert!(
        annotated == ordinary,
        "the hint must preserve the ordinary target representation"
    );
}
