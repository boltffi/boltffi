use boltffi_ast::PackageInfo;
use boltffi_binding::{Native, lower};

use boltffi_backend::{
    bridge::{
        c::CBridge,
        jni::{JniBridge, JniBridgeContract},
    },
    core::{BridgeLayer, BridgeOutput, BridgeStack},
};

mod source;

#[path = "jni/associated.rs"]
mod associated;
#[path = "jni/callback.rs"]
mod callback;
#[path = "jni/callback_return.rs"]
mod callback_return;
#[path = "jni/constant.rs"]
mod constant;
#[path = "jni/direct_vector.rs"]
mod direct_vector;
#[path = "jni/native_methods.rs"]
mod native_methods;
#[path = "jni/stream.rs"]
mod stream;

use source::SourceFixture;

fn bindings(source: &str) -> boltffi_binding::Bindings<Native> {
    let file = syn::parse_str(source).expect("valid source fixture");
    let source =
        boltffi_scan::scan_file(file, PackageInfo::new("demo", None)).expect("fixture should scan");
    lower::<Native>(&source).expect("fixture should lower")
}

pub fn bridge(source: &str) -> BridgeOutput<JniBridgeContract> {
    let bindings = bindings(source);
    let stack = BridgeLayer::new(
        CBridge::new("jni/demo.h").expect("C header bridge"),
        JniBridge::new("com.boltffi.demo", "Native", "jni/jni_glue.c").expect("JNI bridge"),
    );
    stack.build(&bindings).expect("JNI bridge stack")
}

pub fn files(source: &str) -> Vec<(String, String)> {
    bridge(source)
        .output()
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

pub fn rendered_fixture(name: &str) -> String {
    rendered_source(SourceFixture::one(name))
}

pub fn rendered_source(fixture: SourceFixture) -> String {
    rendered_files(&files(&fixture.read()))
}

pub fn rendered_fixture_with_support(name: &str) -> String {
    rendered_files_with_support(&files(&fixture(name)))
}

pub fn bridge_fixture(name: &str) -> BridgeOutput<JniBridgeContract> {
    bridge(&fixture(name))
}

fn fixture(name: &str) -> String {
    SourceFixture::one(name).read()
}

fn rendered_files(files: &[(String, String)]) -> String {
    files
        .iter()
        .map(|(path, contents)| format!("===== {path} =====\n{}", normalized_file(path, contents)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn rendered_files_with_support(files: &[(String, String)]) -> String {
    files
        .iter()
        .map(|(path, contents)| format!("===== {path} =====\n{contents}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalized_file(path: &str, contents: &str) -> String {
    if path.ends_with(".h") {
        return normalized_header(contents);
    }
    if path.ends_with(".c") {
        return normalized_source(contents);
    }
    contents.to_owned()
}

fn normalized_header(contents: &str) -> String {
    line_end_after(contents, "void boltffi_clear_last_error(void);").map_or_else(
        || contents.to_owned(),
        |end| {
            format!(
                "<c abi support declarations>\n{}",
                contents[end..]
                    .replace("\n#ifdef __cplusplus\n}\n#endif", "")
                    .trim_start()
            )
        },
    )
}

fn normalized_source(contents: &str) -> String {
    [
        SourceBlock::new(
            "static JavaVM *boltffi_jni_vm",
            "static void boltffi_jni_exit",
            "<jni thread runtime>",
        ),
        SourceBlock::new(
            "static void boltffi_jni_throw_runtime",
            "static void boltffi_jni_throw_illegal_argument",
            "<jni exception helpers>",
        ),
        SourceBlock::new(
            "static void boltffi_jni_throw_status",
            "static void boltffi_jni_throw_status",
            "<jni status helper>",
        ),
        SourceBlock::new(
            "static void boltffi_jni_throw_error_buffer",
            "static void boltffi_jni_throw_error_buffer",
            "<jni error-buffer helper>",
        ),
        SourceBlock::new(
            "static jbyteArray boltffi_jni_buffer_to_byte_array",
            "static FfiBuf_u8 boltffi_jni_byte_array_to_buffer",
            "<jni byte-array helpers>",
        ),
        SourceBlock::new(
            "static bool boltffi_jni_read_record",
            "static jbyteArray boltffi_jni_record_to_byte_array",
            "<jni record helpers>",
        ),
        SourceBlock::new(
            "static jmethodID boltffi_jni_continuation_method",
            "static void boltffi_jni_continuation_callback",
            "<jni continuation helper>",
        ),
    ]
    .into_iter()
    .fold(normalized_includes(contents), |source, block| {
        block.apply(&source)
    })
}

fn normalized_includes(contents: &str) -> String {
    line_end_after(contents, "#include \"demo.h\"").map_or_else(
        || contents.to_owned(),
        |end| format!("<jni source includes>\n{}", contents[end..].trim_start()),
    )
}

struct SourceBlock {
    start: &'static str,
    end: &'static str,
    marker: &'static str,
}

impl SourceBlock {
    fn new(start: &'static str, end: &'static str, marker: &'static str) -> Self {
        Self { start, end, marker }
    }

    fn apply(self, source: &str) -> String {
        let Some(start) = source.find(self.start) else {
            return source.to_owned();
        };
        let Some(end_start) = source[start..].find(self.end).map(|index| start + index) else {
            return source.to_owned();
        };
        let Some(end) = CBlock::from_start(&source[end_start..]).map(CBlock::end) else {
            return source.to_owned();
        };
        format!(
            "{}\n{}\n{}",
            source[..start].trim_end(),
            self.marker,
            source[end_start + end..].trim_start()
        )
    }
}

struct CBlock {
    end: usize,
}

impl CBlock {
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

fn line_end_after(contents: &str, needle: &str) -> Option<usize> {
    let line = contents.find(needle)?;
    contents[line..]
        .find('\n')
        .map(|index| line + index + '\n'.len_utf8())
}
