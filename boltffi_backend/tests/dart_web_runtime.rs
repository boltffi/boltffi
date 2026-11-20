use std::{
    fs,
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use boltffi_ast::PackageInfo;
use boltffi_backend::target::dart_web::DartWebHost;
use boltffi_binding::{Wasm32, lower};

fn dart() -> Command {
    if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.args(["/C", "dart"]);
        command
    } else {
        Command::new("dart")
    }
}

fn run(command: &mut Command) {
    let output = command.output().expect("tool starts");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn generated_web_bindings_execute() {
    if !dart()
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
        || !Command::new("node")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    {
        eprintln!("Dart web execution test requires Dart and Node.js");
        return;
    }
    let directory = std::env::temp_dir().join(format!(
        "boltffi-dart-web-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&directory).unwrap();
    let ast = boltffi_scan::scan_file(
        syn::parse_str(include_str!("fixtures/dart_web/runtime.rs")).unwrap(),
        PackageInfo::new("probe", None),
    )
    .unwrap();
    let bindings = lower::<Wasm32>(&ast).unwrap();
    let output = DartWebHost::new("probe")
        .unwrap()
        .into_target()
        .render(&bindings)
        .unwrap();
    for file in output.files() {
        fs::write(directory.join(file.path().as_path()), file.contents()).unwrap();
    }
    fs::write(
        directory.join("main.dart"),
        include_str!("fixtures/dart_web/main.dart"),
    )
    .unwrap();
    fs::write(
        directory.join("run.mjs"),
        include_str!("fixtures/dart_web/run.mjs"),
    )
    .unwrap();
    fs::write(
        directory.join("support.mjs"),
        include_str!("../templates/target/dart_web/support.js"),
    )
    .unwrap();
    let runtime = directory.join("runtime");
    fs::create_dir(&runtime).unwrap();
    fs::write(runtime.join("package.json"), "{\"type\":\"module\"}").unwrap();
    let vendor = Path::new(env!("CARGO_MANIFEST_DIR")).join("../boltffi_cli/vendor/runtime-js");
    for entry in fs::read_dir(vendor).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "js") {
            fs::copy(entry.path(), runtime.join(entry.file_name())).unwrap();
        }
    }
    run(dart()
        .current_dir(&directory)
        .args(["compile", "js", "main.dart", "-o", "main.js"]));
    run(Command::new("node").current_dir(&directory).arg("run.mjs"));
    run(dart()
        .current_dir(&directory)
        .args(["compile", "wasm", "main.dart", "-o", "main.wasm"]));
    run(Command::new("node")
        .current_dir(&directory)
        .args(["run.mjs", "wasm"]));
    fs::remove_dir_all(directory).unwrap();
}
