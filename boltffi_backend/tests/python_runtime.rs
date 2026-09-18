//! Builds the rendered CPython extension and runs the package, so the shapes
//! that only exist at runtime are asserted against a real interpreter.
//!
//! Transparent variants are the reason this harness exists: a conforming
//! direct record's type is built by `PyType_FromSpecWithBases` during package
//! import, and no amount of rendered-source matching shows whether the
//! resulting type actually inherits its bases, survives the boxer, or
//! deallocates cleanly once the python bases pull in their GC flag.
//!
//! Every external tool is optional: the test returns instead of failing when
//! `cc`, `python3` or `python3-config` is missing, the way the C# and Java
//! smoke tests treat `dotnet` and `javac`.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::UNIX_EPOCH,
};

use boltffi_ast::PackageInfo;
use boltffi_backend::{GeneratedOutput, target::python::PythonCExtHost};
use boltffi_binding::{Bindings, Native, lower};

/// Two enums share `Ping`, so the same payload carries a different tag in
/// each — the reason the tag cannot live on the payload. `Ping` is direct
/// (one fixed-width primitive) and `Note` is encoded, so both payload lanes
/// are covered, alongside a wrapped scalar variant and `Unset`.
const SOURCE: &str = r#"
#[data]
pub struct Ping {
    sequence: u32,
}

#[data]
pub struct Note {
    body: String,
}

#[data]
pub enum Envelope {
    Unset,
    #[boltffi::transparent]
    Ping(Ping),
    #[boltffi::transparent]
    Note(Note),
    Raw(String),
}

#[data]
pub enum Reply {
    #[boltffi::transparent]
    Ping(Ping),
    Ack,
}

#[export]
pub fn echo_envelope(envelope: Envelope) -> Envelope {
    envelope
}
"#;

/// Exercises the package through the interpreter. Printing `ok` is the
/// contract: a python-level assertion failure exits non-zero, and anything
/// that silently skips the file would print nothing.
const SCRIPT: &str = r#"
import gc, pickle, sys
sys.path.insert(0, sys.argv[1])
import demo

# a conforming direct record inherits every base it is a payload of
assert [c.__name__ for c in demo.Ping.__mro__] == ["Ping", "Envelope", "Reply", "object"], demo.Ping.__mro__
assert [c.__name__ for c in demo.Note.__mro__] == ["Note", "Envelope", "object"], demo.Note.__mro__

ping, note = demo.Ping(sequence=7), demo.Note(body="hi")
assert isinstance(ping, demo.Envelope) and isinstance(ping, demo.Reply)
assert isinstance(note, demo.Envelope) and not isinstance(note, demo.Reply)

# the tag lives on the base, so one payload encodes differently per enum
assert demo.Envelope._boltffi_wire_value(ping) != demo.Reply._boltffi_wire_value(ping)
assert demo.Envelope._boltffi_from_wire(demo.Envelope._boltffi_wire_value(ping)) == ping
assert demo.Reply._boltffi_from_wire(demo.Reply._boltffi_wire_value(ping)) == ping
assert demo.Envelope._boltffi_from_wire(demo.Envelope._boltffi_wire_value(note)) == note

# the payload is the match target, and so is the base it inherits
match demo.Envelope._boltffi_from_wire(demo.Envelope._boltffi_wire_value(ping)):
    case demo.Ping(sequence):
        assert sequence == 7
    case _:
        raise AssertionError("transparent payload did not match its own class")
match note:
    case demo.Envelope():
        pass
    case _:
        raise AssertionError("payload did not match its base")

# wrapped and unit variants keep working next to the transparent ones
demo.Envelope._boltffi_wire_value(demo.EnvelopeRaw("x"))
demo.Envelope._boltffi_wire_value(demo.EnvelopeUnset())
try:
    demo.Envelope._boltffi_wire_value(object())
except TypeError:
    pass
else:
    raise AssertionError("a foreign value was accepted")

# the C-native record keeps its record semantics through the added bases
try:
    ping.sequence = 9
except AttributeError:
    pass
else:
    raise AssertionError("a direct record accepted a field write")
assert not hasattr(ping, "__dict__")
assert hash(ping) == hash(demo.Ping(sequence=7))
assert pickle.loads(pickle.dumps(ping)) == ping

# inheriting python bases makes instances GC-tracked; the dealloc must untrack
for _ in range(50_000):
    doomed = demo.Ping(sequence=1)
    del doomed
gc.collect()

print("ok")
"#;

#[test]
fn python_package_runs_transparent_variants_against_the_interpreter() {
    if cfg!(windows) {
        return;
    }
    let (Some(python), Some(includes), Some(compiler)) = (tool("python3"), includes(), tool("cc"))
    else {
        return;
    };

    let bindings = bindings(SOURCE);
    let output = PythonCExtHost::new()
        .native_library("demo")
        .into_target(&bindings)
        .expect("python target")
        .render(&bindings)
        .expect("transparent variants should render");

    let root = scratch("python-runtime-transparent");
    write_package(&output, &root);
    let extension = root.join(native_module_path(&output));
    let package = extension.parent().expect("package directory").to_owned();

    // The package dlopens the cdylib on import and resolves every symbol up
    // front, so the extension needs a library to find. Nothing here calls an
    // export, so empty definitions are enough.
    let stub = root.join("stub.c");
    let native = fs::read_to_string(&extension).expect("generated extension source");
    fs::write(&stub, stub_library(&native)).expect("write stub source");
    build_shared_library(&compiler, &stub, &package.join(cdylib_name()), &[]);
    build_shared_library(
        &compiler,
        &extension,
        &package.join("_native.so"),
        &[includes, extension_flags()].concat(),
    );

    let script = root.join("check.py");
    fs::write(&script, SCRIPT).expect("write check script");
    let run = Command::new(&python)
        .arg(&script)
        .arg(&root)
        .output()
        .expect("run the generated package");
    assert!(
        run.status.success() && String::from_utf8_lossy(&run.stdout).contains("ok"),
        "the generated package failed:\n{}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr),
    );

    fs::remove_dir_all(&root).ok();
}

fn bindings(source: &str) -> Bindings<Native> {
    let source = boltffi_scan::scan_file(
        syn::parse_str(source).expect("valid source"),
        PackageInfo::new("demo", None),
    )
    .expect("source should scan");
    lower::<Native>(&source).expect("source should lower")
}

/// The name of a tool on `PATH`, or `None` when it cannot be run.
fn tool(name: &str) -> Option<String> {
    Command::new(name)
        .arg("--version")
        .output()
        .ok()
        .filter(|version| version.status.success())
        .map(|_| name.to_owned())
}

/// The interpreter's own include flags, which the extension needs to compile.
fn includes() -> Option<Vec<String>> {
    let output = Command::new("python3-config")
        .arg("--includes")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .map(str::to_owned)
            .collect(),
    )
}

fn scratch(prefix: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "{prefix}-{}",
        UNIX_EPOCH.elapsed().expect("system clock").as_nanos()
    ));
    fs::create_dir_all(&root).expect("create scratch directory");
    root
}

/// The rendered extension source, which the host places inside the package
/// module rather than at the tree root.
fn native_module_path(output: &GeneratedOutput) -> PathBuf {
    output
        .files()
        .iter()
        .map(|file| file.path().as_path().to_owned())
        .find(|path| path.file_name().is_some_and(|name| name == "_native.c"))
        .expect("generated _native.c")
}

fn write_package(output: &GeneratedOutput, root: &Path) {
    for file in output.files() {
        let path = root.join(file.path().as_path());
        fs::create_dir_all(path.parent().expect("generated file parent"))
            .expect("create package directory");
        fs::write(&path, file.contents()).expect("write generated file");
    }
}

/// Empty definitions for every symbol the extension resolves out of the
/// cdylib, read off the generated `dlsym` calls so the list cannot drift.
fn stub_library(native: &str) -> String {
    let mut symbols: Vec<&str> = native
        .match_indices("dlsym(boltffi_python_library_handle, \"")
        .filter_map(|(index, marker)| {
            let rest = &native[index + marker.len()..];
            rest.find('"').map(|end| &rest[..end])
        })
        .collect();
    symbols.sort_unstable();
    symbols.dedup();
    assert!(!symbols.is_empty(), "the extension resolved no symbols");
    symbols
        .iter()
        .map(|symbol| format!("void {symbol}(void) {{}}\n"))
        .collect()
}

/// The library filename the generated package looks for, which it derives
/// from `sys.platform`.
fn cdylib_name() -> &'static str {
    match cfg!(target_os = "macos") {
        true => "libdemo.dylib",
        false => "libdemo.so",
    }
}

/// Extra flags the extension itself needs. The interpreter supplies the
/// CPython symbols at load time, which the mach-o linker rejects as
/// undefined unless it is told to look them up dynamically.
fn extension_flags() -> Vec<String> {
    match cfg!(target_os = "macos") {
        true => vec!["-undefined".to_owned(), "dynamic_lookup".to_owned()],
        false => Vec::new(),
    }
}

fn build_shared_library(compiler: &str, source: &Path, output: &Path, flags: &[String]) {
    let build = Command::new(compiler)
        .arg("-shared")
        .arg("-fPIC")
        .args(flags)
        .arg("-o")
        .arg(output)
        .arg(source)
        .output()
        .expect("run the C compiler");
    assert!(
        build.status.success(),
        "compiling {} failed:\n{}",
        source.display(),
        String::from_utf8_lossy(&build.stderr),
    );
}
