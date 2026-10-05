use boltffi_ast::PackageInfo;
use boltffi_backend::target::{dart_web::DartWebHost, typescript::TypeScriptHost};
use boltffi_binding::{Wasm32, lower};

#[test]
fn wasm_hosts_reject_encoded_mutation_without_a_writeback_channel() {
    let source = boltffi_scan::scan_file(
        syn::parse_str(
            r#"
            #[data]
            pub struct Message { pub text: String }
            #[export]
            pub fn update(value: &mut Message) { value.text.push_str("updated"); }
            "#,
        )
        .unwrap(),
        PackageInfo::new("probe", None),
    )
    .unwrap();
    let bindings = lower::<Wasm32>(&source).unwrap();
    let dart = DartWebHost::new("probe")
        .unwrap()
        .into_target()
        .render(&bindings)
        .unwrap_err();
    let typescript = TypeScriptHost::new("probe")
        .unwrap()
        .into_target()
        .render(&bindings)
        .unwrap_err();
    assert!(
        dart.to_string().contains("mutable encoded parameter"),
        "{dart}"
    );
    assert!(
        typescript.to_string().contains("mutable encoded parameter"),
        "{typescript}"
    );
}
