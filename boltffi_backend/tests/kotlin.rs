use std::{env, fs, process::Command, time::UNIX_EPOCH};

use boltffi_ast::PackageInfo;
use boltffi_backend::target::kotlin::KotlinHost;
use boltffi_binding::{Native, lower};

mod source;

#[path = "kotlin/callback.rs"]
mod callback;
#[path = "kotlin/constant.rs"]
mod constant;
#[path = "kotlin/direct_vector.rs"]
mod direct_vector;
#[path = "kotlin/exports.rs"]
mod exports;
#[path = "kotlin/record.rs"]
mod record;
#[path = "kotlin/stream.rs"]
mod stream;

use source::SourceFixture;

fn bindings(source: &str) -> boltffi_binding::Bindings<Native> {
    let file = syn::parse_str(source).expect("valid source fixture");
    let source =
        boltffi_scan::scan_file(file, PackageInfo::new("demo", None)).expect("fixture should scan");
    lower::<Native>(&source).expect("fixture should lower")
}

pub fn rendered_fixture(name: &str) -> String {
    let host = KotlinHost::new("com.boltffi.demo", "Demo").expect("Kotlin host");
    rendered_source_with_host(SourceFixture::one(name), host)
}

pub fn rendered_source(fixture: SourceFixture) -> String {
    let host = KotlinHost::new("com.boltffi.demo", "Demo").expect("Kotlin host");
    rendered_source_with_host(fixture, host)
}

pub fn rendered_fixture_with_host(name: &str, host: KotlinHost) -> String {
    rendered_source_with_host(SourceFixture::one(name), host)
}

pub fn rendered_source_with_host(fixture: SourceFixture, host: KotlinHost) -> String {
    let kotlin_file = files_with_host(&fixture.read(), host)
        .into_iter()
        .find(|(path, _)| path.ends_with(".kt"))
        .expect("Kotlin target should render a Kotlin source file");
    rendered_files(&[kotlin_file])
}

pub fn rendered_fixture_with_runtime(name: &str) -> String {
    let host = KotlinHost::new("com.boltffi.demo", "Demo").expect("Kotlin host");
    let kotlin_file = files_with_host(&SourceFixture::one(name).read(), host)
        .into_iter()
        .find(|(path, _)| path.ends_with(".kt"))
        .expect("Kotlin target should render a Kotlin source file");
    rendered_files_with_runtime(&[kotlin_file])
}

pub fn files(source: &str) -> Vec<(String, String)> {
    let host = KotlinHost::new("com.boltffi.demo", "Demo").expect("Kotlin host");
    files_with_host(source, host)
}

pub fn files_with_host(source: &str, host: KotlinHost) -> Vec<(String, String)> {
    let bindings = bindings(source);
    let target = host.into_target().expect("Kotlin target");

    target
        .render(&bindings)
        .expect("Kotlin target renders")
        .files()
        .iter()
        .map(|file| {
            (
                file.path().as_path().display().to_string(),
                file.contents().to_owned(),
            )
        })
        .collect()
}

pub fn fixture(name: &str) -> String {
    SourceFixture::one(name).read()
}

/// Compiles a fixture's generated Kotlin with `assertions` and runs its `main`;
/// skipped when `kotlinc` is unavailable.
pub fn run_kotlin_assertions(name: &str, assertions: &str) {
    let compiler = if cfg!(windows) {
        "kotlinc.bat"
    } else {
        "kotlinc"
    };
    if Command::new(compiler).arg("-version").output().is_err() {
        eprintln!("Kotlin compiler is unavailable; skipping {name} runtime assertions");
        return;
    }

    let directory = env::temp_dir().join(format!(
        "boltffi-kotlin-{}-{}-{}",
        name.replace('/', "-"),
        std::process::id(),
        UNIX_EPOCH.elapsed().expect("system clock").as_nanos()
    ));
    fs::create_dir_all(&directory).expect("create Kotlin test directory");
    let source_paths = files(&fixture(name))
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
    let assertions_path = directory.join("Assertions.kt");
    fs::write(&assertions_path, assertions).expect("write Kotlin assertions");
    let jar = directory.join("assertions.jar");
    let compilation = Command::new(compiler)
        .args(&source_paths)
        .arg(assertions_path)
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
    let execution = Command::new("java")
        .arg("-jar")
        .arg(jar)
        .output()
        .expect("run Kotlin assertions");
    assert!(
        execution.status.success(),
        "Kotlin assertions for {name} failed:\n{}\n{}",
        String::from_utf8_lossy(&execution.stdout),
        String::from_utf8_lossy(&execution.stderr)
    );
    fs::remove_dir_all(directory).expect("remove Kotlin test directory");
}

pub fn rendered_files(files: &[(String, String)]) -> String {
    files
        .iter()
        .map(|(path, contents)| {
            let snapshot = KotlinSnapshot::new(contents);
            format!("===== {path} =====\n{}", snapshot.without_runtime())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn rendered_files_with_runtime(files: &[(String, String)]) -> String {
    files
        .iter()
        .map(|(path, contents)| format!("===== {path} =====\n{contents}"))
        .collect::<Vec<_>>()
        .join("\n")
}

struct KotlinSnapshot<'source> {
    source: &'source str,
}

impl<'source> KotlinSnapshot<'source> {
    fn new(source: &'source str) -> Self {
        Self { source }
    }

    fn without_runtime(&self) -> String {
        let source = self.without_shared_runtime();
        Self::without_native_loader(&source)
    }

    fn without_shared_runtime(&self) -> String {
        let Some(runtime) = self.source.find("\nprivate object Utf8Codec") else {
            return self.source.to_owned();
        };
        let Some(native) = self
            .source
            .find("\n@Suppress(\"FunctionName\")\nprivate object Native")
        else {
            return self.source.to_owned();
        };
        format!(
            "{}\n{}",
            self.source[..runtime].trim_end(),
            self.source[native..].trim_start()
        )
    }

    fn without_native_loader(source: &str) -> String {
        let Some(native) = source.find("@Suppress(\"FunctionName\")\nprivate object Native {\n")
        else {
            return source.to_owned();
        };
        let Some(external) = source[native..].find("\n    @JvmStatic external fun") else {
            return Self::without_empty_native_loader(source, native);
        };
        format!(
            "{}@Suppress(\"FunctionName\")\nprivate object Native {{\n{}",
            &source[..native],
            source[native + external..].trim_start_matches('\n')
        )
    }

    fn without_empty_native_loader(source: &str, native: usize) -> String {
        let Some(end) = KotlinObject::from_start(&source[native..]).map(KotlinObject::end) else {
            return source.to_owned();
        };
        format!(
            "{}\n\n{}",
            source[..native].trim_end(),
            source[native + end..].trim_start_matches('\n')
        )
    }
}

struct KotlinObject {
    end: usize,
}

impl KotlinObject {
    fn from_start(source: &str) -> Option<Self> {
        let open = source.find('{')?;
        source[open..]
            .char_indices()
            .scan(0usize, |depth, (index, character)| match character {
                '{' => {
                    *depth += 1;
                    Some(None)
                }
                '}' => {
                    *depth -= 1;
                    Some((*depth == 0).then_some(open + index + character.len_utf8()))
                }
                _ => Some(None),
            })
            .flatten()
            .next()
            .map(|end| Self { end })
    }

    fn end(self) -> usize {
        self.end
    }
}
