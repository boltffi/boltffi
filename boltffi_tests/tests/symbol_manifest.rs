use std::collections::BTreeSet;

#[cfg(not(miri))]
use boltffi_ast::PackageInfo;
#[cfg(not(miri))]
use boltffi_backend::bridge::c::CBridge;
#[cfg(not(miri))]
use boltffi_backend::core::bridge::BridgeBackend;
#[cfg(not(miri))]
use boltffi_binding::{
    Bindings, Decl, EnumDecl, ErrorDecl, ExportedCallable, IncomingParam, IntoRust, Native,
    OutOfRust, ParamPlan, Receive, RecordDecl, ReturnPlan, TypeRef, lower,
};
#[cfg(not(miri))]
use boltffi_scan::{ScanInput, scan_package};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PendingExpansion {
    item: &'static str,
    row: &'static str,
    failure: &'static str,
}

const PENDING_EXPANSIONS: &[PendingExpansion] = &[PendingExpansion {
    item: "try_make_adder",
    row: "return:closure:out-pointer+error:encoded:return-slot",
    failure: "expected `FfiBuf`, found `FfiStatus`",
}];

#[cfg(not(miri))]
fn source_contract() -> boltffi_ast::SourceContract {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = manifest.join("src").join("lib.rs");
    scan_package(
        &ScanInput::new(source, PackageInfo::new("boltffi_tests", None))
            .with_manifest_dir(manifest),
    )
    .expect("scan test package")
    .root_with_support()
}

#[cfg(not(miri))]
fn c_published_function_symbols() -> BTreeSet<String> {
    let bindings = native_bindings();
    let bridge = CBridge::default_header().expect("C bridge");
    let contract = bridge.build_contract(&bindings).expect("C bridge contract");
    contract
        .functions()
        .iter()
        .chain(
            contract
                .callbacks()
                .iter()
                .flat_map(|callback| [callback.register(), callback.create_handle()]),
        )
        .map(|function| function.name().to_owned())
        .collect()
}

#[cfg(not(miri))]
fn native_symbol_names() -> BTreeSet<String> {
    native_bindings()
        .symbols()
        .symbols()
        .iter()
        .map(|symbol| symbol.name().as_str().to_owned())
        .collect()
}

#[cfg(not(miri))]
fn native_bindings() -> Bindings<Native> {
    let source = source_contract();
    lower::<Native>(&source).expect("lower test package")
}

fn asserted_symbols() -> BTreeSet<String> {
    boltffi_tests::contract::ASSERTED_SYMBOLS
        .iter()
        .map(|symbol| (*symbol).to_owned())
        .collect()
}

fn c_harness_symbols() -> BTreeSet<String> {
    boltffi_tests::contract::C_HARNESS_SYMBOLS
        .iter()
        .map(|symbol| (*symbol).to_owned())
        .collect()
}

fn signature_only_c_symbol(symbol: &str) -> bool {
    symbol.ends_with("_panic_message")
        || symbol.ends_with("_cancel")
        || symbol
            == "boltffi_async_method_class_boltffi_tests_results_cancellable_task_long_running_task_complete"
}

#[cfg(not(miri))]
fn assert_symbol_sets_eq(
    actual_name: &str,
    actual: BTreeSet<String>,
    expected_name: &str,
    expected: BTreeSet<String>,
) {
    let missing = expected
        .difference(&actual)
        .cloned()
        .collect::<Vec<String>>();
    let extra = actual
        .difference(&expected)
        .cloned()
        .collect::<Vec<String>>();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{actual_name} differs from {expected_name}; missing: {missing:#?}; extra: {extra:#?}"
    );
}

#[test]
#[cfg(not(miri))]
fn generated_signature_assertions_cover_every_c_contract_function() {
    assert_symbol_sets_eq(
        "asserted symbols",
        asserted_symbols(),
        "C published functions",
        c_published_function_symbols(),
    );
}

#[test]
#[cfg(not(miri))]
fn generated_signature_assertions_cover_every_native_symbol() {
    assert_symbol_sets_eq(
        "asserted symbols",
        asserted_symbols(),
        "native symbols",
        native_symbol_names(),
    );
}

#[test]
#[cfg(not(miri))]
fn c_contract_functions_name_native_symbols() {
    assert_symbol_sets_eq(
        "C published functions",
        c_published_function_symbols(),
        "native symbols",
        native_symbol_names(),
    );
}

#[test]
fn c_harness_executes_every_runtime_contract_symbol() {
    let c_symbols = c_harness_symbols();
    let missing = asserted_symbols()
        .difference(&c_symbols)
        .filter(|symbol| !signature_only_c_symbol(symbol))
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "C harness misses runtime symbols: {missing:#?}"
    );
}

#[test]
fn c_harness_exercises_the_async_cancellation_path() {
    let c_symbols = c_harness_symbols();
    [
        "boltffi_method_class_boltffi_tests_results_cancellable_task_long_running_task",
        "boltffi_async_method_class_boltffi_tests_results_cancellable_task_long_running_task_poll",
        "boltffi_async_method_class_boltffi_tests_results_cancellable_task_long_running_task_cancel",
        "boltffi_async_method_class_boltffi_tests_results_cancellable_task_long_running_task_free",
    ]
    .into_iter()
    .for_each(|symbol| assert!(c_symbols.contains(symbol), "C harness misses {symbol}"));
}

#[test]
fn quarantined_rows_have_registered_failures() {
    let source = include_str!("../src/quarantine.rs");
    let actual = PENDING_EXPANSIONS
        .iter()
        .inspect(|pending| assert!(source.contains(pending.item)))
        .copied()
        .collect::<BTreeSet<_>>();
    let expected = [PendingExpansion {
        item: "try_make_adder",
        row: "return:closure:out-pointer+error:encoded:return-slot",
        failure: "expected `FfiBuf`, found `FfiStatus`",
    }]
    .into_iter()
    .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn generated_signature_assertions_cover_mutable_byte_fixture() {
    assert!(asserted_symbols().contains("boltffi_function_boltffi_tests_bytes_fill_bytes"));
}

#[test]
#[cfg(not(miri))]
fn mutable_byte_fixture_lowers_to_encoded_mutable_receive() {
    let bindings = native_bindings();
    let function_names = bindings
        .decls()
        .iter()
        .filter_map(|decl| match decl {
            Decl::Function(function) => Some(function.name().as_path_string()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let function = bindings
        .decls()
        .iter()
        .find_map(|decl| match decl {
            Decl::Function(function) if function.name().as_path_string() == "fill::bytes" => {
                Some(function)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("fill_bytes function in {function_names:?}"));
    let parameter = &function.callable().params()[0];
    match parameter.payload() {
        IncomingParam::Value(ParamPlan::Encoded { ty, receive, .. }) => {
            assert_eq!(ty, &TypeRef::Bytes);
            assert_eq!(receive, &Receive::ByMutRef);
        }
        other => panic!("expected mutable encoded byte parameter, got {other:?}"),
    }
}

#[test]
#[cfg(not(miri))]
fn lowering_produces_the_claimed_crossing_matrix() {
    let actual = crossing_rows(&native_bindings());
    let expected = [
        "decl:callback",
        "decl:class",
        "decl:constant",
        "decl:custom-type",
        "decl:enum:c-style",
        "decl:enum:data",
        "decl:function",
        "decl:record:direct",
        "decl:record:encoded",
        "decl:stream",
        "error:encoded:return-slot",
        "error:none",
        "param:closure",
        "param:direct:by-mut-ref",
        "param:direct:by-ref",
        "param:direct:by-value",
        "param:direct-vec",
        "param:encoded:by-mut-ref",
        "param:encoded:by-ref",
        "param:encoded:by-value",
        "param:handle:by-value",
        "param:scalar-option",
        "receiver:by-mut-ref",
        "receiver:by-ref",
        "return:closure:out-pointer",
        "return:direct:out-pointer",
        "return:direct:return-slot",
        "return:direct-vec:return-slot",
        "return:encoded:out-pointer",
        "return:encoded:return-slot",
        "return:handle:out-pointer",
        "return:handle:return-slot",
        "return:scalar-option:return-slot",
        "return:void",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
}

#[cfg(not(miri))]
fn crossing_rows(bindings: &Bindings<Native>) -> BTreeSet<&'static str> {
    let mut rows = BTreeSet::new();
    bindings
        .decls()
        .iter()
        .for_each(|decl| collect_decl_rows(decl, &mut rows));
    rows
}

#[cfg(not(miri))]
fn collect_decl_rows(decl: &Decl<Native>, rows: &mut BTreeSet<&'static str>) {
    match decl {
        Decl::Record(record) => collect_record_rows(record, rows),
        Decl::Enum(enumeration) => collect_enum_rows(enumeration, rows),
        Decl::Function(function) => {
            rows.insert("decl:function");
            collect_callable_rows(function.callable(), rows);
        }
        Decl::Class(class) => {
            rows.insert("decl:class");
            class
                .initializers()
                .iter()
                .for_each(|initializer| collect_callable_rows(initializer.callable(), rows));
            class
                .methods()
                .iter()
                .for_each(|method| collect_callable_rows(method.callable(), rows));
        }
        Decl::Callback(_) => {
            rows.insert("decl:callback");
        }
        Decl::Stream(_) => {
            rows.insert("decl:stream");
        }
        Decl::Constant(_) => {
            rows.insert("decl:constant");
        }
        Decl::CustomType(_) => {
            rows.insert("decl:custom-type");
        }
        other => panic!("unexpected declaration row {other:?}"),
    }
}

#[cfg(not(miri))]
fn collect_record_rows(record: &RecordDecl<Native>, rows: &mut BTreeSet<&'static str>) {
    match record {
        RecordDecl::Direct(record) => {
            rows.insert("decl:record:direct");
            record
                .initializers()
                .iter()
                .for_each(|initializer| collect_callable_rows(initializer.callable(), rows));
            record
                .methods()
                .iter()
                .for_each(|method| collect_callable_rows(method.callable(), rows));
        }
        RecordDecl::Encoded(record) => {
            rows.insert("decl:record:encoded");
            record
                .initializers()
                .iter()
                .for_each(|initializer| collect_callable_rows(initializer.callable(), rows));
            record
                .methods()
                .iter()
                .for_each(|method| collect_callable_rows(method.callable(), rows));
        }
        other => panic!("unexpected record row {other:?}"),
    }
}

#[cfg(not(miri))]
fn collect_enum_rows(enumeration: &EnumDecl<Native>, rows: &mut BTreeSet<&'static str>) {
    match enumeration {
        EnumDecl::CStyle(enumeration) => {
            rows.insert("decl:enum:c-style");
            enumeration
                .initializers()
                .iter()
                .for_each(|initializer| collect_callable_rows(initializer.callable(), rows));
            enumeration
                .methods()
                .iter()
                .for_each(|method| collect_callable_rows(method.callable(), rows));
        }
        EnumDecl::Data(enumeration) => {
            rows.insert("decl:enum:data");
            enumeration
                .initializers()
                .iter()
                .for_each(|initializer| collect_callable_rows(initializer.callable(), rows));
            enumeration
                .methods()
                .iter()
                .for_each(|method| collect_callable_rows(method.callable(), rows));
        }
        other => panic!("unexpected enum row {other:?}"),
    }
}

#[cfg(not(miri))]
fn collect_callable_rows(callable: &ExportedCallable<Native>, rows: &mut BTreeSet<&'static str>) {
    match callable.receiver() {
        Some(Receive::ByValue) => {
            rows.insert("receiver:by-value");
        }
        Some(Receive::ByRef) => {
            rows.insert("receiver:by-ref");
        }
        Some(Receive::ByMutRef) => {
            rows.insert("receiver:by-mut-ref");
        }
        None => {}
        other => panic!("unexpected receiver row {other:?}"),
    }
    callable
        .params()
        .iter()
        .for_each(|param| collect_param_rows(param.payload(), rows));
    collect_return_rows(callable.returns().plan(), rows);
    collect_error_rows(callable.error(), rows);
}

#[cfg(not(miri))]
fn collect_param_rows(param: &IncomingParam<Native>, rows: &mut BTreeSet<&'static str>) {
    match param {
        IncomingParam::Value(plan) => collect_value_param_rows(plan, rows),
        IncomingParam::Closure(_) => {
            rows.insert("param:closure");
        }
    }
}

#[cfg(not(miri))]
fn collect_value_param_rows(plan: &ParamPlan<Native, IntoRust>, rows: &mut BTreeSet<&'static str>) {
    match plan {
        ParamPlan::Direct { receive, .. } => {
            rows.insert(receive_row("param:direct", *receive));
        }
        ParamPlan::Encoded { receive, .. } => {
            rows.insert(receive_row("param:encoded", *receive));
        }
        ParamPlan::Handle { receive, .. } => {
            rows.insert(receive_row("param:handle", *receive));
        }
        ParamPlan::ScalarOption { .. } => {
            rows.insert("param:scalar-option");
        }
        ParamPlan::DirectVec { .. } => {
            rows.insert("param:direct-vec");
        }
        other => panic!("unexpected parameter row {other:?}"),
    }
}

#[cfg(not(miri))]
fn collect_return_rows(plan: &ReturnPlan<Native, OutOfRust>, rows: &mut BTreeSet<&'static str>) {
    rows.insert(match plan {
        ReturnPlan::Void => "return:void",
        ReturnPlan::DirectViaReturnSlot { .. } => "return:direct:return-slot",
        ReturnPlan::EncodedViaReturnSlot { .. } => "return:encoded:return-slot",
        ReturnPlan::HandleViaReturnSlot { .. } => "return:handle:return-slot",
        ReturnPlan::ScalarOptionViaReturnSlot { .. } => "return:scalar-option:return-slot",
        ReturnPlan::DirectVecViaReturnSlot { .. } => "return:direct-vec:return-slot",
        ReturnPlan::DirectViaOutPointer { .. } => "return:direct:out-pointer",
        ReturnPlan::EncodedViaOutPointer { .. } => "return:encoded:out-pointer",
        ReturnPlan::HandleViaOutPointer { .. } => "return:handle:out-pointer",
        ReturnPlan::ClosureViaOutPointer(_) => "return:closure:out-pointer",
        other => panic!("unexpected return row {other:?}"),
    });
}

#[cfg(not(miri))]
fn collect_error_rows(error: &ErrorDecl<Native, OutOfRust>, rows: &mut BTreeSet<&'static str>) {
    rows.insert(match error {
        ErrorDecl::None(_) => "error:none",
        ErrorDecl::StatusViaReturnSlot { .. } => "error:status:return-slot",
        ErrorDecl::StatusViaOutPointer { .. } => "error:status:out-pointer",
        ErrorDecl::EncodedViaReturnSlot { .. } => "error:encoded:return-slot",
        ErrorDecl::EncodedViaOutPointer { .. } => "error:encoded:out-pointer",
        other => panic!("unexpected error row {other:?}"),
    });
}

#[cfg(not(miri))]
fn receive_row(prefix: &'static str, receive: Receive) -> &'static str {
    match (prefix, receive) {
        ("param:direct", Receive::ByValue) => "param:direct:by-value",
        ("param:direct", Receive::ByRef) => "param:direct:by-ref",
        ("param:direct", Receive::ByMutRef) => "param:direct:by-mut-ref",
        ("param:encoded", Receive::ByValue) => "param:encoded:by-value",
        ("param:encoded", Receive::ByRef) => "param:encoded:by-ref",
        ("param:encoded", Receive::ByMutRef) => "param:encoded:by-mut-ref",
        ("param:handle", Receive::ByValue) => "param:handle:by-value",
        ("param:handle", Receive::ByRef) => "param:handle:by-ref",
        ("param:handle", Receive::ByMutRef) => "param:handle:by-mut-ref",
        _ => unreachable!(),
    }
}
