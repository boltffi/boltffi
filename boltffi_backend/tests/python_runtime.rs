use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::UNIX_EPOCH,
};

use boltffi_ast::PackageInfo;
use boltffi_backend::{GeneratedOutput, target::python::PythonCExtHost};
use boltffi_binding::{Bindings, Native, lower};

const SOURCE: &str = include_str!("fixtures/source/enums/transparent.rs");
static NEXT_PYTHON_PACKAGE: AtomicU64 = AtomicU64::new(0);

/// Exercises the package through the interpreter. Printing `ok` is the
/// contract: a python-level assertion failure exits non-zero, and anything
/// that silently skips the file would print nothing.
const SCRIPT: &str = r#"
import asyncio, gc, pickle, sys
sys.path.insert(0, sys.argv[1])
import demo

# a conforming direct record inherits every base it is a payload of
assert [c.__name__ for c in demo.Ping.__mro__] == ["Ping", "Envelope", "Reply", "object"], demo.Ping.__mro__
assert [c.__name__ for c in demo.Note.__mro__] == ["Note", "Envelope", "object"], demo.Note.__mro__

ping, note = demo.Ping(sequence=7), demo.Note(body="hi")
assert demo.echo_envelope(ping) == ping
assert demo.echo_envelope(note) == note
values = [ping, note, demo.EnvelopeUnset(), demo.EnvelopeRaw("raw")]
assert demo.echo_envelopes(values) == values
assert demo.Envelope.new() == demo.Ping(sequence=7)
assert demo.Envelope.new(13) == demo.Ping(sequence=13)
assert demo.Envelope.default_step() == 3
assert ping.offset() == 10
assert ping.offset(2) == 9
assert ping.reply_offset() == 18
assert note.offset() == 3

async def check_async():
    assert await demo.Envelope.load() == demo.Ping(sequence=9)
    assert await demo.Envelope.load(21) == demo.Ping(sequence=21)
    assert await ping.async_offset() == 12
    assert await ping.async_offset(4) == 11

asyncio.run(check_async())

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

# both payload lanes are subclassable, and a subclass still carries its
# variant tag: dispatching on the exact type would fall through to the
# untagged payload encoding and decode as the wrong variant
class SubPing(demo.Ping):
    pass

class SubNote(demo.Note):
    pass

sub_ping, sub_note = SubPing(sequence=7), SubNote(body="hi")
assert demo.Envelope._boltffi_wire_value(sub_ping) == demo.Envelope._boltffi_wire_value(ping)
assert demo.Reply._boltffi_wire_value(sub_ping) == demo.Reply._boltffi_wire_value(ping)
assert demo.Envelope._boltffi_wire_value(sub_note) == demo.Envelope._boltffi_wire_value(note)

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
    PythonPackage::run(SOURCE, SCRIPT);
}

#[test]
fn python_package_runs_wrapped_variants_when_members_conflict() {
    let source = format!(
        "{SOURCE}\n#[data(impl)] impl Reply {{
        pub fn offset(&self, #[boltffi::default(13)] step: u32) -> u32 {{
            match self {{ Self::Ping(ping) => ping.sequence + step + 100, Self::Ack => step + 100 }}
        }}
    }}"
    );
    PythonPackage::run(
        &source,
        r#"
import asyncio, sys
sys.path.insert(0, sys.argv[1])
import demo

ping = demo.Ping(sequence=7)
envelope = demo.EnvelopePing(ping)
reply = demo.ReplyPing(ping)
assert not isinstance(ping, demo.Envelope)
assert not isinstance(ping, demo.Reply)
assert envelope.offset() == 10
assert reply.offset() == 120
assert reply.reply_offset() == 18
assert demo.echo_envelope(envelope) == envelope
assert demo.echo_envelopes([envelope, demo.EnvelopeNote(demo.Note("hi")), demo.EnvelopeUnset()]) == [envelope, demo.EnvelopeNote(demo.Note("hi")), demo.EnvelopeUnset()]
assert demo.Envelope.new() == envelope

async def check_async():
    assert await demo.Envelope.load() == demo.EnvelopePing(demo.Ping(sequence=9))
    assert await envelope.async_offset() == 12

asyncio.run(check_async())
print("ok")
"#,
    );
}

struct PythonPackage {
    root: PathBuf,
}

impl PythonPackage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "python-runtime-transparent-{}-{}-{}",
            std::process::id(),
            UNIX_EPOCH.elapsed().expect("system clock").as_nanos(),
            NEXT_PYTHON_PACKAGE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).expect("create unique Python package directory");
        Self { root }
    }

    fn run(source: &str, script_source: &str) {
        if cfg!(windows) {
            return;
        }
        let (Some(python), Some(includes), Some(compiler)) =
            (tool("python3"), includes(), tool("cc"))
        else {
            return;
        };

        let bindings = bindings(source);
        let output = PythonCExtHost::new()
            .native_library("demo")
            .into_target(&bindings)
            .expect("python target")
            .render(&bindings)
            .expect("transparent variants should render");

        let package = Self::new();
        let root = &package.root;
        write_package(&output, root);
        let extension = root.join(native_module_path(&output));
        let package_directory = extension.parent().expect("package directory").to_owned();

        build_rust_library(root, &package_directory.join(cdylib_name()), source);
        build_shared_library(
            &compiler,
            &extension,
            &package_directory.join("_native.so"),
            &[includes, extension_flags()].concat(),
        );

        let script = root.join("check.py");
        fs::write(&script, script_source).expect("write check script");
        let run = Command::new(&python)
            .arg(&script)
            .arg(root)
            .output()
            .expect("run the generated package");
        assert!(
            run.status.success() && String::from_utf8_lossy(&run.stdout).contains("ok"),
            "the generated package failed:\n{}\n{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr),
        );
    }
}

impl Drop for PythonPackage {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.root) {
            eprintln!(
                "removing Python package {} failed: {error}",
                self.root.display()
            );
        }
    }
}

fn bindings(source: &str) -> Bindings<Native> {
    let source = boltffi_scan::scan_file(
        syn::parse_str(source).expect("valid source"),
        PackageInfo::new("demo", None),
    )
    .expect("source should scan");
    lower::<Native>(&source).expect("source should lower")
}

#[test]
fn python_target_wraps_conflicting_transparent_members() {
    [
        ("#[data(impl)] impl Reply { pub fn offset(&self, step: u32) -> u32 { step } }", true),
        ("#[data(impl)] impl Ping { pub fn offset(&self, step: u32) -> u32 { self.sequence + step } }", false),
        ("#[data(impl)] impl Envelope { pub fn sequence(&self) -> u32 { 0 } }", false),
    ]
    .into_iter()
    .for_each(|(conflict, both_enums)| {
        let source = format!("{SOURCE}\n{conflict}");
        let expected = if both_enums {
            source.replace("#[boltffi::transparent]", "")
        } else {
            source.replacen("#[boltffi::transparent]", "", 2)
        };
        let render = |source: &str| {
            let bindings = bindings(source);
            PythonCExtHost::new().native_library("demo").into_target(&bindings)
                .expect("Python target").render(&bindings)
                .expect("unsupported transparency must use ordinary variants")
        };
        assert!(render(&source) == render(&expected), "ordinary variants must be used");
    });
}

#[test]
fn python_uses_ordinary_error_variants() {
    let source = SOURCE.replace(
        "#[data]\npub enum Envelope",
        "#[boltffi::error]\npub enum Envelope",
    );
    let source =
        format!("{source}\n#[export] pub fn try_envelope() -> Result<u32, Envelope> {{ Ok(1) }}");
    let expected = source.replacen("#[boltffi::transparent]", "", 2);
    let render = |source: &str| {
        let bindings = bindings(source);
        PythonCExtHost::new()
            .native_library("demo")
            .into_target(&bindings)
            .expect("Python target")
            .render(&bindings)
            .expect("ordinary error enum")
    };
    assert!(
        render(&source) == render(&expected),
        "ordinary variants must be used"
    );
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
    output.files().iter().for_each(|file| {
        let path = root.join(file.path().as_path());
        fs::create_dir_all(path.parent().expect("generated file parent"))
            .expect("create package directory");
        fs::write(&path, file.contents()).expect("write generated file");
    });
}

fn build_rust_library(root: &Path, destination: &Path, source: &str) {
    let crate_directory = root.join("rust");
    fs::create_dir_all(crate_directory.join("src")).expect("create Rust fixture directory");
    let boltffi_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("boltffi");
    fs::write(
        crate_directory.join("Cargo.toml"),
        format!(
            r#"[package]
name = "demo"
version = "0.0.0"
edition = "2024"

[workspace]

[lib]
crate-type = ["cdylib"]

[dependencies]
boltffi = {{ path = {path:?} }}
"#,
            path = boltffi_path.to_str().expect("UTF-8 fixture path"),
        ),
    )
    .expect("write Rust fixture manifest");
    fs::write(
        crate_directory.join("src/lib.rs"),
        format!("use boltffi::{{data, export}};\nboltffi::scaffolding!();\n{source}"),
    )
    .expect("write Rust fixture source");
    let target_directory = root.join("target");
    let build = Command::new(env!("CARGO"))
        .args(["build", "--quiet", "--manifest-path"])
        .arg(crate_directory.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", &target_directory)
        .env("CARGO_BUILD_BUILD_DIR", target_directory.join("build"))
        .output()
        .expect("build Rust fixture");
    assert!(
        build.status.success(),
        "Rust fixture failed to build:\n{}",
        String::from_utf8_lossy(&build.stderr),
    );
    fs::copy(
        target_directory.join("debug").join(cdylib_name()),
        destination,
    )
    .expect("copy compiled Rust library into the generated package");
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
