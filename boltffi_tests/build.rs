use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

use boltffi_ast::{PackageInfo, SourceContract};
use boltffi_backend::bridge::c::{
    CBridge, CBridgeContract, Function, ParameterGroup, Record, Type,
};
use boltffi_backend::core::{BridgeBackend, GeneratedOutput};
use boltffi_binding::{
    BINDING_EXPANSION_BUILD_ENV, BINDING_EXPANSION_ROOT_ENV, BINDING_EXPANSION_SOURCE_ENV,
    BINDING_EXPANSION_SURFACE_ENV, BindingMetadataSurface, Bindings, Decl, EnumDecl,
    ExportedCallable, HandlePresence, IncomingParam, Native, RecordDecl, lower,
};
use boltffi_scan::{ScanInput, scan_package};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

fn main() {
    let paths = BuildPaths::from_env();
    println!("cargo:rustc-check-cfg=cfg(boltffi_pending_constants)");
    println!("cargo:rustc-check-cfg=cfg(boltffi_pending_closure_return)");
    ExperimentalExpansion::new(&paths).emit();
    println!("cargo:rerun-if-changed={}", paths.source.display());
    println!("cargo:rerun-if-changed={}", paths.src.display());

    let source = source_contract(&paths);
    let bindings = lower::<Native>(&source).expect("test contract lowers");
    let bridge = CBridge::default_header().expect("C bridge");
    let contract = bridge
        .build_contract(&bindings)
        .expect("C bridge contract builds");
    let output = bridge
        .render_bridge(&bindings, &contract)
        .expect("C bridge renders");
    let signature_map = SignatureMap::new(&bindings, &contract);

    GeneratedContract::new(&paths, signature_map)
        .write(&contract)
        .expect("write generated contract checks");
    CGlue::new(&paths)
        .write(&contract, &output)
        .expect("write C glue harness");
}

struct BuildPaths {
    manifest: PathBuf,
    source: PathBuf,
    src: PathBuf,
    out: PathBuf,
}

struct ExperimentalExpansion<'paths> {
    paths: &'paths BuildPaths,
}

struct GeneratedContract<'paths> {
    paths: &'paths BuildPaths,
    signature_map: SignatureMap,
}

struct CGlue<'paths> {
    paths: &'paths BuildPaths,
}

struct SignatureMap {
    direct_records: BTreeMap<String, String>,
    c_style_enums: BTreeMap<String, String>,
    callback_vtables: BTreeMap<String, String>,
    nullable_function_pointers: BTreeSet<(String, usize)>,
}

enum SignaturePosition {
    Input,
    Output,
}

enum FillBytesHarness {
    EncodedWriteback,
    MutableBytes,
}

impl BuildPaths {
    fn from_env() -> Self {
        let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
        let source = manifest.join("src").join("lib.rs");
        let src = manifest.join("src");
        let out = PathBuf::from(env::var("OUT_DIR").expect("out dir"));
        Self {
            manifest,
            source,
            src,
            out,
        }
    }
}

impl<'paths> ExperimentalExpansion<'paths> {
    fn new(paths: &'paths BuildPaths) -> Self {
        Self { paths }
    }

    fn emit(&self) {
        self.rustc_env(BINDING_EXPANSION_BUILD_ENV, "1");
        self.rustc_env(BINDING_EXPANSION_ROOT_ENV, self.paths.manifest.display());
        self.rustc_env(BINDING_EXPANSION_SOURCE_ENV, self.paths.source.display());
        self.rustc_env(
            BINDING_EXPANSION_SURFACE_ENV,
            BindingMetadataSurface::Native.as_str(),
        );
    }

    fn rustc_env(&self, key: &str, value: impl std::fmt::Display) {
        println!("cargo:rustc-env={key}={value}");
    }
}

impl<'paths> GeneratedContract<'paths> {
    fn new(paths: &'paths BuildPaths, signature_map: SignatureMap) -> Self {
        Self {
            paths,
            signature_map,
        }
    }

    fn write(&self, contract: &CBridgeContract) -> std::io::Result<()> {
        fs::write(
            self.paths.out.join("signature_asserts.rs"),
            self.signature_asserts(contract).to_string(),
        )?;
        fs::write(
            self.paths.out.join("layout_asserts.rs"),
            self.layout_asserts(contract).to_string(),
        )?;
        fs::write(
            self.paths.out.join("asserted_symbols.rs"),
            self.asserted_symbols(contract).to_string(),
        )
    }

    fn signature_asserts(&self, contract: &CBridgeContract) -> TokenStream {
        let assertions = contract
            .published_functions()
            .map(|function| self.signature_assert(function))
            .collect::<Vec<_>>();
        quote! {
            #(#assertions)*
        }
    }

    fn asserted_symbols(&self, contract: &CBridgeContract) -> TokenStream {
        let symbols = contract
            .published_functions()
            .map(Function::name)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        quote! {
            pub const ASSERTED_SYMBOLS: &[&str] = &[#(#symbols),*];
        }
    }

    fn layout_asserts(&self, contract: &CBridgeContract) -> TokenStream {
        let records = contract
            .direct_records()
            .iter()
            .map(|record| self.record_layout_assert(record))
            .collect::<Vec<_>>();
        let enums = contract
            .enums()
            .iter()
            .filter(|enumeration| self.signature_map.c_style_enums.contains_key(enumeration.name()))
            .map(|enumeration| {
                let rust_ty = self.signature_map.rust_ty(
                    &self.signature_map.c_style_enums,
                    enumeration.name(),
                );
                let repr = self.signature_map.layout_ty(enumeration.repr());
                quote! {
                    const _: () = {
                        assert!(::core::mem::size_of::<#rust_ty>() == ::core::mem::size_of::<#repr>());
                        assert!(::core::mem::align_of::<#rust_ty>() == ::core::mem::align_of::<#repr>());
                    };
                }
            })
            .collect::<Vec<_>>();
        quote! {
            #(#records)*
            #(#enums)*
        }
    }

    fn record_layout_assert(&self, record: &Record) -> TokenStream {
        let rust_ty = self
            .signature_map
            .rust_ty(&self.signature_map.direct_records, record.name());
        let mirror = format_ident!("__BoltffiLayout{}", record.name());
        let fields = record
            .fields()
            .iter()
            .map(|field| {
                let name = format_ident!("{}", field.name());
                let ty = self.signature_map.layout_ty(field.ty());
                quote! { #name: #ty }
            })
            .collect::<Vec<_>>();
        let offsets = record
            .fields()
            .iter()
            .map(|field| {
                let name = format_ident!("{}", field.name());
                quote! {
                    assert!(::core::mem::offset_of!(#rust_ty, #name) == ::core::mem::offset_of!(#mirror, #name));
                }
            })
            .collect::<Vec<_>>();
        quote! {
            #[repr(C)]
            struct #mirror {
                #(#fields,)*
            }

            const _: () = {
                assert!(::core::mem::size_of::<#rust_ty>() == ::core::mem::size_of::<#mirror>());
                assert!(::core::mem::align_of::<#rust_ty>() == ::core::mem::align_of::<#mirror>());
                #(#offsets)*
            };
        }
    }

    fn signature_assert(&self, function: &Function) -> TokenStream {
        let symbol = format_ident!("{}", function.name());
        let params = function
            .params()
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                self.signature_map
                    .parameter_ty(function.name(), index, parameter.ty())
            })
            .collect::<Vec<_>>();
        let returns = self
            .signature_map
            .ty(function.returns(), SignaturePosition::Output);
        quote! {
            const _: unsafe extern "C" fn(#(#params),*) -> #returns =
                crate::__boltffi_ir_expansion::#symbol;
        }
    }
}

impl<'paths> CGlue<'paths> {
    fn new(paths: &'paths BuildPaths) -> Self {
        Self { paths }
    }

    fn write(&self, contract: &CBridgeContract, output: &GeneratedOutput) -> io::Result<()> {
        self.write_header(output)?;
        let source = self.harness_source(contract);
        let source_path = self.paths.out.join("glue_harness.c");
        let harness_symbols = self.harness_symbols(contract, &source);
        fs::write(&source_path, source)?;
        fs::write(
            self.paths.out.join("c_harness_symbols.rs"),
            harness_symbols.to_string(),
        )?;
        cc::Build::new()
            .file(source_path)
            .include(&self.paths.out)
            .warnings(true)
            .extra_warnings(true)
            .warnings_into_errors(true)
            .std("c11")
            .compile("boltffi_tests_glue_harness");
        Ok(())
    }

    fn write_header(&self, output: &GeneratedOutput) -> io::Result<()> {
        output.files().iter().try_for_each(|file| {
            let path = self.paths.out.join(file.path().as_path());
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, file.contents())
        })
    }

    fn harness_source(&self, contract: &CBridgeContract) -> String {
        let fill_bytes = FillBytesHarness::from_contract(contract);
        let helpers = self.helpers_harness();
        let bytes = self.bytes_harness();
        let primitives = self.primitives_harness();
        let record_methods = self.record_methods_harness();
        let direct_records = self.direct_records_harness();
        let enums = self.enums_harness();
        let data_enums = self.data_enums_harness();
        let strings = self.strings_harness();
        let encoded_records = self.encoded_records_harness();
        let vectors = self.vectors_harness();
        let collections = self.collections_harness();
        let options = self.options_harness();
        let customs = self.customs_harness();
        let closures = self.closures_harness();
        let callbacks = self.callbacks_harness();
        let classes = self.classes_harness();
        let streams = self.streams_harness();
        let results = self.results_harness();
        let asynchronous = self.asynchronous_harness();
        format!(
            r#"#include "boltffi.h"
#include <stddef.h>
#include <stdint.h>
#include <string.h>

{helpers}
{fill_bytes}
{bytes}
{primitives}
{record_methods}
{direct_records}
{enums}
{data_enums}
{strings}
{encoded_records}
{vectors}
{collections}
{options}
{customs}
{closures}
{callbacks}
{classes}
{streams}
{results}
{asynchronous}

int boltffi_tests_run_glue_harness(void) {{
    int fill_bytes = boltffi_tests_check_fill_bytes();
    if (fill_bytes != 0) {{
        return fill_bytes;
    }}
    int bytes = boltffi_tests_check_bytes();
    if (bytes != 0) {{
        return bytes;
    }}
    int primitives = boltffi_tests_check_primitives();
    if (primitives != 0) {{
        return primitives;
    }}
    int record_methods = boltffi_tests_check_record_methods();
    if (record_methods != 0) {{
        return record_methods;
    }}
    int direct_records = boltffi_tests_check_direct_records();
    if (direct_records != 0) {{
        return direct_records;
    }}
    int enums = boltffi_tests_check_enums();
    if (enums != 0) {{
        return enums;
    }}
    int data_enums = boltffi_tests_check_data_enums();
    if (data_enums != 0) {{
        return data_enums;
    }}
    int strings = boltffi_tests_check_strings();
    if (strings != 0) {{
        return strings;
    }}
    int encoded_records = boltffi_tests_check_encoded_records();
    if (encoded_records != 0) {{
        return encoded_records;
    }}
    int vectors = boltffi_tests_check_vectors();
    if (vectors != 0) {{
        return vectors;
    }}
    int collections = boltffi_tests_check_collections();
    if (collections != 0) {{
        return collections;
    }}
    int options = boltffi_tests_check_options();
    if (options != 0) {{
        return options;
    }}
    int customs = boltffi_tests_check_customs();
    if (customs != 0) {{
        return customs;
    }}
    int closures = boltffi_tests_check_closures();
    if (closures != 0) {{
        return closures;
    }}
    int callbacks = boltffi_tests_check_callbacks();
    if (callbacks != 0) {{
        return callbacks;
    }}
    int classes = boltffi_tests_check_classes();
    if (classes != 0) {{
        return classes;
    }}
    int streams = boltffi_tests_check_streams();
    if (streams != 0) {{
        return streams;
    }}
    int results = boltffi_tests_check_results();
    if (results != 0) {{
        return results;
    }}
    int asynchronous = boltffi_tests_check_asynchronous();
    if (asynchronous != 0) {{
        return asynchronous;
    }}
    return 0;
}}
"#
        )
    }

    fn harness_symbols(&self, contract: &CBridgeContract, source: &str) -> TokenStream {
        let source_identifiers = self.source_identifiers(source);
        let symbols = contract
            .published_functions()
            .map(Function::name)
            .filter(|symbol| source_identifiers.contains(*symbol))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        quote! {
            pub const C_HARNESS_SYMBOLS: &[&str] = &[#(#symbols),*];
        }
    }

    fn source_identifiers<'source>(&self, source: &'source str) -> BTreeSet<&'source str> {
        source
            .split(|character: char| !(character == '_' || character.is_ascii_alphanumeric()))
            .filter(|token| !token.is_empty())
            .collect()
    }

    fn helpers_harness(&self) -> &'static str {
        r#"static int boltffi_tests_check_buf(FfiBuf_u8 buf, const uint8_t *expected, uintptr_t expected_len, int code) {
    if (buf.ptr == 0) {
        return code;
    }
    if (buf.len != expected_len) {
        boltffi_free_buf(buf);
        return code + 1;
    }
    uintptr_t index = 0;
    while (index < expected_len) {
        if (buf.ptr[index] != expected[index]) {
            boltffi_free_buf(buf);
            return code + 2;
        }
        index += 1;
    }
    boltffi_free_buf(buf);
    return 0;
}

static int boltffi_tests_check_empty_buf(FfiBuf_u8 buf, int code) {
    if (buf.len != 0) {
        boltffi_free_buf(buf);
        return code;
    }
    return 0;
}

static int boltffi_tests_check_i64_buf(FfiBuf_u8 buf, int64_t expected, int code) {
    if (buf.len != sizeof(int64_t)) {
        boltffi_free_buf(buf);
        return code;
    }
    int64_t value = 0;
    memcpy(&value, buf.ptr, sizeof(value));
    boltffi_free_buf(buf);
    if (value != expected) {
        return code + 1;
    }
    return 0;
}

static int boltffi_tests_check_i32_buf(FfiBuf_u8 buf, int32_t expected, int code) {
    if (buf.len != sizeof(int32_t)) {
        boltffi_free_buf(buf);
        return code;
    }
    int32_t value = 0;
    memcpy(&value, buf.ptr, sizeof(value));
    boltffi_free_buf(buf);
    if (value != expected) {
        return code + 1;
    }
    return 0;
}

static void boltffi_tests_async_noop(uint64_t data, int8_t status) {
    (void)data;
    (void)status;
}

static FfiBuf_u8 boltffi_tests_point_buf(double x, double y) {
    uint8_t bytes[sizeof(double) * 2];
    memcpy(bytes, &x, sizeof(x));
    memcpy(bytes + sizeof(x), &y, sizeof(y));
    return boltffi_buf_from_bytes(bytes, sizeof(bytes));
}

static int boltffi_tests_check_point_buf(FfiBuf_u8 buf, double expected_x, double expected_y, int code) {
    if (buf.len != sizeof(double) * 2) {
        boltffi_free_buf(buf);
        return code;
    }
    double x = 0.0;
    double y = 0.0;
    memcpy(&x, buf.ptr, sizeof(x));
    memcpy(&y, buf.ptr + sizeof(x), sizeof(y));
    boltffi_free_buf(buf);
    if (x != expected_x || y != expected_y) {
        return code + 1;
    }
    return 0;
}

static int boltffi_tests_check_point_vec_buf(FfiBuf_u8 buf, double expected_x, double expected_y, int code) {
    uintptr_t offset = 0;
    if (buf.len == sizeof(uint32_t) + sizeof(double) * 2) {
        uint32_t count = 0;
        memcpy(&count, buf.ptr, sizeof(count));
        if (count != 1) {
            boltffi_free_buf(buf);
            return code;
        }
        offset = sizeof(uint32_t);
    } else if (buf.len != sizeof(double) * 2) {
        boltffi_free_buf(buf);
        return code + 1;
    }
    double x = 0.0;
    double y = 0.0;
    memcpy(&x, buf.ptr + offset, sizeof(x));
    memcpy(&y, buf.ptr + offset + sizeof(x), sizeof(y));
    boltffi_free_buf(buf);
    if (x != expected_x || y != expected_y) {
        return code + 2;
    }
    return 0;
}

static FfiBuf_u8 boltffi_tests_string_buf(const char *value, uintptr_t len) {
    uint8_t bytes[64] = {0};
    uint32_t wire_len = (uint32_t)len;
    memcpy(bytes, &wire_len, sizeof(wire_len));
    memcpy(bytes + sizeof(wire_len), value, len);
    return boltffi_buf_from_bytes(bytes, sizeof(wire_len) + len);
}

static FfiBuf_u8 boltffi_tests_option_i32_buf(int present, int32_t value) {
    uint8_t bytes[1 + sizeof(int32_t)] = {0};
    if (present) {
        bytes[0] = 1;
        memcpy(bytes + 1, &value, sizeof(value));
        return boltffi_buf_from_bytes(bytes, sizeof(bytes));
    }
    return boltffi_buf_from_bytes(bytes, 1);
}

static FfiBuf_u8 boltffi_tests_option_i64_buf(int present, int64_t value) {
    uint8_t bytes[1 + sizeof(int64_t)] = {0};
    if (present) {
        bytes[0] = 1;
        memcpy(bytes + 1, &value, sizeof(value));
        return boltffi_buf_from_bytes(bytes, sizeof(bytes));
    }
    return boltffi_buf_from_bytes(bytes, 1);
}

static FfiBuf_u8 boltffi_tests_i32_vec2_buf(int32_t first, int32_t second) {
    uint8_t bytes[sizeof(uint32_t) + sizeof(int32_t) * 2] = {0};
    uint32_t len = 2;
    memcpy(bytes, &len, sizeof(len));
    memcpy(bytes + sizeof(len), &first, sizeof(first));
    memcpy(bytes + sizeof(len) + sizeof(first), &second, sizeof(second));
    return boltffi_buf_from_bytes(bytes, sizeof(bytes));
}

static int boltffi_tests_check_i32_vec_buf(FfiBuf_u8 buf, const int32_t *expected, uintptr_t count, int code) {
    uintptr_t offset = 0;
    if (buf.len == sizeof(uint32_t) + sizeof(int32_t) * count) {
        uint32_t wire_count = 0;
        memcpy(&wire_count, buf.ptr, sizeof(wire_count));
        if (wire_count != count) {
            boltffi_free_buf(buf);
            return code;
        }
        offset = sizeof(uint32_t);
    } else if (buf.len != sizeof(int32_t) * count) {
        boltffi_free_buf(buf);
        return code + 1;
    }
    uintptr_t index = 0;
    while (index < count) {
        int32_t value = 0;
        memcpy(&value, buf.ptr + offset + sizeof(int32_t) * index, sizeof(value));
        if (value != expected[index]) {
            boltffi_free_buf(buf);
            return code + 2;
        }
        index += 1;
    }
    boltffi_free_buf(buf);
    return 0;
}
"#
    }

    fn bytes_harness(&self) -> &'static str {
        r#"static int boltffi_tests_check_bytes(void) {
    const uint8_t data[8] = {4, 0, 0, 0, 1, 2, 3, 4};
    if (boltffi_function_boltffi_tests_bytes_byte_sum(data, 8) != 10) {
        return 51;
    }
    if (boltffi_function_boltffi_tests_bytes_borrowed_byte_sum(data, 8) != 10) {
        return 52;
    }
    int echoed = boltffi_tests_check_buf(
        boltffi_function_boltffi_tests_bytes_echo_bytes(data, 8),
        data,
        8,
        53
    );
    if (echoed != 0) {
        return echoed;
    }
    return 0;
}
"#
    }

    fn primitives_harness(&self) -> &'static str {
        r#"static int boltffi_tests_check_primitives(void) {
    if (boltffi_function_boltffi_tests_primitives_add_i8(100, 20) != 120) {
        return 101;
    }
    if (boltffi_function_boltffi_tests_primitives_add_u8(200, 30) != 230) {
        return 102;
    }
    if (boltffi_function_boltffi_tests_primitives_add_i16(2000, 3000) != 5000) {
        return 103;
    }
    if (boltffi_function_boltffi_tests_primitives_add_u16(40000, 2000) != 42000) {
        return 104;
    }
    if (boltffi_function_boltffi_tests_primitives_add_i32(100000, 230000) != 330000) {
        return 105;
    }
    if (boltffi_function_boltffi_tests_primitives_add_u32(3000000000u, 42u) != 3000000042u) {
        return 106;
    }
    if (boltffi_function_boltffi_tests_primitives_add_i64(4000000000ll, 50ll) != 4000000050ll) {
        return 107;
    }
    if (boltffi_function_boltffi_tests_primitives_add_u64(9000000000ull, 12ull) != 9000000012ull) {
        return 108;
    }
    if (boltffi_function_boltffi_tests_primitives_add_isize((intptr_t)50, (intptr_t)7) != (intptr_t)57) {
        return 109;
    }
    if (boltffi_function_boltffi_tests_primitives_add_usize((uintptr_t)70, (uintptr_t)8) != (uintptr_t)78) {
        return 110;
    }
    if (boltffi_function_boltffi_tests_primitives_mix_floats(2.5f, 4.0) != 10.5) {
        return 111;
    }
    if (boltffi_function_boltffi_tests_primitives_toggle(true) != false) {
        return 112;
    }
    if (boltffi_function_boltffi_tests_primitives_read_ref(17) != 17) {
        return 113;
    }
    FfiStatus bump = boltffi_function_boltffi_tests_primitives_bump_in_place(19);
    if (bump.code != FFI_STATUS_OK.code) {
        return 114;
    }
    FfiStatus noop = boltffi_function_boltffi_tests_primitives_noop();
    if (noop.code != FFI_STATUS_OK.code) {
        return 115;
    }
    return 0;
}
"#
    }

    fn direct_records_harness(&self) -> &'static str {
        r#"_Static_assert(sizeof(___FixtureRect) == sizeof(double) * 4, "fixture rect size");
_Static_assert(_Alignof(___FixtureRect) == _Alignof(double), "fixture rect alignment");

static int boltffi_tests_check_direct_records(void) {
    ___FixtureRect rect = boltffi_function_boltffi_tests_records_direct_make_rect(1.0, 2.0, 3.0, 4.0);
    if (rect.x != 1.0 || rect.y != 2.0 || rect.width != 3.0 || rect.height != 4.0) {
        return 201;
    }
    if (boltffi_function_boltffi_tests_records_direct_rect_area(rect) != 12.0) {
        return 202;
    }
    if (boltffi_function_boltffi_tests_records_direct_rect_x(&rect) != 1.0) {
        return 203;
    }
    FfiStatus status = boltffi_function_boltffi_tests_records_direct_scale_rect_in_place(&rect, 2.0);
    if (status.code != FFI_STATUS_OK.code) {
        return 204;
    }
    if (rect.width != 6.0 || rect.height != 8.0) {
        return 205;
    }
    return 0;
}
"#
    }

    fn record_methods_harness(&self) -> &'static str {
        r#"static int boltffi_tests_check_record_methods(void) {
    FfiBuf_u8 origin = boltffi_init_record_boltffi_tests_fixture_point_origin();
    double origin_distance = boltffi_method_record_boltffi_tests_fixture_point_distance_to_origin(origin.ptr, origin.len);
    if (origin_distance != 0.0) {
        boltffi_free_buf(origin);
        return 181;
    }
    boltffi_free_buf(origin);
    FfiBuf_u8 point = boltffi_init_record_boltffi_tests_fixture_point_new_at(3.0, 4.0);
    double distance = boltffi_method_record_boltffi_tests_fixture_point_distance_to_origin(point.ptr, point.len);
    if (distance != 5.0) {
        boltffi_free_buf(point);
        return 182;
    }
    FfiBuf_u8 scaled = {0};
    FfiStatus status = boltffi_method_record_boltffi_tests_fixture_point_scale(point.ptr, point.len, &scaled, 2.0);
    boltffi_free_buf(point);
    if (status.code != FFI_STATUS_OK.code) {
        return 183;
    }
    double scaled_distance = boltffi_method_record_boltffi_tests_fixture_point_distance_to_origin(scaled.ptr, scaled.len);
    boltffi_free_buf(scaled);
    if (scaled_distance != 10.0) {
        return 184;
    }
    FfiBuf_u8 first = boltffi_tests_point_buf(0.0, 2.0);
    FfiBuf_u8 second = boltffi_tests_point_buf(4.0, 6.0);
    int midpoint = boltffi_tests_check_point_buf(
        boltffi_init_record_boltffi_tests_fixture_point_midpoint_to(first.ptr, first.len, second.ptr, second.len),
        2.0,
        4.0,
        185
    );
    boltffi_free_buf(first);
    boltffi_free_buf(second);
    if (midpoint != 0) {
        return midpoint;
    }
    FfiBuf_u8 owned = boltffi_tests_string_buf("name", 4);
    FfiBuf_u8 owned_config = boltffi_init_record_boltffi_tests_records_encoded_fixture_string_config_from_owned_name(owned.ptr, owned.len);
    boltffi_free_buf(owned);
    if (owned_config.len == 0) {
        return 188;
    }
    boltffi_free_buf(owned_config);
    FfiBuf_u8 borrowed = boltffi_tests_string_buf("name", 4);
    FfiBuf_u8 borrowed_config = boltffi_init_record_boltffi_tests_records_encoded_fixture_string_config_from_borrowed_name(borrowed.ptr, borrowed.len);
    boltffi_free_buf(borrowed);
    if (borrowed_config.len == 0) {
        return 189;
    }
    boltffi_free_buf(borrowed_config);
    FfiBuf_u8 string_ref = boltffi_tests_string_buf("name", 4);
    FfiBuf_u8 string_ref_config = boltffi_init_record_boltffi_tests_records_encoded_fixture_string_config_from_string_ref_name(string_ref.ptr, string_ref.len);
    boltffi_free_buf(string_ref);
    if (string_ref_config.len == 0) {
        return 190;
    }
    boltffi_free_buf(string_ref_config);
    return 0;
}
"#
    }

    fn enums_harness(&self) -> &'static str {
        r#"static int boltffi_tests_check_enums(void) {
    if (boltffi_function_boltffi_tests_enums_next_status(FIXTURE_STATUS_PENDING) != FIXTURE_STATUS_ACTIVE) {
        return 301;
    }
    if (boltffi_function_boltffi_tests_enums_next_status(FIXTURE_STATUS_ACTIVE) != FIXTURE_STATUS_COMPLETED) {
        return 302;
    }
    if (boltffi_function_boltffi_tests_enums_next_status(FIXTURE_STATUS_COMPLETED) != FIXTURE_STATUS_FAILED) {
        return 303;
    }
    if (boltffi_function_boltffi_tests_enums_next_status(FIXTURE_STATUS_FAILED) != FIXTURE_STATUS_PENDING) {
        return 304;
    }
    return 0;
}
"#
    }

    fn data_enums_harness(&self) -> &'static str {
        r#"static int boltffi_tests_check_data_enums(void) {
    const uint8_t rect[20] = {
        2, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 8, 64,
        0, 0, 0, 0, 0, 0, 16, 64
    };
    if (boltffi_function_boltffi_tests_enums_area(rect, 20) != 12.0) {
        return 321;
    }
    const uint8_t line[12] = {
        1, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 20, 64
    };
    FfiBuf_u8 widened = boltffi_function_boltffi_tests_enums_widen(line, 12, 2.0);
    double area = boltffi_function_boltffi_tests_enums_area(widened.ptr, widened.len);
    boltffi_free_buf(widened);
    if (area != 7.0) {
        return 322;
    }
    return 0;
}
"#
    }

    fn strings_harness(&self) -> &'static str {
        r#"static int boltffi_tests_check_strings(void) {
    const uint8_t hello[9] = {5, 0, 0, 0, 'h', 'e', 'l', 'l', 'o'};
    if (boltffi_function_boltffi_tests_strings_borrowed_len(hello, 9) != 5) {
        return 401;
    }
    const uint8_t shouted_expected[9] = {5, 0, 0, 0, 'H', 'E', 'L', 'L', 'O'};
    int shouted = boltffi_tests_check_buf(
        boltffi_function_boltffi_tests_strings_shout(hello, 9),
        shouted_expected,
        9,
        402
    );
    if (shouted != 0) {
        return shouted;
    }
    uint8_t base[8] = {4, 0, 0, 0, 'b', 'a', 's', 'e'};
    const uint8_t suffix[6] = {2, 0, 0, 0, ':', 'x'};
    const uint8_t rewritten_expected[10] = {6, 0, 0, 0, 'b', 'a', 's', 'e', ':', 'x'};
    FfiBuf_u8 rewritten = {0};
    FfiStatus status = boltffi_function_boltffi_tests_strings_rewrite(base, 8, &rewritten, suffix, 6);
    if (status.code != FFI_STATUS_OK.code) {
        return 405;
    }
    return boltffi_tests_check_buf(rewritten, rewritten_expected, 10, 406);
}
"#
    }

    fn encoded_records_harness(&self) -> &'static str {
        r#"static int boltffi_tests_check_encoded_records(void) {
    uint8_t record[27] = {
        3, 0, 0, 0, 'o', 'l', 'd',
        0, 0, 0, 0, 0, 0, 0, 64,
        0, 0, 0, 0, 0, 0, 8, 64,
        2, 0, 0, 0
    };
    if (boltffi_function_boltffi_tests_records_encoded_peek_label(record, 27) != 3) {
        return 451;
    }
    const uint8_t description_expected[21] = {17, 0, 0, 0, 'o', 'l', 'd', ':', '2', ':', '3', ':', 'C', 'o', 'm', 'p', 'l', 'e', 't', 'e', 'd'};
    int description = boltffi_tests_check_buf(
        boltffi_function_boltffi_tests_records_encoded_describe_message(record, 27),
        description_expected,
        21,
        452
    );
    if (description != 0) {
        return description;
    }
    const uint8_t label[7] = {3, 0, 0, 0, 'n', 'e', 'w'};
    FfiBuf_u8 relabeled = {0};
    FfiStatus status = boltffi_function_boltffi_tests_records_encoded_relabel(record, 27, &relabeled, label, 7);
    if (status.code != FFI_STATUS_OK.code) {
        return 455;
    }
    const uint8_t relabeled_expected[21] = {17, 0, 0, 0, 'n', 'e', 'w', ':', '2', ':', '3', ':', 'C', 'o', 'm', 'p', 'l', 'e', 't', 'e', 'd'};
    int relabeled_description = boltffi_tests_check_buf(
        boltffi_function_boltffi_tests_records_encoded_describe_message(relabeled.ptr, relabeled.len),
        relabeled_expected,
        21,
        456
    );
    boltffi_free_buf(relabeled);
    if (relabeled_description != 0) {
        return relabeled_description;
    }
    const uint8_t made[8] = {4, 0, 0, 0, 'm', 'a', 'd', 'e'};
    FfiBuf_u8 message = boltffi_function_boltffi_tests_records_encoded_make_message(made, 8);
    if (message.len < 8 || message.ptr[0] != 4 || message.ptr[4] != 'm' || message.ptr[7] != 'e') {
        boltffi_free_buf(message);
        return 459;
    }
    boltffi_free_buf(message);
    return 0;
}
"#
    }

    fn vectors_harness(&self) -> &'static str {
        r#"static int boltffi_tests_check_vectors(void) {
    uint32_t values[4] = {1, 2, 3, 4};
    if (boltffi_function_boltffi_tests_vectors_sum_u32(values, 4) != 10) {
        return 501;
    }
    double floats[3] = {2.0, 4.0, 6.0};
    FfiBuf_u8 halved = boltffi_function_boltffi_tests_vectors_halve_f64(floats, 3);
    if (halved.len != sizeof(double) * 3) {
        boltffi_free_buf(halved);
        return 502;
    }
    if (halved.align != _Alignof(double)) {
        boltffi_free_buf(halved);
        return 503;
    }
    double *halved_values = (double *)halved.ptr;
    if (halved_values[0] != 1.0 || halved_values[1] != 2.0 || halved_values[2] != 3.0) {
        boltffi_free_buf(halved);
        return 504;
    }
    boltffi_free_buf(halved);
    ___FixtureRect rects[2] = {
        {1.0, 1.0, 2.0, 2.0},
        {-1.0, 0.0, 1.0, 5.0},
    };
    ___FixtureRect bounds = boltffi_function_boltffi_tests_vectors_bounding_box((const uint8_t *)rects, sizeof(rects));
    if (bounds.x != -1.0 || bounds.y != 0.0 || bounds.width != 4.0 || bounds.height != 5.0) {
        return 505;
    }
    const uint8_t labels[14] = {2, 0, 0, 0, 1, 0, 0, 0, 'a', 1, 0, 0, 0, 'b'};
    const uint8_t joined_expected[7] = {3, 0, 0, 0, 'a', '|', 'b'};
    int joined = boltffi_tests_check_buf(
        boltffi_function_boltffi_tests_vectors_join_labels(labels, 14),
        joined_expected,
        7,
        506
    );
    if (joined != 0) {
        return joined;
    }
    const uint8_t text[9] = {5, 0, 0, 0, 'a', '|', 'b', '|', 'c'};
    FfiBuf_u8 split = boltffi_function_boltffi_tests_vectors_split_labels(text, 9);
    const uint8_t roundtrip_expected[9] = {5, 0, 0, 0, 'a', '|', 'b', '|', 'c'};
    int roundtrip = boltffi_tests_check_buf(
        boltffi_function_boltffi_tests_vectors_join_labels(split.ptr, split.len),
        roundtrip_expected,
        9,
        509
    );
    boltffi_free_buf(split);
    if (roundtrip != 0) {
        return roundtrip;
    }
    FfiBuf_u8 statuses = boltffi_function_boltffi_tests_vectors_statuses(4);
    uintptr_t status_offset = 0;
    if (statuses.len == sizeof(uint32_t) + sizeof(int32_t) * 4) {
        uint32_t count = 0;
        memcpy(&count, statuses.ptr, sizeof(count));
        if (count != 4) {
            boltffi_free_buf(statuses);
            return 512;
        }
        status_offset = sizeof(uint32_t);
    } else if (statuses.len != sizeof(int32_t) * 4) {
        boltffi_free_buf(statuses);
        return 513;
    }
    int32_t status_values[4] = {0, 0, 0, 0};
    memcpy(status_values, statuses.ptr + status_offset, sizeof(status_values));
    if (status_values[0] != FIXTURE_STATUS_PENDING || status_values[1] != FIXTURE_STATUS_ACTIVE || status_values[2] != FIXTURE_STATUS_COMPLETED || status_values[3] != FIXTURE_STATUS_FAILED) {
        boltffi_free_buf(statuses);
        return 514;
    }
    boltffi_free_buf(statuses);
    return 0;
}
"#
    }

    fn collections_harness(&self) -> &'static str {
        r#"static int boltffi_tests_check_collections(void) {
    const uint8_t labels[26] = {
        2, 0, 0, 0,
        3, 0, 0, 0, 'o', 'n', 'e', 1, 0, 0, 0,
        3, 0, 0, 0, 't', 'w', 'o', 2, 0, 0, 0
    };
    if (boltffi_function_boltffi_tests_collections_tally(labels, 26) != 3) {
        return 551;
    }
    FfiBuf_u8 inverted = boltffi_function_boltffi_tests_collections_invert(labels, 26);
    int32_t inverted_total = boltffi_function_boltffi_tests_collections_tally(inverted.ptr, inverted.len);
    boltffi_free_buf(inverted);
    if (inverted_total != -3) {
        return 552;
    }
    const uint8_t label[9] = {5, 0, 0, 0, 'l', 'a', 'b', 'e', 'l'};
    const uint8_t pair_expected[15] = {14, 0, 0, 0, 7, 0, 0, 0, 'l', 'a', 'b', 'e', 'l', ':', '7'};
    int pair = boltffi_tests_check_buf(
        boltffi_function_boltffi_tests_collections_pair_up(7, label, 9),
        pair_expected,
        15,
        553
    );
    if (pair != 0) {
        return pair;
    }
    const uint8_t deep[20] = {
        3, 0, 0, 0,
        1, 2, 0, 0, 0, 'a', 'b',
        0,
        1, 3, 0, 0, 0, 'c', 'd', 'e'
    };
    if (boltffi_function_boltffi_tests_collections_deep(deep, 20) != 5) {
        return 556;
    }
    return 0;
}
"#
    }

    fn options_harness(&self) -> &'static str {
        r#"static int boltffi_tests_check_options(void) {
    const uint8_t some_expected[5] = {1, 8, 0, 0, 0};
    int some = boltffi_tests_check_buf(
        boltffi_function_boltffi_tests_options_simple_maybe_double(4),
        some_expected,
        5,
        601
    );
    if (some != 0) {
        return some;
    }
    const uint8_t none_expected[1] = {0};
    int none = boltffi_tests_check_buf(
        boltffi_function_boltffi_tests_options_simple_maybe_double(-1),
        none_expected,
        1,
        604
    );
    if (none != 0) {
        return none;
    }
    const uint8_t encoded_some[5] = {1, 5, 0, 0, 0};
    const uint8_t encoded_some_expected[5] = {1, 10, 0, 0, 0};
    int encoded_some_result = boltffi_tests_check_buf(
        boltffi_function_boltffi_tests_options_maybe_double(encoded_some, 5),
        encoded_some_expected,
        5,
        607
    );
    if (encoded_some_result != 0) {
        return encoded_some_result;
    }
    const uint8_t scale_some[9] = {1, 0, 0, 0, 0, 0, 0, 16, 64};
    FfiBuf_u8 scaled = boltffi_function_boltffi_tests_options_maybe_scale(scale_some, 9);
    if (scaled.len != 9 || scaled.ptr[0] != 1) {
        boltffi_free_buf(scaled);
        return 609;
    }
    boltffi_free_buf(scaled);
    FfiBuf_u8 point = boltffi_function_boltffi_tests_options_maybe_point(true);
    if (point.len != 17 || point.ptr[0] != 1) {
        boltffi_free_buf(point);
        return 610;
    }
    boltffi_free_buf(point);
    ___FixtureRect rect = {9.0, 8.0, 7.0, 6.0};
    uint8_t optional_rect[1 + sizeof(___FixtureRect)] = {1};
    memcpy(optional_rect + 1, &rect, sizeof(rect));
    ___FixtureRect returned = boltffi_function_boltffi_tests_options_point_or_origin(optional_rect, sizeof(optional_rect));
    if (returned.x != 9.0 || returned.y != 8.0 || returned.width != 7.0 || returned.height != 6.0) {
        return 611;
    }
    const uint8_t label[8] = {1, 3, 0, 0, 0, 't', 'a', 'g'};
    const uint8_t label_expected[13] = {1, 8, 0, 0, 0, 't', 'a', 'g', ':', 's', 'e', 'e', 'n'};
    return boltffi_tests_check_buf(
        boltffi_function_boltffi_tests_options_maybe_label(label, 8),
        label_expected,
        13,
        612
    );
}
"#
    }

    fn customs_harness(&self) -> &'static str {
        r#"static int boltffi_tests_check_customs(void) {
    const uint8_t instant[8] = {232, 3, 0, 0, 0, 0, 0, 0};
    int shifted = boltffi_tests_check_i64_buf(
        boltffi_function_boltffi_tests_customs_shift_instant(instant, 8, 250),
        1250,
        651
    );
    if (shifted != 0) {
        return shifted;
    }
    const uint8_t maybe_expected[9] = {1, 210, 4, 0, 0, 0, 0, 0, 0};
    int maybe = boltffi_tests_check_buf(
        boltffi_function_boltffi_tests_customs_maybe_instant(true),
        maybe_expected,
        9,
        653
    );
    if (maybe != 0) {
        return maybe;
    }
    const uint8_t instants_expected[28] = {
        3, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0,
        232, 3, 0, 0, 0, 0, 0, 0,
        208, 7, 0, 0, 0, 0, 0, 0
    };
    return boltffi_tests_check_buf(
        boltffi_function_boltffi_tests_customs_instants(3),
        instants_expected,
        28,
        656
    );
}
"#
    }

    fn closures_harness(&self) -> &'static str {
        r#"static uint32_t boltffi_tests_closure_add_three(void *context, uint32_t value) {
    (void)context;
    return value + 3;
}

static FfiBuf_u8 boltffi_tests_closure_uppercase(void *context, const uint8_t *ptr, uintptr_t len) {
    (void)context;
    (void)ptr;
    (void)len;
    const uint8_t result[12] = {8, 0, 0, 0, 'H', 'E', 'L', 'L', 'O', ':', 'I', 'N'};
    return boltffi_buf_from_bytes(result, 12);
}

static void boltffi_tests_closure_release(void *context) {
    (void)context;
}

typedef struct {
    uint32_t (*invoke)(void *, uint32_t);
    void *context;
    void (*release)(void *);
} BoltffiTestsReturnedU32Closure;

static int boltffi_tests_check_closures(void) {
    if (boltffi_function_boltffi_tests_closures_apply(boltffi_tests_closure_add_three, 0, boltffi_tests_closure_release, 10) != 23) {
        return 681;
    }
    if (boltffi_function_boltffi_tests_closures_apply_boxed(boltffi_tests_closure_add_three, 0, boltffi_tests_closure_release, 10) != 26) {
        return 682;
    }
    if (boltffi_function_boltffi_tests_closures_apply_optional(boltffi_tests_closure_add_three, 0, boltffi_tests_closure_release, 10) != 13) {
        return 683;
    }
    if (boltffi_function_boltffi_tests_closures_apply_optional(0, 0, 0, 10) != 10) {
        return 684;
    }
    BoltffiTestsReturnedU32Closure adder = {0};
    FfiStatus adder_status = boltffi_function_boltffi_tests_closures_make_adder(5, &adder);
    if (adder_status.code != FFI_STATUS_OK.code || adder.invoke == 0 || adder.release == 0) {
        return 685;
    }
    uint32_t added = adder.invoke(adder.context, 7);
    adder.release(adder.context);
    if (added != 12) {
        return 686;
    }
    BoltffiTestsReturnedU32Closure boxed = {0};
    FfiStatus boxed_status = boltffi_function_boltffi_tests_closures_make_boxed_adder(6, &boxed);
    if (boxed_status.code != FFI_STATUS_OK.code || boxed.invoke == 0 || boxed.release == 0) {
        return 687;
    }
    uint32_t boxed_added = boxed.invoke(boxed.context, 8);
    boxed.release(boxed.context);
    if (boxed_added != 14) {
        return 688;
    }
    const uint8_t text[9] = {5, 0, 0, 0, 'h', 'e', 'l', 'l', 'o'};
    const uint8_t expected[12] = {8, 0, 0, 0, 'H', 'E', 'L', 'L', 'O', ':', 'I', 'N'};
    return boltffi_tests_check_buf(
        boltffi_function_boltffi_tests_closures_map_label(boltffi_tests_closure_uppercase, 0, boltffi_tests_closure_release, text, 9),
        expected,
        12,
        689
    );
}
"#
    }

    fn callbacks_harness(&self) -> &'static str {
        r#"static void boltffi_tests_callback_free(uint64_t handle) {
    (void)handle;
}

static uint64_t boltffi_tests_callback_clone(uint64_t handle) {
    return handle;
}

static int32_t boltffi_tests_callback_on_value(uint64_t handle, int32_t value) {
    return (int32_t)handle + value;
}

static uint32_t boltffi_tests_provider_count(uint64_t handle) {
    return (uint32_t)handle;
}

static FfiBuf_u8 boltffi_tests_provider_item(uint64_t handle, uint32_t index) {
    return boltffi_tests_point_buf((double)index + 1.0, (double)handle);
}

static FfiBuf_u8 boltffi_tests_vec_callback(uint64_t handle, const int32_t *values, uintptr_t len) {
    (void)len;
    return boltffi_tests_i32_vec2_buf(values[0] + (int32_t)handle, values[1] + (int32_t)handle);
}

static FfiBuf_u8 boltffi_tests_struct_callback(uint64_t handle, const uint8_t *ptr, uintptr_t len) {
    (void)ptr;
    (void)len;
    return boltffi_tests_point_buf((double)handle, (double)handle + 1.0);
}

static FfiBuf_u8 boltffi_tests_option_callback(uint64_t handle, int32_t key) {
    return boltffi_tests_option_i32_buf(key > (int32_t)handle, key * 10);
}

static ___FixtureStatus boltffi_tests_enum_callback(uint64_t handle, int32_t id) {
    return (___FixtureStatus)((handle + (uint64_t)id) % 4);
}

static int32_t boltffi_tests_multi_method_a(uint64_t handle, int32_t x) {
    return (int32_t)handle + x;
}

static int32_t boltffi_tests_multi_method_b(uint64_t handle, int32_t x, int32_t y) {
    return (int32_t)handle + x * y;
}

static int32_t boltffi_tests_multi_method_c(uint64_t handle) {
    return (int32_t)handle * 2;
}

static void boltffi_tests_async_fetch(uint64_t handle, uint32_t key, void (*callback)(void *, FfiStatus, uint64_t), void *context) {
    callback(context, FFI_STATUS_OK, handle + key);
}

static void boltffi_tests_async_find(uint64_t handle, int32_t key, void (*callback)(void *, FfiStatus, FfiBuf_u8), void *context) {
    callback(context, FFI_STATUS_OK, boltffi_tests_option_i64_buf(key > 0, (int64_t)handle + key * 100));
}

static void boltffi_tests_async_load(uint64_t handle, int64_t id, void (*callback)(void *, FfiStatus, int64_t), void *context) {
    callback(context, FFI_STATUS_OK, (int64_t)handle + id);
}

static void boltffi_tests_async_compute(uint64_t handle, int32_t a, int32_t b, void (*callback)(void *, FfiStatus, int64_t), void *context) {
    callback(context, FFI_STATUS_OK, (int64_t)handle + (int64_t)a * b);
}

static int boltffi_tests_check_callbacks(void) {
    ___SyncValueCallbackVTable value_vtable = {
        boltffi_tests_callback_free,
        boltffi_tests_callback_clone,
        boltffi_tests_callback_on_value
    };
    ___SyncDataProviderVTable provider_vtable = {
        boltffi_tests_callback_free,
        boltffi_tests_callback_clone,
        boltffi_tests_provider_count,
        boltffi_tests_provider_item
    };
    ___SyncVecCallbackVTable vec_vtable = {
        boltffi_tests_callback_free,
        boltffi_tests_callback_clone,
        boltffi_tests_vec_callback
    };
    ___SyncStructCallbackVTable struct_vtable = {
        boltffi_tests_callback_free,
        boltffi_tests_callback_clone,
        boltffi_tests_struct_callback
    };
    ___SyncOptionCallbackVTable option_vtable = {
        boltffi_tests_callback_free,
        boltffi_tests_callback_clone,
        boltffi_tests_option_callback
    };
    ___SyncEnumCallbackVTable enum_vtable = {
        boltffi_tests_callback_free,
        boltffi_tests_callback_clone,
        boltffi_tests_enum_callback
    };
    ___SyncMultiMethodCallbackVTable multi_vtable = {
        boltffi_tests_callback_free,
        boltffi_tests_callback_clone,
        boltffi_tests_multi_method_a,
        boltffi_tests_multi_method_b,
        boltffi_tests_multi_method_c
    };
    ___AsyncFetcherVTable async_fetch_vtable = {
        boltffi_tests_callback_free,
        boltffi_tests_callback_clone,
        boltffi_tests_async_fetch
    };
    ___AsyncOptionFetcherVTable async_option_vtable = {
        boltffi_tests_callback_free,
        boltffi_tests_callback_clone,
        boltffi_tests_async_find
    };
    ___AsyncMultiMethodVTable async_multi_vtable = {
        boltffi_tests_callback_free,
        boltffi_tests_callback_clone,
        boltffi_tests_async_load,
        boltffi_tests_async_compute
    };
    boltffi_register_callback_boltffi_tests_callbacks_sync_value_callback(&value_vtable);
    boltffi_register_callback_boltffi_tests_callbacks_sync_data_provider(&provider_vtable);
    boltffi_register_callback_boltffi_tests_callbacks_sync_vec_callback(&vec_vtable);
    boltffi_register_callback_boltffi_tests_callbacks_sync_struct_callback(&struct_vtable);
    boltffi_register_callback_boltffi_tests_callbacks_sync_option_callback(&option_vtable);
    boltffi_register_callback_boltffi_tests_callbacks_sync_enum_callback(&enum_vtable);
    boltffi_register_callback_boltffi_tests_callbacks_sync_multi_method_callback(&multi_vtable);
    boltffi_register_callback_boltffi_tests_callbacks_async_fetcher(&async_fetch_vtable);
    boltffi_register_callback_boltffi_tests_callbacks_async_option_fetcher(&async_option_vtable);
    boltffi_register_callback_boltffi_tests_callbacks_async_multi_method(&async_multi_vtable);
    BoltFFICallbackHandle callback = boltffi_create_callback_boltffi_tests_callbacks_sync_value_callback(5);
    if (boltffi_function_boltffi_tests_callbacks_invoke_sync_impl(callback, 10) != 15) {
        return 711;
    }
    if (boltffi_function_boltffi_tests_callbacks_invoke_sync_boxed(callback, 11) != 16) {
        return 712;
    }
    BoltFFICallbackHandle provider = boltffi_create_callback_boltffi_tests_callbacks_sync_data_provider(2);
    if (boltffi_function_boltffi_tests_callbacks_sum_provider_impl(provider) != 7.0) {
        return 713;
    }
    if (boltffi_function_boltffi_tests_callbacks_sum_provider_boxed(provider) != 7.0) {
        return 714;
    }
    BoltFFICallbackHandle vec = boltffi_create_callback_boltffi_tests_callbacks_sync_vec_callback(3);
    int32_t values[2] = {10, 20};
    int32_t expected_values[2] = {13, 23};
    int vec_impl = boltffi_tests_check_i32_vec_buf(
        boltffi_function_boltffi_tests_callbacks_invoke_vec_impl(vec, values, 2),
        expected_values,
        2,
        715
    );
    if (vec_impl != 0) {
        return vec_impl;
    }
    int vec_boxed = boltffi_tests_check_i32_vec_buf(
        boltffi_function_boltffi_tests_callbacks_invoke_vec_boxed(vec, values, 2),
        expected_values,
        2,
        718
    );
    if (vec_boxed != 0) {
        return vec_boxed;
    }
    BoltFFICallbackHandle struct_callback = boltffi_create_callback_boltffi_tests_callbacks_sync_struct_callback(6);
    FfiBuf_u8 point = boltffi_tests_point_buf(1.0, 2.0);
    int struct_impl = boltffi_tests_check_point_buf(
        boltffi_function_boltffi_tests_callbacks_invoke_struct_impl(struct_callback, point.ptr, point.len),
        6.0,
        7.0,
        721
    );
    boltffi_free_buf(point);
    if (struct_impl != 0) {
        return struct_impl;
    }
    FfiBuf_u8 point_boxed = boltffi_tests_point_buf(2.0, 3.0);
    int struct_boxed = boltffi_tests_check_point_buf(
        boltffi_function_boltffi_tests_callbacks_invoke_struct_boxed(struct_callback, point_boxed.ptr, point_boxed.len),
        6.0,
        7.0,
        724
    );
    boltffi_free_buf(point_boxed);
    if (struct_boxed != 0) {
        return struct_boxed;
    }
    BoltFFICallbackHandle option = boltffi_create_callback_boltffi_tests_callbacks_sync_option_callback(4);
    const uint8_t option_expected[5] = {1, 70, 0, 0, 0};
    int option_impl = boltffi_tests_check_buf(
        boltffi_function_boltffi_tests_callbacks_invoke_option_impl(option, 7),
        option_expected,
        5,
        727
    );
    if (option_impl != 0) {
        return option_impl;
    }
    const uint8_t none_expected[1] = {0};
    int option_boxed = boltffi_tests_check_buf(
        boltffi_function_boltffi_tests_callbacks_invoke_option_boxed(option, 3),
        none_expected,
        1,
        730
    );
    if (option_boxed != 0) {
        return option_boxed;
    }
    BoltFFICallbackHandle enum_callback = boltffi_create_callback_boltffi_tests_callbacks_sync_enum_callback(1);
    if (boltffi_function_boltffi_tests_callbacks_invoke_enum_impl(enum_callback, 2) != FIXTURE_STATUS_FAILED) {
        return 733;
    }
    if (boltffi_function_boltffi_tests_callbacks_invoke_enum_boxed(enum_callback, 3) != FIXTURE_STATUS_PENDING) {
        return 734;
    }
    BoltFFICallbackHandle multi = boltffi_create_callback_boltffi_tests_callbacks_sync_multi_method_callback(2);
    if (boltffi_function_boltffi_tests_callbacks_invoke_multi_method_impl(multi, 3, 4) != 23) {
        return 735;
    }
    if (boltffi_function_boltffi_tests_callbacks_invoke_multi_method_boxed(multi, 3, 4) != 23) {
        return 736;
    }
    if (boltffi_function_boltffi_tests_callbacks_invoke_two_sync_impl(callback, callback, 1) != 12) {
        return 737;
    }
    if (boltffi_function_boltffi_tests_callbacks_invoke_three_sync_impl(callback, callback, callback, 1) != 18) {
        return 738;
    }
    if (boltffi_function_boltffi_tests_callbacks_invoke_mixed_sync(callback, callback, 1) != 36) {
        return 739;
    }
    if (boltffi_function_boltffi_tests_callbacks_invoke_mixed_three(callback, callback, callback, 1) != 18) {
        return 740;
    }
    BoltFFICallbackHandle returned = boltffi_function_boltffi_tests_callbacks_make_value_callback(7);
    if (boltffi_function_boltffi_tests_callbacks_invoke_sync_impl(returned, 5) != 12) {
        return 741;
    }
    BoltFFICallbackHandle missing = boltffi_function_boltffi_tests_callbacks_maybe_callback(false);
    if (missing.handle != 0) {
        return 742;
    }
    BoltFFICallbackHandle present = boltffi_function_boltffi_tests_callbacks_maybe_callback(true);
    if (boltffi_function_boltffi_tests_callbacks_invoke_sync_impl(present, 1) != 8) {
        return 743;
    }
    BoltFFICallbackHandle shared = boltffi_function_boltffi_tests_callbacks_shared_callback();
    if (boltffi_function_boltffi_tests_callbacks_invoke_sync_impl(shared, 4) != 15) {
        return 744;
    }
    BoltFFICallbackHandle fallible = {0};
    FfiBuf_u8 error = boltffi_function_boltffi_tests_callbacks_try_make_callback(false, &fallible);
    int empty = boltffi_tests_check_empty_buf(error, 745);
    if (empty != 0) {
        return empty;
    }
    if (boltffi_function_boltffi_tests_callbacks_invoke_sync_impl(fallible, 1) != 14) {
        return 746;
    }
    BoltFFICallbackHandle async_fetch = boltffi_create_callback_boltffi_tests_callbacks_async_fetcher(10);
    RustFutureHandle async_value = boltffi_function_boltffi_tests_callbacks_invoke_async_impl(async_fetch, 5);
    boltffi_async_function_boltffi_tests_callbacks_invoke_async_impl_poll(async_value, 0, boltffi_tests_async_noop);
    FfiStatus async_status = FFI_STATUS_INTERNAL_ERROR;
    uint64_t async_result = boltffi_async_function_boltffi_tests_callbacks_invoke_async_impl_complete(async_value, &async_status);
    boltffi_async_function_boltffi_tests_callbacks_invoke_async_impl_free(async_value);
    if (async_status.code != FFI_STATUS_OK.code || async_result != 15) {
        return 747;
    }
    RustFutureHandle async_two = boltffi_function_boltffi_tests_callbacks_invoke_two_async_impl(async_fetch, async_fetch, 2);
    boltffi_async_function_boltffi_tests_callbacks_invoke_two_async_impl_poll(async_two, 0, boltffi_tests_async_noop);
    FfiStatus async_two_status = FFI_STATUS_INTERNAL_ERROR;
    uint64_t async_two_result = boltffi_async_function_boltffi_tests_callbacks_invoke_two_async_impl_complete(async_two, &async_two_status);
    boltffi_async_function_boltffi_tests_callbacks_invoke_two_async_impl_free(async_two);
    if (async_two_status.code != FFI_STATUS_OK.code || async_two_result != 144) {
        return 748;
    }
    RustFutureHandle async_three = boltffi_function_boltffi_tests_callbacks_invoke_three_async_impl(async_fetch, async_fetch, async_fetch, 1);
    boltffi_async_function_boltffi_tests_callbacks_invoke_three_async_impl_poll(async_three, 0, boltffi_tests_async_noop);
    FfiStatus async_three_status = FFI_STATUS_INTERNAL_ERROR;
    uint64_t async_three_result = boltffi_async_function_boltffi_tests_callbacks_invoke_three_async_impl_complete(async_three, &async_three_status);
    boltffi_async_function_boltffi_tests_callbacks_invoke_three_async_impl_free(async_three);
    if (async_three_status.code != FFI_STATUS_OK.code || async_three_result != 33) {
        return 749;
    }
    BoltFFICallbackHandle async_option = boltffi_create_callback_boltffi_tests_callbacks_async_option_fetcher(9);
    RustFutureHandle async_option_future = boltffi_function_boltffi_tests_callbacks_invoke_async_option_impl(async_option, 2);
    boltffi_async_function_boltffi_tests_callbacks_invoke_async_option_impl_poll(async_option_future, 0, boltffi_tests_async_noop);
    FfiStatus async_option_status = FFI_STATUS_INTERNAL_ERROR;
    const uint8_t async_option_expected[9] = {1, 209, 0, 0, 0, 0, 0, 0, 0};
    int async_option_check = boltffi_tests_check_buf(
        boltffi_async_function_boltffi_tests_callbacks_invoke_async_option_impl_complete(async_option_future, &async_option_status),
        async_option_expected,
        9,
        750
    );
    boltffi_async_function_boltffi_tests_callbacks_invoke_async_option_impl_free(async_option_future);
    if (async_option_status.code != FFI_STATUS_OK.code || async_option_check != 0) {
        return 753;
    }
    BoltFFICallbackHandle async_multi = boltffi_create_callback_boltffi_tests_callbacks_async_multi_method(5);
    RustFutureHandle async_multi_future = boltffi_function_boltffi_tests_callbacks_invoke_async_multi_impl(async_multi, 7, 2, 3);
    boltffi_async_function_boltffi_tests_callbacks_invoke_async_multi_impl_poll(async_multi_future, 0, boltffi_tests_async_noop);
    FfiStatus async_multi_status = FFI_STATUS_INTERNAL_ERROR;
    int64_t async_multi_result = boltffi_async_function_boltffi_tests_callbacks_invoke_async_multi_impl_complete(async_multi_future, &async_multi_status);
    boltffi_async_function_boltffi_tests_callbacks_invoke_async_multi_impl_free(async_multi_future);
    if (async_multi_status.code != FFI_STATUS_OK.code || async_multi_result != 23) {
        return 754;
    }
    const uint8_t record[27] = {
        3, 0, 0, 0, 'o', 'l', 'd',
        0, 0, 0, 0, 0, 0, 0, 64,
        0, 0, 0, 0, 0, 0, 8, 64,
        2, 0, 0, 0
    };
    RustFutureHandle echo = boltffi_function_boltffi_tests_callbacks_async_echo_message_record(record, 27);
    boltffi_async_function_boltffi_tests_callbacks_async_echo_message_record_poll(echo, 0, boltffi_tests_async_noop);
    FfiStatus echo_status = FFI_STATUS_INTERNAL_ERROR;
    int echo_check = boltffi_tests_check_buf(
        boltffi_async_function_boltffi_tests_callbacks_async_echo_message_record_complete(echo, &echo_status),
        record,
        27,
        755
    );
    boltffi_async_function_boltffi_tests_callbacks_async_echo_message_record_free(echo);
    if (echo_status.code != FFI_STATUS_OK.code || echo_check != 0) {
        return 758;
    }
    uint64_t sync_processor = boltffi_init_class_boltffi_tests_callbacks_sync_processor_new(3);
    if (boltffi_method_class_boltffi_tests_callbacks_sync_processor_apply_impl(sync_processor, callback, 4) != 17) {
        boltffi_release_class_boltffi_tests_callbacks_sync_processor(sync_processor);
        return 759;
    }
    if (boltffi_method_class_boltffi_tests_callbacks_sync_processor_apply_boxed(sync_processor, callback, 5) != 20) {
        boltffi_release_class_boltffi_tests_callbacks_sync_processor(sync_processor);
        return 760;
    }
    FfiBuf_u8 processor_point = boltffi_tests_point_buf(1.0, 2.0);
    int processor_point_check = boltffi_tests_check_point_buf(
        boltffi_method_class_boltffi_tests_callbacks_sync_processor_apply_struct_impl(sync_processor, struct_callback, processor_point.ptr, processor_point.len),
        6.0,
        7.0,
        761
    );
    boltffi_free_buf(processor_point);
    if (processor_point_check != 0) {
        boltffi_release_class_boltffi_tests_callbacks_sync_processor(sync_processor);
        return processor_point_check;
    }
    const uint8_t processor_option_expected[5] = {1, 150, 0, 0, 0};
    int processor_option_check = boltffi_tests_check_buf(
        boltffi_method_class_boltffi_tests_callbacks_sync_processor_apply_option_impl(sync_processor, option, 5),
        processor_option_expected,
        5,
        764
    );
    boltffi_release_class_boltffi_tests_callbacks_sync_processor(sync_processor);
    if (processor_option_check != 0) {
        return processor_option_check;
    }
    uint64_t async_processor = boltffi_init_class_boltffi_tests_callbacks_async_processor_new(100);
    RustFutureHandle fetched = boltffi_method_class_boltffi_tests_callbacks_async_processor_fetch_with_offset(async_processor, async_fetch, 4);
    boltffi_async_method_class_boltffi_tests_callbacks_async_processor_fetch_with_offset_poll(fetched, 0, boltffi_tests_async_noop);
    FfiStatus fetched_status = FFI_STATUS_INTERNAL_ERROR;
    uint64_t fetched_value = boltffi_async_method_class_boltffi_tests_callbacks_async_processor_fetch_with_offset_complete(fetched, &fetched_status);
    boltffi_async_method_class_boltffi_tests_callbacks_async_processor_fetch_with_offset_free(fetched);
    if (fetched_status.code != FFI_STATUS_OK.code || fetched_value != 114) {
        boltffi_release_class_boltffi_tests_callbacks_async_processor(async_processor);
        return 767;
    }
    RustFutureHandle found = boltffi_method_class_boltffi_tests_callbacks_async_processor_find_with_offset(async_processor, async_option, 2);
    boltffi_async_method_class_boltffi_tests_callbacks_async_processor_find_with_offset_poll(found, 0, boltffi_tests_async_noop);
    FfiStatus found_status = FFI_STATUS_INTERNAL_ERROR;
    const uint8_t found_expected[9] = {1, 53, 1, 0, 0, 0, 0, 0, 0};
    int found_check = boltffi_tests_check_buf(
        boltffi_async_method_class_boltffi_tests_callbacks_async_processor_find_with_offset_complete(found, &found_status),
        found_expected,
        9,
        768
    );
    boltffi_async_method_class_boltffi_tests_callbacks_async_processor_find_with_offset_free(found);
    boltffi_release_class_boltffi_tests_callbacks_async_processor(async_processor);
    if (found_status.code != FFI_STATUS_OK.code || found_check != 0) {
        return 771;
    }
    return 0;
}
"#
    }

    fn classes_harness(&self) -> &'static str {
        r#"static int boltffi_tests_check_classes(void) {
    uint64_t counter = boltffi_init_class_boltffi_tests_classes_test_counter_new(40);
    if (counter == 0) {
        return 731;
    }
    if (boltffi_method_class_boltffi_tests_classes_test_counter_get(counter) != 40) {
        boltffi_release_class_boltffi_tests_classes_test_counter(counter);
        return 732;
    }
    FfiStatus counter_set = boltffi_method_class_boltffi_tests_classes_test_counter_set(counter, 41);
    if (counter_set.code != FFI_STATUS_OK.code || boltffi_method_class_boltffi_tests_classes_test_counter_get(counter) != 41) {
        boltffi_release_class_boltffi_tests_classes_test_counter(counter);
        return 733;
    }
    if (boltffi_method_class_boltffi_tests_classes_test_counter_add(counter, 1) != 42) {
        boltffi_release_class_boltffi_tests_classes_test_counter(counter);
        return 734;
    }
    RustFutureHandle counter_get = boltffi_method_class_boltffi_tests_classes_test_counter_async_get(counter);
    boltffi_async_method_class_boltffi_tests_classes_test_counter_async_get_poll(counter_get, 0, boltffi_tests_async_noop);
    FfiStatus counter_get_status = FFI_STATUS_INTERNAL_ERROR;
    int32_t counter_async_value = boltffi_async_method_class_boltffi_tests_classes_test_counter_async_get_complete(counter_get, &counter_get_status);
    boltffi_async_method_class_boltffi_tests_classes_test_counter_async_get_free(counter_get);
    if (counter_get_status.code != FFI_STATUS_OK.code || counter_async_value != 42) {
        boltffi_release_class_boltffi_tests_classes_test_counter(counter);
        return 735;
    }
    RustFutureHandle counter_add = boltffi_method_class_boltffi_tests_classes_test_counter_async_add(counter, 8);
    boltffi_async_method_class_boltffi_tests_classes_test_counter_async_add_poll(counter_add, 0, boltffi_tests_async_noop);
    FfiStatus counter_add_status = FFI_STATUS_INTERNAL_ERROR;
    int32_t counter_add_value = boltffi_async_method_class_boltffi_tests_classes_test_counter_async_add_complete(counter_add, &counter_add_status);
    boltffi_async_method_class_boltffi_tests_classes_test_counter_async_add_free(counter_add);
    if (counter_add_status.code != FFI_STATUS_OK.code || counter_add_value != 50) {
        boltffi_release_class_boltffi_tests_classes_test_counter(counter);
        return 736;
    }
    boltffi_release_class_boltffi_tests_classes_test_counter(counter);
    uint64_t thread_safe = boltffi_init_class_boltffi_tests_classes_thread_safe_counter_new(10);
    if (thread_safe == 0) {
        return 737;
    }
    FfiStatus thread_set = boltffi_method_class_boltffi_tests_classes_thread_safe_counter_set(thread_safe, 20);
    if (thread_set.code != FFI_STATUS_OK.code) {
        boltffi_release_class_boltffi_tests_classes_thread_safe_counter(thread_safe);
        return 738;
    }
    if (boltffi_method_class_boltffi_tests_classes_thread_safe_counter_get(thread_safe) != 20) {
        boltffi_release_class_boltffi_tests_classes_thread_safe_counter(thread_safe);
        return 739;
    }
    if (boltffi_method_class_boltffi_tests_classes_thread_safe_counter_add(thread_safe, 5) != 25) {
        boltffi_release_class_boltffi_tests_classes_thread_safe_counter(thread_safe);
        return 740;
    }
    if (boltffi_method_class_boltffi_tests_classes_thread_safe_counter_increment(thread_safe) != 26) {
        boltffi_release_class_boltffi_tests_classes_thread_safe_counter(thread_safe);
        return 741;
    }
    boltffi_release_class_boltffi_tests_classes_thread_safe_counter(thread_safe);
    uint64_t map = boltffi_init_class_boltffi_tests_classes_fixture_map_new();
    if (map == 0) {
        return 739;
    }
    int32_t marker_id = 44;
    uint8_t marker_options[sizeof(marker_id)];
    memcpy(marker_options, &marker_id, sizeof(marker_id));
    uint64_t marker = boltffi_method_class_boltffi_tests_classes_fixture_map_add_marker(map, marker_options, sizeof(marker_options));
    if (marker == 0 || boltffi_method_class_boltffi_tests_classes_fixture_marker_id(marker) != 44) {
        boltffi_release_class_boltffi_tests_classes_fixture_map(map);
        return 740;
    }
    boltffi_release_class_boltffi_tests_classes_fixture_marker(marker);
    uint64_t maybe = boltffi_method_class_boltffi_tests_classes_fixture_map_maybe_marker(map, marker_options, sizeof(marker_options), true);
    if (maybe == 0 || boltffi_method_class_boltffi_tests_classes_fixture_marker_id(maybe) != 44) {
        boltffi_release_class_boltffi_tests_classes_fixture_map(map);
        return 741;
    }
    boltffi_release_class_boltffi_tests_classes_fixture_marker(maybe);
    uint64_t missing = boltffi_method_class_boltffi_tests_classes_fixture_map_maybe_marker(map, marker_options, sizeof(marker_options), false);
    if (missing != 0) {
        boltffi_release_class_boltffi_tests_classes_fixture_map(map);
        return 742;
    }
    uint64_t default_marker = boltffi_method_class_boltffi_tests_classes_fixture_map_default_marker(marker_options, sizeof(marker_options));
    if (default_marker == 0 || boltffi_method_class_boltffi_tests_classes_fixture_marker_id(default_marker) != 44) {
        boltffi_release_class_boltffi_tests_classes_fixture_map(map);
        return 743;
    }
    boltffi_release_class_boltffi_tests_classes_fixture_marker(default_marker);
    uint64_t cloned_map = boltffi_method_class_boltffi_tests_classes_fixture_map_clone_handle(map);
    if (cloned_map == 0) {
        boltffi_release_class_boltffi_tests_classes_fixture_map(map);
        return 744;
    }
    boltffi_release_class_boltffi_tests_classes_fixture_map(cloned_map);
    boltffi_release_class_boltffi_tests_classes_fixture_map(map);
    uint64_t fixture = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
    if (fixture == 0) {
        return 745;
    }
    FfiStatus set_id = boltffi_method_class_boltffi_tests_classes_class_test_fixture_set_id(fixture, 77);
    if (set_id.code != FFI_STATUS_OK.code || boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_id(fixture) != 77) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 746;
    }
    FfiBuf_u8 name = boltffi_tests_string_buf("ali", 3);
    FfiStatus set_name = boltffi_method_class_boltffi_tests_classes_class_test_fixture_set_name(fixture, name.ptr, name.len);
    boltffi_free_buf(name);
    const uint8_t ali_expected[7] = {3, 0, 0, 0, 'a', 'l', 'i'};
    int name_check = boltffi_tests_check_buf(
        boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_name(fixture),
        ali_expected,
        7,
        747
    );
    if (set_name.code != FFI_STATUS_OK.code || name_check != 0) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 750;
    }
    FfiBuf_u8 point = boltffi_tests_point_buf(3.0, 4.0);
    FfiStatus set_point = boltffi_method_class_boltffi_tests_classes_class_test_fixture_set_point(fixture, point.ptr, point.len);
    boltffi_free_buf(point);
    int point_check = boltffi_tests_check_point_buf(
        boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_point(fixture),
        3.0,
        4.0,
        751
    );
    if (set_point.code != FFI_STATUS_OK.code || point_check != 0) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 754;
    }
    FfiStatus set_status = boltffi_method_class_boltffi_tests_classes_class_test_fixture_set_status(fixture, FIXTURE_STATUS_COMPLETED);
    if (set_status.code != FFI_STATUS_OK.code || boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_status(fixture) != FIXTURE_STATUS_COMPLETED) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 755;
    }
    FfiStatus add_value = boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(fixture, 33);
    if (add_value.code != FFI_STATUS_OK.code || boltffi_method_class_boltffi_tests_classes_class_test_fixture_values_count(fixture) != 1) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 756;
    }
    FfiStatus clear_values = boltffi_method_class_boltffi_tests_classes_class_test_fixture_clear_values(fixture);
    if (clear_values.code != FFI_STATUS_OK.code || boltffi_method_class_boltffi_tests_classes_class_test_fixture_values_count(fixture) != 0) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 757;
    }
    int32_t values[3] = {1, 4, -2};
    FfiStatus set_values = boltffi_method_class_boltffi_tests_classes_class_test_fixture_set_values(fixture, values, 3);
    int32_t expected_values[3] = {1, 4, -2};
    int values_check = boltffi_tests_check_i32_vec_buf(
        boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_values(fixture),
        expected_values,
        3,
        756
    );
    if (set_values.code != FFI_STATUS_OK.code || values_check != 0) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 759;
    }
    FfiBuf_u8 optional = boltffi_tests_option_i32_buf(1, 99);
    FfiStatus set_optional = boltffi_method_class_boltffi_tests_classes_class_test_fixture_set_optional(fixture, optional.ptr, optional.len);
    boltffi_free_buf(optional);
    const uint8_t optional_expected[5] = {1, 99, 0, 0, 0};
    int optional_check = boltffi_tests_check_buf(
        boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_optional(fixture),
        optional_expected,
        5,
        760
    );
    if (set_optional.code != FFI_STATUS_OK.code || optional_check != 0) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 763;
    }
    if (boltffi_method_class_boltffi_tests_classes_class_test_fixture_values_count(fixture) != 3) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 764;
    }
    if (boltffi_method_class_boltffi_tests_classes_class_test_fixture_compute_sum(fixture) != 3) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 765;
    }
    int32_t value_out = 0;
    FfiBuf_u8 value_error = boltffi_method_class_boltffi_tests_classes_class_test_fixture_try_get_value(fixture, 1, &value_out);
    int value_empty = boltffi_tests_check_empty_buf(value_error, 766);
    if (value_empty != 0 || value_out != 4) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 767;
    }
    const uint8_t found_expected[5] = {1, 1, 0, 0, 0};
    int found = boltffi_tests_check_buf(
        boltffi_method_class_boltffi_tests_classes_class_test_fixture_find_value(fixture, 4),
        found_expected,
        5,
        768
    );
    if (found != 0) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return found;
    }
    FfiBuf_u8 near_point = boltffi_tests_point_buf(2.0, 1.0);
    int32_t near_expected[2] = {1, -2};
    int near = boltffi_tests_check_i32_vec_buf(
        boltffi_method_class_boltffi_tests_classes_class_test_fixture_values_near_point(fixture, near_point.ptr, near_point.len),
        near_expected,
        2,
        771
    );
    boltffi_free_buf(near_point);
    if (near != 0) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return near;
    }
    const uint8_t echo_data[8] = {4, 0, 0, 0, 1, 2, 3, 4};
    int echo = boltffi_tests_check_buf(
        boltffi_method_class_boltffi_tests_classes_class_test_fixture_echo_bytes(fixture, echo_data, 8),
        echo_data,
        8,
        774
    );
    if (echo != 0) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return echo;
    }
    if (boltffi_method_class_boltffi_tests_classes_class_test_fixture_with_primitives(fixture, 1, 2, 3, 4, 5, 6, 7.0f, 8.0, true) != 37) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 777;
    }
    if (boltffi_method_class_boltffi_tests_classes_class_test_fixture_static_add(5, 6) != 11) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 778;
    }
    FfiBuf_u8 left = boltffi_tests_string_buf("ab", 2);
    FfiBuf_u8 right = boltffi_tests_string_buf("cd", 2);
    const uint8_t concat_expected[8] = {4, 0, 0, 0, 'a', 'b', 'c', 'd'};
    int concat = boltffi_tests_check_buf(
        boltffi_method_class_boltffi_tests_classes_class_test_fixture_static_concat(left.ptr, left.len, right.ptr, right.len),
        concat_expected,
        8,
        779
    );
    boltffi_free_buf(left);
    boltffi_free_buf(right);
    if (concat != 0) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return concat;
    }
    int static_point = boltffi_tests_check_point_buf(
        boltffi_method_class_boltffi_tests_classes_class_test_fixture_static_make_point(8.0, 9.0),
        8.0,
        9.0,
        782
    );
    if (static_point != 0) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return static_point;
    }
    if (boltffi_method_class_boltffi_tests_classes_class_test_fixture_static_identity_status(FIXTURE_STATUS_FAILED) != FIXTURE_STATUS_FAILED) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 785;
    }
    FfiBuf_u8 parse_text = boltffi_tests_string_buf("123", 3);
    int32_t parsed = 0;
    FfiBuf_u8 parse_error = boltffi_method_class_boltffi_tests_classes_class_test_fixture_static_try_parse(parse_text.ptr, parse_text.len, &parsed);
    boltffi_free_buf(parse_text);
    int parse_empty = boltffi_tests_check_empty_buf(parse_error, 786);
    if (parse_empty != 0 || parsed != 123) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 787;
    }
    const uint8_t maybe_expected[5] = {1, 42, 0, 0, 0};
    int maybe_value = boltffi_tests_check_buf(
        boltffi_method_class_boltffi_tests_classes_class_test_fixture_static_maybe_value(true),
        maybe_expected,
        5,
        788
    );
    if (maybe_value != 0) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return maybe_value;
    }
    RustFutureHandle async_get_id = boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_get_id(fixture);
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_get_id_poll(async_get_id, 0, boltffi_tests_async_noop);
    FfiStatus async_id_status = FFI_STATUS_INTERNAL_ERROR;
    int32_t async_id = boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_get_id_complete(async_get_id, &async_id_status);
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_get_id_free(async_get_id);
    if (async_id_status.code != FFI_STATUS_OK.code || async_id != 77) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 791;
    }
    RustFutureHandle async_set_id = boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_set_id(fixture, 88);
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_set_id_poll(async_set_id, 0, boltffi_tests_async_noop);
    FfiStatus async_set_status = FFI_STATUS_INTERNAL_ERROR;
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_set_id_complete(async_set_id, &async_set_status);
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_set_id_free(async_set_id);
    if (async_set_status.code != FFI_STATUS_OK.code || boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_id(fixture) != 88) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 792;
    }
    RustFutureHandle async_get_name = boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_get_name(fixture);
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_get_name_poll(async_get_name, 0, boltffi_tests_async_noop);
    FfiStatus async_name_status = FFI_STATUS_INTERNAL_ERROR;
    int async_name = boltffi_tests_check_buf(
        boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_get_name_complete(async_get_name, &async_name_status),
        ali_expected,
        7,
        793
    );
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_get_name_free(async_get_name);
    if (async_name_status.code != FFI_STATUS_OK.code || async_name != 0) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 796;
    }
    FfiBuf_u8 zed = boltffi_tests_string_buf("zed", 3);
    RustFutureHandle async_set_name = boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_set_name(fixture, zed.ptr, zed.len);
    boltffi_free_buf(zed);
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_set_name_poll(async_set_name, 0, boltffi_tests_async_noop);
    FfiStatus async_set_name_status = FFI_STATUS_INTERNAL_ERROR;
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_set_name_complete(async_set_name, &async_set_name_status);
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_set_name_free(async_set_name);
    const uint8_t zed_expected[7] = {3, 0, 0, 0, 'z', 'e', 'd'};
    int zed_check = boltffi_tests_check_buf(
        boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_name(fixture),
        zed_expected,
        7,
        797
    );
    if (async_set_name_status.code != FFI_STATUS_OK.code || zed_check != 0) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 800;
    }
    RustFutureHandle async_sum = boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_compute_sum(fixture);
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_compute_sum_poll(async_sum, 0, boltffi_tests_async_noop);
    FfiStatus async_sum_status = FFI_STATUS_INTERNAL_ERROR;
    int32_t async_sum_value = boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_compute_sum_complete(async_sum, &async_sum_status);
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_compute_sum_free(async_sum);
    if (async_sum_status.code != FFI_STATUS_OK.code || async_sum_value != 3) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 801;
    }
    RustFutureHandle async_add_value = boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_add_value(fixture, 10);
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_add_value_poll(async_add_value, 0, boltffi_tests_async_noop);
    FfiStatus async_add_value_status = FFI_STATUS_INTERNAL_ERROR;
    int32_t async_add_value_count = boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_add_value_complete(async_add_value, &async_add_value_status);
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_add_value_free(async_add_value);
    if (async_add_value_status.code != FFI_STATUS_OK.code || async_add_value_count != 4) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 802;
    }
    RustFutureHandle async_find = boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_find(fixture, 10);
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_find_poll(async_find, 0, boltffi_tests_async_noop);
    FfiStatus async_find_status = FFI_STATUS_INTERNAL_ERROR;
    const uint8_t async_find_expected[5] = {1, 3, 0, 0, 0};
    int async_find_check = boltffi_tests_check_buf(
        boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_find_complete(async_find, &async_find_status),
        async_find_expected,
        5,
        803
    );
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_find_free(async_find);
    if (async_find_status.code != FFI_STATUS_OK.code || async_find_check != 0) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 806;
    }
    RustFutureHandle async_try_get = boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_try_get(fixture, 3);
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_try_get_poll(async_try_get, 0, boltffi_tests_async_noop);
    FfiStatus async_try_get_status = FFI_STATUS_INTERNAL_ERROR;
    int32_t async_try_get_value = 0;
    FfiBuf_u8 async_try_get_error = boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_try_get_complete(async_try_get, &async_try_get_status, &async_try_get_value);
    int async_try_get_empty = boltffi_tests_check_empty_buf(async_try_get_error, 807);
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_try_get_free(async_try_get);
    if (async_try_get_status.code != FFI_STATUS_OK.code || async_try_get_empty != 0 || async_try_get_value != 10) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 808;
    }
    const uint8_t record[27] = {
        3, 0, 0, 0, 'o', 'l', 'd',
        0, 0, 0, 0, 0, 0, 0, 64,
        0, 0, 0, 0, 0, 0, 8, 64,
        2, 0, 0, 0
    };
    RustFutureHandle async_record = boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_echo_message_record(fixture, record, 27);
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_echo_message_record_poll(async_record, 0, boltffi_tests_async_noop);
    FfiStatus async_record_status = FFI_STATUS_INTERNAL_ERROR;
    int async_record_check = boltffi_tests_check_buf(
        boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_echo_message_record_complete(async_record, &async_record_status),
        record,
        27,
        809
    );
    boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_echo_message_record_free(async_record);
    if (async_record_status.code != FFI_STATUS_OK.code || async_record_check != 0) {
        boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
        return 812;
    }
    boltffi_release_class_boltffi_tests_classes_class_test_fixture(fixture);
    uint64_t with_id = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_with_id(123);
    if (with_id == 0 || boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_id(with_id) != 123) {
        return 813;
    }
    boltffi_release_class_boltffi_tests_classes_class_test_fixture(with_id);
    uint64_t named = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_with_name(ali_expected, 7);
    if (named == 0) {
        return 794;
    }
    boltffi_release_class_boltffi_tests_classes_class_test_fixture(named);
    FfiBuf_u8 constructor_point = boltffi_tests_point_buf(6.0, 7.0);
    uint64_t with_point = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_with_point(constructor_point.ptr, constructor_point.len);
    boltffi_free_buf(constructor_point);
    if (with_point == 0) {
        return 814;
    }
    int with_point_check = boltffi_tests_check_point_buf(
        boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_point(with_point),
        6.0,
        7.0,
        815
    );
    if (with_point_check != 0) {
        return 817;
    }
    boltffi_release_class_boltffi_tests_classes_class_test_fixture(with_point);
    uint64_t with_status = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_with_status(FIXTURE_STATUS_ACTIVE);
    if (with_status == 0 || boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_status(with_status) != FIXTURE_STATUS_ACTIVE) {
        return 795;
    }
    boltffi_release_class_boltffi_tests_classes_class_test_fixture(with_status);
    FfiBuf_u8 full_point = boltffi_tests_point_buf(1.0, 2.0);
    uint64_t full = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_full(321, zed_expected, 7, full_point.ptr, full_point.len, FIXTURE_STATUS_FAILED);
    boltffi_free_buf(full_point);
    if (full == 0 || boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_id(full) != 321 || boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_status(full) != FIXTURE_STATUS_FAILED) {
        return 818;
    }
    boltffi_release_class_boltffi_tests_classes_class_test_fixture(full);
    uint64_t created = 0;
    FfiBuf_u8 create_error = boltffi_init_class_boltffi_tests_classes_class_test_fixture_try_new(9, &created);
    int create_empty = boltffi_tests_check_empty_buf(create_error, 796);
    if (create_empty != 0 || created == 0) {
        return 797;
    }
    boltffi_release_class_boltffi_tests_classes_class_test_fixture(created);
    return 0;
}
"#
    }

    fn streams_harness(&self) -> &'static str {
        r#"static int8_t boltffi_tests_stream_poll_result = -1;

static void boltffi_tests_stream_capture(uint64_t data, StreamPollResult result) {
    (void)data;
    boltffi_tests_stream_poll_result = result;
}

static int boltffi_tests_check_streams(void) {
    uint64_t stream = boltffi_init_class_boltffi_tests_streams_counter_stream_new();
    if (stream == 0) {
        return 751;
    }
    uint64_t subscription = boltffi_stream_boltffi_tests_streams_counter_stream_subscribe_subscribe(stream);
    if (subscription == 0) {
        boltffi_release_class_boltffi_tests_streams_counter_stream(stream);
        return 752;
    }
    FfiStatus first = boltffi_method_class_boltffi_tests_streams_counter_stream_emit(stream, 10);
    FfiStatus second = boltffi_method_class_boltffi_tests_streams_counter_stream_emit(stream, 20);
    if (first.code != FFI_STATUS_OK.code || second.code != FFI_STATUS_OK.code) {
        boltffi_stream_boltffi_tests_streams_counter_stream_subscribe_free(subscription);
        boltffi_release_class_boltffi_tests_streams_counter_stream(stream);
        return 753;
    }
    if (boltffi_stream_boltffi_tests_streams_counter_stream_subscribe_wait(subscription, 0) != 1) {
        boltffi_stream_boltffi_tests_streams_counter_stream_subscribe_free(subscription);
        boltffi_release_class_boltffi_tests_streams_counter_stream(stream);
        return 754;
    }
    boltffi_tests_stream_poll_result = -1;
    boltffi_stream_boltffi_tests_streams_counter_stream_subscribe_poll(subscription, 0, boltffi_tests_stream_capture);
    if (boltffi_tests_stream_poll_result != 0) {
        boltffi_stream_boltffi_tests_streams_counter_stream_subscribe_free(subscription);
        boltffi_release_class_boltffi_tests_streams_counter_stream(stream);
        return 755;
    }
    int32_t values[2] = {0, 0};
    uintptr_t count = boltffi_stream_boltffi_tests_streams_counter_stream_subscribe_pop_batch(subscription, values, 2);
    if (count != 2 || values[0] != 10 || values[1] != 20) {
        boltffi_stream_boltffi_tests_streams_counter_stream_subscribe_free(subscription);
        boltffi_release_class_boltffi_tests_streams_counter_stream(stream);
        return 756;
    }
    int32_t batch_values[3] = {30, 40, 50};
    if (boltffi_method_class_boltffi_tests_streams_counter_stream_emit_batch(stream, batch_values, 3) != 3) {
        boltffi_stream_boltffi_tests_streams_counter_stream_subscribe_free(subscription);
        boltffi_release_class_boltffi_tests_streams_counter_stream(stream);
        return 757;
    }
    boltffi_stream_boltffi_tests_streams_counter_stream_subscribe_unsubscribe(subscription);
    if (boltffi_stream_boltffi_tests_streams_counter_stream_subscribe_wait(subscription, 0) != -1) {
        boltffi_stream_boltffi_tests_streams_counter_stream_subscribe_free(subscription);
        boltffi_release_class_boltffi_tests_streams_counter_stream(stream);
        return 758;
    }
    boltffi_stream_boltffi_tests_streams_counter_stream_subscribe_free(subscription);
    boltffi_release_class_boltffi_tests_streams_counter_stream(stream);
    uint64_t point_stream = boltffi_init_class_boltffi_tests_streams_point_stream_new();
    if (point_stream == 0) {
        return 759;
    }
    uint64_t point_subscription = boltffi_stream_boltffi_tests_streams_point_stream_subscribe_subscribe(point_stream);
    if (point_subscription == 0) {
        boltffi_release_class_boltffi_tests_streams_point_stream(point_stream);
        return 760;
    }
    FfiBuf_u8 point = boltffi_tests_point_buf(1.5, 2.5);
    FfiStatus point_emit = boltffi_method_class_boltffi_tests_streams_point_stream_emit(point_stream, point.ptr, point.len);
    boltffi_free_buf(point);
    if (point_emit.code != FFI_STATUS_OK.code) {
        boltffi_stream_boltffi_tests_streams_point_stream_subscribe_free(point_subscription);
        boltffi_release_class_boltffi_tests_streams_point_stream(point_stream);
        return 761;
    }
    int point_batch = boltffi_tests_check_point_vec_buf(
        boltffi_stream_boltffi_tests_streams_point_stream_subscribe_pop_batch(point_subscription, 1),
        1.5,
        2.5,
        762
    );
    if (point_batch != 0) {
        boltffi_stream_boltffi_tests_streams_point_stream_subscribe_free(point_subscription);
        boltffi_release_class_boltffi_tests_streams_point_stream(point_stream);
        return point_batch;
    }
    FfiBuf_u8 point_again = boltffi_tests_point_buf(3.5, 4.5);
    FfiStatus point_again_emit = boltffi_method_class_boltffi_tests_streams_point_stream_emit(point_stream, point_again.ptr, point_again.len);
    boltffi_free_buf(point_again);
    if (point_again_emit.code != FFI_STATUS_OK.code) {
        boltffi_stream_boltffi_tests_streams_point_stream_subscribe_free(point_subscription);
        boltffi_release_class_boltffi_tests_streams_point_stream(point_stream);
        return 765;
    }
    boltffi_tests_stream_poll_result = -1;
    boltffi_stream_boltffi_tests_streams_point_stream_subscribe_poll(point_subscription, 0, boltffi_tests_stream_capture);
    if (boltffi_tests_stream_poll_result != 0) {
        boltffi_stream_boltffi_tests_streams_point_stream_subscribe_free(point_subscription);
        boltffi_release_class_boltffi_tests_streams_point_stream(point_stream);
        return 766;
    }
    boltffi_stream_boltffi_tests_streams_point_stream_subscribe_unsubscribe(point_subscription);
    if (boltffi_stream_boltffi_tests_streams_point_stream_subscribe_wait(point_subscription, 0) != -1) {
        boltffi_stream_boltffi_tests_streams_point_stream_subscribe_free(point_subscription);
        boltffi_release_class_boltffi_tests_streams_point_stream(point_stream);
        return 767;
    }
    boltffi_stream_boltffi_tests_streams_point_stream_subscribe_free(point_subscription);
    boltffi_release_class_boltffi_tests_streams_point_stream(point_stream);
    uint64_t label_stream = boltffi_init_class_boltffi_tests_streams_label_stream_new();
    if (label_stream == 0) {
        return 768;
    }
    uint64_t label_subscription = boltffi_stream_boltffi_tests_streams_label_stream_subscribe_subscribe(label_stream);
    if (label_subscription == 0) {
        boltffi_release_class_boltffi_tests_streams_label_stream(label_stream);
        return 769;
    }
    FfiBuf_u8 label = boltffi_tests_string_buf("one", 3);
    FfiStatus label_emit = boltffi_method_class_boltffi_tests_streams_label_stream_emit(label_stream, label.ptr, label.len);
    boltffi_free_buf(label);
    if (label_emit.code != FFI_STATUS_OK.code) {
        boltffi_stream_boltffi_tests_streams_label_stream_subscribe_free(label_subscription);
        boltffi_release_class_boltffi_tests_streams_label_stream(label_stream);
        return 770;
    }
    FfiBuf_u8 labels = boltffi_stream_boltffi_tests_streams_label_stream_subscribe_pop_batch(label_subscription, 1);
    const uint8_t joined_expected[7] = {3, 0, 0, 0, 'o', 'n', 'e'};
    int joined = boltffi_tests_check_buf(
        boltffi_function_boltffi_tests_vectors_join_labels(labels.ptr, labels.len),
        joined_expected,
        7,
        771
    );
    boltffi_free_buf(labels);
    if (joined != 0) {
        boltffi_stream_boltffi_tests_streams_label_stream_subscribe_free(label_subscription);
        boltffi_release_class_boltffi_tests_streams_label_stream(label_stream);
        return joined;
    }
    FfiBuf_u8 label_again = boltffi_tests_string_buf("two", 3);
    FfiStatus label_again_emit = boltffi_method_class_boltffi_tests_streams_label_stream_emit(label_stream, label_again.ptr, label_again.len);
    boltffi_free_buf(label_again);
    if (label_again_emit.code != FFI_STATUS_OK.code) {
        boltffi_stream_boltffi_tests_streams_label_stream_subscribe_free(label_subscription);
        boltffi_release_class_boltffi_tests_streams_label_stream(label_stream);
        return 774;
    }
    boltffi_tests_stream_poll_result = -1;
    boltffi_stream_boltffi_tests_streams_label_stream_subscribe_poll(label_subscription, 0, boltffi_tests_stream_capture);
    if (boltffi_tests_stream_poll_result != 0) {
        boltffi_stream_boltffi_tests_streams_label_stream_subscribe_free(label_subscription);
        boltffi_release_class_boltffi_tests_streams_label_stream(label_stream);
        return 775;
    }
    boltffi_stream_boltffi_tests_streams_label_stream_subscribe_unsubscribe(label_subscription);
    if (boltffi_stream_boltffi_tests_streams_label_stream_subscribe_wait(label_subscription, 0) != -1) {
        boltffi_stream_boltffi_tests_streams_label_stream_subscribe_free(label_subscription);
        boltffi_release_class_boltffi_tests_streams_label_stream(label_stream);
        return 776;
    }
    boltffi_stream_boltffi_tests_streams_label_stream_subscribe_free(label_subscription);
    boltffi_release_class_boltffi_tests_streams_label_stream(label_stream);
    return 0;
}
"#
    }

    fn results_harness(&self) -> &'static str {
        r#"static int boltffi_tests_check_results(void) {
    int32_t fixture_error_value = 0;
    FfiBuf_u8 fixture_error = boltffi_function_boltffi_tests_results_fallible_divide(12, 3, &fixture_error_value);
    int fixture_error_empty = boltffi_tests_check_empty_buf(fixture_error, 690);
    if (fixture_error_empty != 0 || fixture_error_value != 4) {
        return 691;
    }
    FfiBuf_u8 lookup = {0};
    FfiBuf_u8 lookup_error = boltffi_function_boltffi_tests_results_fallible_lookup(2, &lookup);
    int lookup_empty = boltffi_tests_check_empty_buf(lookup_error, 692);
    if (lookup_empty != 0) {
        return lookup_empty;
    }
    const uint8_t lookup_expected[7] = {3, 0, 0, 0, 't', 'w', 'o'};
    int lookup_check = boltffi_tests_check_buf(lookup, lookup_expected, 7, 693);
    if (lookup_check != 0) {
        return lookup_check;
    }
    int32_t simple_value = 0;
    FfiBuf_u8 simple_error = boltffi_function_boltffi_tests_results_simple_try_divide(9, 3, &simple_value);
    int simple_empty = boltffi_tests_check_empty_buf(simple_error, 696);
    if (simple_empty != 0 || simple_value != 3) {
        return 697;
    }
    int32_t value = 0;
    FfiBuf_u8 ok_error = boltffi_function_boltffi_tests_results_try_divide(12, 3, &value);
    if (ok_error.len != 0) {
        boltffi_free_buf(ok_error);
        return 701;
    }
    if (value != 4) {
        return 702;
    }
    FfiBuf_u8 ping = boltffi_function_boltffi_tests_results_try_ping(false);
    int ping_empty = boltffi_tests_check_empty_buf(ping, 703);
    if (ping_empty != 0) {
        return ping_empty;
    }
    FfiBuf_u8 greeting = {0};
    FfiBuf_u8 greeting_error = boltffi_function_boltffi_tests_results_try_greet(false, &greeting);
    int greeting_empty = boltffi_tests_check_empty_buf(greeting_error, 704);
    if (greeting_empty != 0) {
        return greeting_empty;
    }
    const uint8_t greeting_expected[9] = {5, 0, 0, 0, 'h', 'e', 'l', 'l', 'o'};
    int greeting_check = boltffi_tests_check_buf(greeting, greeting_expected, 9, 705);
    if (greeting_check != 0) {
        return greeting_check;
    }
    ___FixtureRect rect = {0};
    FfiBuf_u8 rect_error = boltffi_function_boltffi_tests_results_try_rect(false, &rect);
    int rect_empty = boltffi_tests_check_empty_buf(rect_error, 708);
    if (rect_empty != 0) {
        return rect_empty;
    }
    if (rect.x != 1.0 || rect.y != 2.0 || rect.width != 3.0 || rect.height != 4.0) {
        return 709;
    }
    FfiBuf_u8 message = {0};
    FfiBuf_u8 message_error = boltffi_function_boltffi_tests_results_try_message(false, &message);
    int message_empty = boltffi_tests_check_empty_buf(message_error, 710);
    if (message_empty != 0) {
        return message_empty;
    }
    if (message.len < 6 || message.ptr[0] != 2 || message.ptr[4] != 'o' || message.ptr[5] != 'k') {
        boltffi_free_buf(message);
        return 711;
    }
    boltffi_free_buf(message);
    int32_t failed_value = 0;
    const uint8_t error_expected[18] = {14, 0, 0, 0, 'd', 'i', 'v', 'i', 'd', 'e', ' ', 'b', 'y', ' ', 'z', 'e', 'r', 'o'};
    int divide_error = boltffi_tests_check_buf(
        boltffi_function_boltffi_tests_results_try_divide(12, 0, &failed_value),
        error_expected,
        18,
        712
    );
    if (divide_error != 0) {
        return divide_error;
    }
    int32_t status_return = 0;
    int status_error = boltffi_tests_check_i32_buf(
        boltffi_function_boltffi_tests_results_try_status_err(-1, &status_return),
        FIXTURE_STATUS_FAILED,
        715
    );
    if (status_error != 0) {
        return status_error;
    }
    int32_t shape_return = 0;
    FfiBuf_u8 shape_error = boltffi_function_boltffi_tests_results_try_shape_err(-5, &shape_return);
    if (shape_error.len == 0) {
        return 718;
    }
    boltffi_free_buf(shape_error);
    RustFutureHandle fetch = boltffi_function_boltffi_tests_results_async_fallible_fetch(8);
    boltffi_async_function_boltffi_tests_results_async_fallible_fetch_poll(fetch, 0, boltffi_tests_async_noop);
    FfiStatus fetch_status = FFI_STATUS_INTERNAL_ERROR;
    FfiBuf_u8 fetch_value = {0};
    FfiBuf_u8 fetch_error = boltffi_async_function_boltffi_tests_results_async_fallible_fetch_complete(fetch, &fetch_status, &fetch_value);
    int fetch_empty = boltffi_tests_check_empty_buf(fetch_error, 719);
    if (fetch_status.code != FFI_STATUS_OK.code || fetch_empty != 0) {
        boltffi_async_function_boltffi_tests_results_async_fallible_fetch_free(fetch);
        return 720;
    }
    const uint8_t fetch_expected[11] = {7, 0, 0, 0, 'v', 'a', 'l', 'u', 'e', '_', '8'};
    int fetch_check = boltffi_tests_check_buf(fetch_value, fetch_expected, 11, 721);
    boltffi_async_function_boltffi_tests_results_async_fallible_fetch_free(fetch);
    if (fetch_check != 0) {
        return fetch_check;
    }
    uint64_t service = boltffi_init_class_boltffi_tests_results_fallible_service_new();
    if (service == 0) {
        return 724;
    }
    FfiStatus service_mode = boltffi_method_class_boltffi_tests_results_fallible_service_set_failure_mode(service, 0);
    int32_t service_value = 0;
    FfiBuf_u8 service_error = boltffi_method_class_boltffi_tests_results_fallible_service_get_value(service, 9, &service_value);
    int service_empty = boltffi_tests_check_empty_buf(service_error, 725);
    if (service_mode.code != FFI_STATUS_OK.code || service_empty != 0 || service_value != 18) {
        boltffi_release_class_boltffi_tests_results_fallible_service(service);
        return 726;
    }
    const uint8_t service_option_expected[5] = {1, 15, 0, 0, 0};
    int service_option = boltffi_tests_check_buf(
        boltffi_method_class_boltffi_tests_results_fallible_service_get_optional(service, 5),
        service_option_expected,
        5,
        727
    );
    if (service_option != 0) {
        boltffi_release_class_boltffi_tests_results_fallible_service(service);
        return service_option;
    }
    FfiBuf_u8 nested_value = {0};
    FfiBuf_u8 nested_error = boltffi_method_class_boltffi_tests_results_fallible_service_get_nested_result(service, 3, &nested_value);
    int nested_empty = boltffi_tests_check_empty_buf(nested_error, 730);
    const uint8_t nested_expected[5] = {1, 12, 0, 0, 0};
    int nested_check = boltffi_tests_check_buf(nested_value, nested_expected, 5, 731);
    if (nested_empty != 0 || nested_check != 0) {
        boltffi_release_class_boltffi_tests_results_fallible_service(service);
        return 734;
    }
    RustFutureHandle async_service = boltffi_method_class_boltffi_tests_results_fallible_service_async_get_value(service, 4);
    boltffi_async_method_class_boltffi_tests_results_fallible_service_async_get_value_poll(async_service, 0, boltffi_tests_async_noop);
    FfiStatus async_service_status = FFI_STATUS_INTERNAL_ERROR;
    int32_t async_service_value = 0;
    FfiBuf_u8 async_service_error = boltffi_async_method_class_boltffi_tests_results_fallible_service_async_get_value_complete(async_service, &async_service_status, &async_service_value);
    int async_service_empty = boltffi_tests_check_empty_buf(async_service_error, 735);
    boltffi_async_method_class_boltffi_tests_results_fallible_service_async_get_value_free(async_service);
    if (async_service_status.code != FFI_STATUS_OK.code || async_service_empty != 0 || async_service_value != 8) {
        boltffi_release_class_boltffi_tests_results_fallible_service(service);
        return 736;
    }
    boltffi_release_class_boltffi_tests_results_fallible_service(service);
    uint64_t task = boltffi_init_class_boltffi_tests_results_cancellable_task_new();
    if (task == 0) {
        return 737;
    }
    if (boltffi_method_class_boltffi_tests_results_cancellable_task_was_started(task)) {
        boltffi_release_class_boltffi_tests_results_cancellable_task(task);
        return 738;
    }
    RustFutureHandle instant = boltffi_method_class_boltffi_tests_results_cancellable_task_instant_task(task);
    boltffi_async_method_class_boltffi_tests_results_cancellable_task_instant_task_poll(instant, 0, boltffi_tests_async_noop);
    FfiStatus instant_status = FFI_STATUS_INTERNAL_ERROR;
    int32_t instant_value = boltffi_async_method_class_boltffi_tests_results_cancellable_task_instant_task_complete(instant, &instant_status);
    boltffi_async_method_class_boltffi_tests_results_cancellable_task_instant_task_free(instant);
    if (instant_status.code != FFI_STATUS_OK.code || instant_value != 99) {
        boltffi_release_class_boltffi_tests_results_cancellable_task(task);
        return 739;
    }
    if (!boltffi_method_class_boltffi_tests_results_cancellable_task_was_started(task) || !boltffi_method_class_boltffi_tests_results_cancellable_task_was_completed(task)) {
        boltffi_release_class_boltffi_tests_results_cancellable_task(task);
        return 740;
    }
    if (boltffi_method_class_boltffi_tests_results_cancellable_task_iteration_count(task) != 0) {
        boltffi_release_class_boltffi_tests_results_cancellable_task(task);
        return 741;
    }
    boltffi_release_class_boltffi_tests_results_cancellable_task(task);
    uint64_t cancel_task = boltffi_init_class_boltffi_tests_results_cancellable_task_new();
    if (cancel_task == 0) {
        return 742;
    }
    RustFutureHandle long_running = boltffi_method_class_boltffi_tests_results_cancellable_task_long_running_task(cancel_task);
    boltffi_async_method_class_boltffi_tests_results_cancellable_task_long_running_task_poll(long_running, 0, boltffi_tests_async_noop);
    if (!boltffi_method_class_boltffi_tests_results_cancellable_task_was_started(cancel_task) || boltffi_method_class_boltffi_tests_results_cancellable_task_was_completed(cancel_task)) {
        boltffi_async_method_class_boltffi_tests_results_cancellable_task_long_running_task_free(long_running);
        boltffi_release_class_boltffi_tests_results_cancellable_task(cancel_task);
        return 743;
    }
    boltffi_async_method_class_boltffi_tests_results_cancellable_task_long_running_task_cancel(long_running);
    boltffi_async_method_class_boltffi_tests_results_cancellable_task_long_running_task_free(long_running);
    boltffi_release_class_boltffi_tests_results_cancellable_task(cancel_task);
    return 0;
}
"#
    }

    fn asynchronous_harness(&self) -> &'static str {
        r#"static int boltffi_tests_check_asynchronous(void) {
    RustFutureHandle future = boltffi_function_boltffi_tests_asynchronous_async_add(20, 22);
    if (future == 0) {
        return 801;
    }
    boltffi_async_function_boltffi_tests_asynchronous_async_add_poll(
        future,
        0,
        boltffi_tests_async_noop
    );
    FfiStatus status = FFI_STATUS_INTERNAL_ERROR;
    int32_t value = boltffi_async_function_boltffi_tests_asynchronous_async_add_complete(future, &status);
    if (status.code != FFI_STATUS_OK.code) {
        boltffi_async_function_boltffi_tests_asynchronous_async_add_free(future);
        return 802;
    }
    if (value != 42) {
        boltffi_async_function_boltffi_tests_asynchronous_async_add_free(future);
        return 803;
    }
    boltffi_async_function_boltffi_tests_asynchronous_async_add_free(future);
    const uint8_t ali[7] = {3, 0, 0, 0, 'A', 'l', 'i'};
    RustFutureHandle greet = boltffi_function_boltffi_tests_asynchronous_async_greet(ali, 7);
    if (greet == 0) {
        return 804;
    }
    boltffi_async_function_boltffi_tests_asynchronous_async_greet_poll(
        greet,
        0,
        boltffi_tests_async_noop
    );
    FfiStatus greet_status = FFI_STATUS_INTERNAL_ERROR;
    const uint8_t greet_expected[13] = {9, 0, 0, 0, 'h', 'e', 'l', 'l', 'o', ' ', 'A', 'l', 'i'};
    int greet_check = boltffi_tests_check_buf(
        boltffi_async_function_boltffi_tests_asynchronous_async_greet_complete(greet, &greet_status),
        greet_expected,
        13,
        805
    );
    boltffi_async_function_boltffi_tests_asynchronous_async_greet_free(greet);
    if (greet_status.code != FFI_STATUS_OK.code) {
        return 808;
    }
    if (greet_check != 0) {
        return greet_check;
    }
    RustFutureHandle rect_future = boltffi_function_boltffi_tests_asynchronous_async_make_rect(2.0, -3.0);
    if (rect_future == 0) {
        return 809;
    }
    boltffi_async_function_boltffi_tests_asynchronous_async_make_rect_poll(
        rect_future,
        0,
        boltffi_tests_async_noop
    );
    FfiStatus rect_status = FFI_STATUS_INTERNAL_ERROR;
    ___FixtureRect rect = boltffi_async_function_boltffi_tests_asynchronous_async_make_rect_complete(rect_future, &rect_status);
    boltffi_async_function_boltffi_tests_asynchronous_async_make_rect_free(rect_future);
    if (rect_status.code != FFI_STATUS_OK.code) {
        return 810;
    }
    if (rect.x != 2.0 || rect.y != -3.0 || rect.width != 3.0 || rect.height != 4.0) {
        return 811;
    }
    RustFutureHandle ping = boltffi_function_boltffi_tests_asynchronous_async_ping();
    if (ping == 0) {
        return 812;
    }
    boltffi_async_function_boltffi_tests_asynchronous_async_ping_poll(
        ping,
        0,
        boltffi_tests_async_noop
    );
    FfiStatus ping_status = FFI_STATUS_INTERNAL_ERROR;
    boltffi_async_function_boltffi_tests_asynchronous_async_ping_complete(ping, &ping_status);
    boltffi_async_function_boltffi_tests_asynchronous_async_ping_free(ping);
    if (ping_status.code != FFI_STATUS_OK.code) {
        return 813;
    }
    return 0;
}
"#
    }
}

impl SignatureMap {
    fn new(bindings: &Bindings<Native>, contract: &CBridgeContract) -> Self {
        Self {
            direct_records: Self::direct_records(bindings, contract),
            c_style_enums: Self::c_style_enums(bindings, contract),
            callback_vtables: Self::callback_vtables(contract),
            nullable_function_pointers: Self::nullable_function_pointers(bindings, contract),
        }
    }

    fn parameter_ty(&self, symbol: &str, index: usize, ty: &Type) -> TokenStream {
        let ty = self.ty(ty, SignaturePosition::Input);
        if self
            .nullable_function_pointers
            .contains(&(symbol.to_owned(), index))
        {
            quote! { Option<#ty> }
        } else {
            ty
        }
    }

    fn ty(&self, ty: &Type, position: SignaturePosition) -> TokenStream {
        match ty {
            Type::Void => quote! { () },
            Type::Bool => quote! { bool },
            Type::Int8 => quote! { i8 },
            Type::Uint8 => quote! { u8 },
            Type::Int16 => quote! { i16 },
            Type::Uint16 => quote! { u16 },
            Type::Int32 => quote! { i32 },
            Type::Uint32 => quote! { u32 },
            Type::Int64 => quote! { i64 },
            Type::Uint64 => quote! { u64 },
            Type::Float32 => quote! { f32 },
            Type::Float64 => quote! { f64 },
            Type::SignedPointerWidth => quote! { isize },
            Type::PointerWidth => quote! { usize },
            Type::Status => quote! { ::boltffi::__private::FfiStatus },
            Type::Buffer => quote! { ::boltffi::__private::FfiBuf },
            Type::Span => quote! { ::boltffi::__private::FfiSpan },
            Type::FutureHandle => quote! { ::boltffi::__private::RustFutureHandle },
            Type::StreamPollResult => quote! { ::boltffi::__private::StreamPollResult },
            Type::WaitResult => quote! { i32 },
            Type::CallbackHandle(_) => quote! { ::boltffi::__private::CallbackHandle },
            Type::DirectRecord(name) => {
                self.passable(&self.direct_records, name.as_str(), position)
            }
            Type::CStyleEnum { name, .. } => {
                self.passable(&self.c_style_enums, name.as_str(), position)
            }
            Type::ConstPointer(pointee) => {
                let pointee = self.pointee_ty(pointee, position);
                quote! { *const #pointee }
            }
            Type::MutPointer(pointee) => {
                let pointee = self.pointee_ty(pointee, position);
                quote! { *mut #pointee }
            }
            Type::FunctionPointer { returns, params } => self.function_pointer(returns, params),
            Type::Named(name) => {
                let ty = self.rust_ty(&self.callback_vtables, name.as_str());
                quote! { #ty }
            }
            other => panic!("unsupported C signature type: {other:?}"),
        }
    }

    fn layout_ty(&self, ty: &Type) -> TokenStream {
        match ty {
            Type::Bool => quote! { bool },
            Type::Int8 => quote! { i8 },
            Type::Uint8 => quote! { u8 },
            Type::Int16 => quote! { i16 },
            Type::Uint16 => quote! { u16 },
            Type::Int32 => quote! { i32 },
            Type::Uint32 => quote! { u32 },
            Type::Int64 => quote! { i64 },
            Type::Uint64 => quote! { u64 },
            Type::Float32 => quote! { f32 },
            Type::Float64 => quote! { f64 },
            Type::SignedPointerWidth => quote! { isize },
            Type::PointerWidth => quote! { usize },
            other => panic!("unsupported C layout type: {other:?}"),
        }
    }

    fn rust_ty(&self, names: &BTreeMap<String, String>, c_name: &str) -> TokenStream {
        let rust_name = names
            .get(c_name)
            .unwrap_or_else(|| panic!("{c_name} is not mapped to a Rust layout type"));
        let ty = format_ident!("{}", rust_name);
        quote! { crate::#ty }
    }

    fn pointee_ty(&self, ty: &Type, position: SignaturePosition) -> TokenStream {
        match ty {
            Type::Void => quote! { ::core::ffi::c_void },
            _ => self.ty(ty, position),
        }
    }

    fn function_pointer(&self, returns: &Type, params: &[Type]) -> TokenStream {
        if self.is_future_continuation(returns, params) {
            return quote! {
                extern "C" fn(u64, ::boltffi::__private::rustfuture::RustFuturePoll)
            };
        }
        if self.is_stream_continuation(returns, params) {
            return quote! {
                extern "C" fn(u64, ::boltffi::__private::StreamPollResult)
            };
        }
        let returns = self.ty(returns, SignaturePosition::Output);
        let params = params
            .iter()
            .map(|param| self.ty(param, SignaturePosition::Input))
            .collect::<Vec<_>>();
        quote! { unsafe extern "C" fn(#(#params),*) -> #returns }
    }

    fn nullable_function_pointers(
        bindings: &Bindings<Native>,
        contract: &CBridgeContract,
    ) -> BTreeSet<(String, usize)> {
        let callables = Self::exported_callables(bindings);
        contract
            .functions()
            .iter()
            .filter_map(|function| {
                callables
                    .get(function.name())
                    .map(|callable| (function, *callable))
            })
            .flat_map(|(function, callable)| {
                Self::nullable_function_pointers_for_callable(function, callable)
            })
            .collect()
    }

    fn nullable_function_pointers_for_callable(
        function: &Function,
        callable: &ExportedCallable<Native>,
    ) -> BTreeSet<(String, usize)> {
        let mut nullable = BTreeSet::new();
        let mut source_index = 0usize;
        let mut skip_receiver = callable.receiver().is_some();
        function.parameter_groups().iter().for_each(|group| {
            if skip_receiver {
                skip_receiver = false;
                return;
            }
            if let Some((call, release)) = Self::closure_function_pointer_indices(group) {
                if callable
                    .params()
                    .get(source_index)
                    .and_then(|parameter| match parameter.payload() {
                        IncomingParam::Closure(closure) => Some(closure.presence()),
                        IncomingParam::Value(_) => None,
                    })
                    .is_some_and(|presence| presence == HandlePresence::Nullable)
                {
                    nullable.insert((function.name().to_owned(), call));
                    nullable.insert((function.name().to_owned(), release));
                }
                source_index += 1;
            } else if Self::source_parameter_group(group) {
                source_index += 1;
            }
        });
        nullable
    }

    fn closure_function_pointer_indices(group: &ParameterGroup) -> Option<(usize, usize)> {
        match group {
            ParameterGroup::Closure(closure) => {
                Some((closure.call().position(), closure.release().position()))
            }
            _ => None,
        }
    }

    fn source_parameter_group(group: &ParameterGroup) -> bool {
        matches!(
            group,
            ParameterGroup::Value(_)
                | ParameterGroup::ByteSlice(_)
                | ParameterGroup::DirectVector(_)
                | ParameterGroup::EncodedWriteback(_)
                | ParameterGroup::DirectWriteback(_)
                | ParameterGroup::Closure(_)
        )
    }

    fn exported_callables(
        bindings: &Bindings<Native>,
    ) -> BTreeMap<String, &ExportedCallable<Native>> {
        bindings
            .decls()
            .iter()
            .fold(BTreeMap::new(), |mut callables, decl| {
                match decl {
                    Decl::Function(function) => {
                        Self::insert_callable(
                            &mut callables,
                            function.symbol(),
                            function.callable(),
                        );
                    }
                    Decl::Record(record) => match record.as_ref() {
                        RecordDecl::Direct(record) => {
                            record.initializers().iter().for_each(|initializer| {
                                Self::insert_callable(
                                    &mut callables,
                                    initializer.symbol(),
                                    initializer.callable(),
                                )
                            });
                            record.methods().iter().for_each(|method| {
                                Self::insert_callable(
                                    &mut callables,
                                    method.target(),
                                    method.callable(),
                                )
                            });
                        }
                        RecordDecl::Encoded(record) => {
                            record.initializers().iter().for_each(|initializer| {
                                Self::insert_callable(
                                    &mut callables,
                                    initializer.symbol(),
                                    initializer.callable(),
                                )
                            });
                            record.methods().iter().for_each(|method| {
                                Self::insert_callable(
                                    &mut callables,
                                    method.target(),
                                    method.callable(),
                                )
                            });
                        }
                        _ => {}
                    },
                    Decl::Enum(enumeration) => match enumeration.as_ref() {
                        EnumDecl::CStyle(enumeration) => {
                            enumeration.initializers().iter().for_each(|initializer| {
                                Self::insert_callable(
                                    &mut callables,
                                    initializer.symbol(),
                                    initializer.callable(),
                                )
                            });
                            enumeration.methods().iter().for_each(|method| {
                                Self::insert_callable(
                                    &mut callables,
                                    method.target(),
                                    method.callable(),
                                )
                            });
                        }
                        EnumDecl::Data(enumeration) => {
                            enumeration.initializers().iter().for_each(|initializer| {
                                Self::insert_callable(
                                    &mut callables,
                                    initializer.symbol(),
                                    initializer.callable(),
                                )
                            });
                            enumeration.methods().iter().for_each(|method| {
                                Self::insert_callable(
                                    &mut callables,
                                    method.target(),
                                    method.callable(),
                                )
                            });
                        }
                        _ => {}
                    },
                    Decl::Class(class) => {
                        class.initializers().iter().for_each(|initializer| {
                            Self::insert_callable(
                                &mut callables,
                                initializer.symbol(),
                                initializer.callable(),
                            )
                        });
                        class.methods().iter().for_each(|method| {
                            Self::insert_callable(
                                &mut callables,
                                method.target(),
                                method.callable(),
                            )
                        });
                    }
                    Decl::Callback(_)
                    | Decl::Stream(_)
                    | Decl::Constant(_)
                    | Decl::CustomType(_) => {}
                    _ => {}
                }
                callables
            })
    }

    fn insert_callable<'bindings>(
        callables: &mut BTreeMap<String, &'bindings ExportedCallable<Native>>,
        symbol: &boltffi_binding::NativeSymbol,
        callable: &'bindings ExportedCallable<Native>,
    ) {
        callables.insert(symbol.name().as_str().to_owned(), callable);
    }

    fn is_future_continuation(&self, returns: &Type, params: &[Type]) -> bool {
        matches!(returns, Type::Void) && matches!(params, [Type::Uint64, Type::Int8])
    }

    fn is_stream_continuation(&self, returns: &Type, params: &[Type]) -> bool {
        matches!(returns, Type::Void) && matches!(params, [Type::Uint64, Type::StreamPollResult])
    }

    fn passable(
        &self,
        names: &BTreeMap<String, String>,
        c_name: &str,
        position: SignaturePosition,
    ) -> TokenStream {
        let rust_name = names
            .get(c_name)
            .unwrap_or_else(|| panic!("{c_name} is not mapped to a Rust passable type"));
        let ty = format_ident!("{}", rust_name);
        match position {
            SignaturePosition::Input => {
                quote! { <crate::#ty as ::boltffi::__private::Passable>::In }
            }
            SignaturePosition::Output => {
                quote! { <crate::#ty as ::boltffi::__private::Passable>::Out }
            }
        }
    }

    fn direct_records(
        bindings: &Bindings<Native>,
        contract: &CBridgeContract,
    ) -> BTreeMap<String, String> {
        bindings
            .decls()
            .iter()
            .filter_map(|decl| match decl {
                Decl::Record(record) => match record.as_ref() {
                    RecordDecl::Direct(record) => {
                        contract.source_direct_record(record.id()).map(|c_record| {
                            (c_record.name().to_owned(), rust_type_name(record.name()))
                        })
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    fn c_style_enums(
        bindings: &Bindings<Native>,
        contract: &CBridgeContract,
    ) -> BTreeMap<String, String> {
        bindings
            .decls()
            .iter()
            .filter_map(|decl| match decl {
                Decl::Enum(enumeration) => match enumeration.as_ref() {
                    EnumDecl::CStyle(enumeration) => contract
                        .source_c_style_enum(enumeration.id())
                        .map(|c_enum| {
                            (c_enum.name().to_owned(), rust_type_name(enumeration.name()))
                        }),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    fn callback_vtables(contract: &CBridgeContract) -> BTreeMap<String, String> {
        contract
            .callbacks()
            .iter()
            .map(|callback| {
                (
                    callback.vtable().name().to_owned(),
                    format!("{}VTable", rust_type_name(callback.name())),
                )
            })
            .collect()
    }
}

trait PublishedFunctions {
    fn published_functions(&self) -> Box<dyn Iterator<Item = &Function> + '_>;
}

impl PublishedFunctions for CBridgeContract {
    fn published_functions(&self) -> Box<dyn Iterator<Item = &Function> + '_> {
        Box::new(
            self.functions().iter().chain(
                self.callbacks()
                    .iter()
                    .flat_map(|callback| [callback.register(), callback.create_handle()]),
            ),
        )
    }
}

impl FillBytesHarness {
    fn from_contract(contract: &CBridgeContract) -> Self {
        let function = contract
            .functions()
            .iter()
            .find(|function| function.name() == "boltffi_function_boltffi_tests_bytes_fill_bytes")
            .expect("fill_bytes C function");
        match (function.params(), function.returns()) {
            ([first, second, third], Type::Uint32)
                if matches!(first.ty(), Type::ConstPointer(pointee) if matches!(pointee.as_ref(), Type::Uint8))
                    && matches!(second.ty(), Type::PointerWidth)
                    && matches!(third.ty(), Type::MutPointer(pointee) if matches!(pointee.as_ref(), Type::Buffer)) =>
            {
                Self::EncodedWriteback
            }
            ([first, second], Type::Uint32)
                if matches!(first.ty(), Type::MutPointer(pointee) if matches!(pointee.as_ref(), Type::Uint8))
                    && matches!(second.ty(), Type::PointerWidth) =>
            {
                Self::MutableBytes
            }
            _ => panic!("unexpected fill_bytes C signature: {function:?}"),
        }
    }
}

impl std::fmt::Display for FillBytesHarness {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::EncodedWriteback => {
                r#"static int boltffi_tests_check_fill_bytes(void) {
    uint8_t input[10] = {6, 0, 0, 0, 0, 0, 0, 0, 0, 0};
    FfiBuf_u8 out = {0};
    uint32_t written = boltffi_function_boltffi_tests_bytes_fill_bytes(input, 10, &out);
    if (written != 6) {
        return 41;
    }
    if (out.len != 10) {
        boltffi_free_buf(out);
        return 42;
    }
    if (out.ptr[0] != 6 || out.ptr[1] != 0 || out.ptr[2] != 0 || out.ptr[3] != 0) {
        boltffi_free_buf(out);
        return 43;
    }
    uintptr_t index = 0;
    while (index < 6) {
        uint8_t expected = (uint8_t)(index * 3 + 1);
        if (out.ptr[index + 4] != expected) {
            boltffi_free_buf(out);
            return 44;
        }
        index += 1;
    }
    boltffi_free_buf(out);
    return 0;
}
"#
            }
            Self::MutableBytes => {
                r#"static int boltffi_tests_check_fill_bytes(void) {
    uint8_t buffer[6] = {0, 0, 0, 0, 0, 0};
    uint32_t written = boltffi_function_boltffi_tests_bytes_fill_bytes(buffer, 6);
    if (written != 6) {
        return 44;
    }
    uintptr_t index = 0;
    while (index < 6) {
        uint8_t expected = (uint8_t)(index * 3 + 1);
        if (buffer[index] != expected) {
            return 45;
        }
        index += 1;
    }
    return 0;
}
"#
            }
        })
    }
}

fn source_contract(paths: &BuildPaths) -> SourceContract {
    let package = PackageInfo::new(
        env::var("CARGO_PKG_NAME").expect("package name"),
        env::var("CARGO_PKG_VERSION").ok(),
    );
    scan_package(&ScanInput::new(&paths.source, package).with_manifest_dir(&paths.manifest))
        .expect("scan test package")
        .root_with_support()
}

fn rust_type_name(name: &boltffi_binding::CanonicalName) -> String {
    name.parts()
        .iter()
        .map(|part| {
            let mut characters = part.as_str().chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(characters).collect::<String>()
            })
        })
        .collect()
}
