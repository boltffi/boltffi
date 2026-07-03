//! Ruby target host backend.

mod name_style;
mod render;
mod syntax;

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    Native, RecordDecl, StreamDecl,
};

use crate::{
    bridge::c::CBridgeContract,
    core::{
        BindingCapability, BridgeCapability, CapabilityRequirements, Emitted, Error,
        GeneratedOutput, HostCapabilities, RenderContext, RenderedDeclaration, Result,
        contract::sealed, host,
    },
};

/// Ruby host renderer for a native C extension linked against the C ABI bridge.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct RubyCExtHost {
    ractor_safe: bool,
}

impl RubyCExtHost {
    /// Creates a Ruby C extension host renderer.
    pub const fn new() -> Self {
        Self { ractor_safe: false }
    }

    /// Declares the extension Ractor-safe, emitting `rb_ext_ractor_safe(true)`
    /// in `Init_`. Opt-in: it asserts the bound crate has no unsynchronized
    /// global mutable state, which BoltFFI cannot verify.
    pub const fn ractor_safe(mut self, ractor_safe: bool) -> Self {
        self.ractor_safe = ractor_safe;
        self
    }
}

impl host::HostBackend for RubyCExtHost {
    type Surface = Native;
    type Bridge = CBridgeContract;
    type Syntax = syntax::Syntax;

    fn name(&self) -> &'static str {
        "ruby"
    }

    fn binding_capabilities(&self) -> HostCapabilities {
        HostCapabilities::new()
            .stable(BindingCapability::Records)
            .stable(BindingCapability::Enums)
            .stable(BindingCapability::Functions)
            .stable(BindingCapability::Classes)
            .unsupported(
                BindingCapability::Callbacks,
                "Ruby callbacks are not migrated yet",
            )
            .unsupported(
                BindingCapability::Streams,
                "Ruby streams are not migrated yet",
            )
            .unsupported(
                BindingCapability::Constants,
                "Ruby constants are not migrated yet",
            )
            .unsupported(
                BindingCapability::CustomTypes,
                "Ruby custom types are not migrated yet",
            )
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        CapabilityRequirements::new().require(BridgeCapability::CAbi)
    }

    fn record(
        &self,
        decl: &RecordDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Record::from_declaration(decl, bridge, context)?.render()
    }

    fn enumeration(
        &self,
        decl: &EnumDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        match decl {
            EnumDecl::CStyle(_) => Ok(Emitted::primary(String::new())),
            EnumDecl::Data(_) | _ => Err(Error::UnsupportedTarget {
                target: self.name(),
                shape: "data enum",
            }),
        }
    }

    fn function(
        &self,
        decl: &FunctionDecl<Self::Surface>,
        bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Function::from_declaration(decl, bridge, _context)?.render()
    }

    fn class(
        &self,
        _decl: &ClassDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(Emitted::primary(String::new()))
    }

    fn callback(
        &self,
        _decl: &CallbackDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: self.name(),
            shape: "callback",
        })
    }

    fn stream(
        &self,
        _decl: &StreamDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: self.name(),
            shape: "stream",
        })
    }

    fn constant(
        &self,
        _decl: &ConstantDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: self.name(),
            shape: "constant",
        })
    }

    fn custom_type(
        &self,
        _decl: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: self.name(),
            shape: "custom type",
        })
    }

    fn assemble(
        &self,
        bindings: &Bindings<Self::Surface>,
        bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
        declarations: Vec<RenderedDeclaration<'_, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        render::NativeExtension::new(bindings, bridge, declarations, self.ractor_safe).render()
    }
}

impl sealed::HostBackend for RubyCExtHost {}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Bindings, Native, lower};

    use crate::{Error, bridge::c::CBridge, core::Target, target::ruby::RubyCExtHost};

    fn bindings(source: &str) -> Bindings<Native> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(source).expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source should scan");
        lower::<Native>(&source).expect("source should lower")
    }

    fn bindings_with_interned_field(
        source: &str,
        field_name: &str,
        static_values: &[&str],
    ) -> Bindings<Native> {
        let mut value = serde_json::to_value(bindings(source)).expect("bindings serialize");
        let mut replaced = false;
        replace_interned_field(&mut value, field_name, static_values, &mut replaced);
        assert!(
            replaced,
            "expected to rewrite field {field_name} as InternedString"
        );
        serde_json::from_value(value).expect("interned fixture should deserialize")
    }

    fn replace_interned_field(
        value: &mut serde_json::Value,
        field_name: &str,
        static_values: &[&str],
        replaced: &mut bool,
    ) {
        match value {
            serde_json::Value::Object(object) => {
                if let Some(fields) = object
                    .get_mut("fields")
                    .and_then(serde_json::Value::as_array_mut)
                {
                    for field in fields {
                        if field_matches_name(field, field_name) {
                            rewrite_string_type(
                                field.get_mut("ty").expect("encoded field has type"),
                                static_values,
                            );
                            *replaced = true;
                        }
                    }
                }
                for child in object.values_mut() {
                    replace_interned_field(child, field_name, static_values, replaced);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    replace_interned_field(item, field_name, static_values, replaced);
                }
            }
            _ => {}
        }
    }

    fn field_matches_name(field: &serde_json::Value, field_name: &str) -> bool {
        field
            .get("key")
            .and_then(|key| key.get("Named"))
            .and_then(|named| named.get("parts"))
            .and_then(serde_json::Value::as_array)
            .is_some_and(|parts| {
                parts.len() == 1
                    && parts.first().and_then(serde_json::Value::as_str) == Some(field_name)
            })
    }

    fn rewrite_string_type(ty: &mut serde_json::Value, static_values: &[&str]) {
        let interned = serde_json::json!({
            "InternedString": { "static_values": static_values }
        });
        if *ty == serde_json::Value::String("String".to_owned()) {
            *ty = interned;
            return;
        }
        if let Some(inner) = ty.get_mut("Optional") {
            assert_eq!(*inner, serde_json::Value::String("String".to_owned()));
            *inner = interned;
            return;
        }
        panic!("expected String or Option<String> field, got {ty}");
    }

    fn target() -> Target<RubyCExtHost, CBridge> {
        Target::new(
            RubyCExtHost::new(),
            CBridge::new("ext/demo/boltffi.h").expect("C header bridge"),
        )
    }

    fn extension(output: &crate::GeneratedOutput) -> &str {
        output
            .files()
            .iter()
            .find(|file| file.path().as_path() == Path::new("ext/demo/_native.c"))
            .map(|file| file.contents())
            .expect("Ruby extension file")
    }

    #[test]
    fn ruby_target_completes_native_extension_package() {
        let output = target()
            .render(&bindings(""))
            .expect("Ruby target should render");
        let paths = output
            .files()
            .iter()
            .map(|file| file.path().as_path().to_owned())
            .collect::<Vec<_>>();

        assert!(paths.contains(&Path::new("ext/demo/boltffi.h").to_owned()));
        assert!(paths.contains(&Path::new("ext/demo/_native.c").to_owned()));
        assert!(paths.contains(&Path::new("ext/demo/extconf.rb").to_owned()));
        assert!(paths.contains(&Path::new("lib/demo.rb").to_owned()));
        assert!(paths.contains(&Path::new("demo.gemspec").to_owned()));
        let lib = output
            .files()
            .iter()
            .find(|file| file.path().as_path() == Path::new("lib/demo.rb"))
            .map(|file| file.contents())
            .expect("Ruby library file");
        assert!(lib.contains("require \"demo_native\""));
        assert!(!lib.contains("require_relative"));
    }

    #[test]
    fn ruby_target_renders_primitive_function_wrapper() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 { left + right }
                "#,
            ))
            .expect("Ruby target should render");
        let extension = extension(&output);

        assert!(extension.contains("#include \"boltffi.h\""));
        // Fixed-arity wrapper: each argument is a named `VALUE argN` parameter,
        // so the VM/YJIT pass them in registers and enforce the count.
        assert!(extension.contains(
            "static VALUE boltffi_ruby_boltffi_function_demo_add(VALUE self, VALUE arg0, VALUE arg1)"
        ));
        assert!(extension.contains("int32_t left = NUM2INT(arg0);"));
        assert!(extension.contains("int32_t right = NUM2INT(arg1);"));
        assert!(extension.contains("int32_t _ffi_ret = boltffi_function_demo_add(left, right);"));
        assert!(extension.contains("return LL2NUM((long long)_ffi_ret);"));
        // Fixed arity lets the VM enforce the count, so no manual check is emitted.
        assert!(!extension.contains("if (argc != 2)"));
        assert!(!extension.contains("int argc, VALUE *argv"));
        assert!(extension.contains("void Init_demo_native(void)"));
        assert!(extension.contains("VALUE mod = rb_define_module(\"Demo\");"));
        let lib = output
            .files()
            .iter()
            .find(|file| file.path().as_path() == Path::new("lib/demo.rb"))
            .map(|file| file.contents())
            .expect("Ruby library file");
        assert!(lib.contains("def add(...)"));
        assert!(lib.contains("Native.add(...)"));
        assert!(extension.contains(
            "rb_define_module_function(native, \"add\", boltffi_ruby_boltffi_function_demo_add, 2);"
        ));
        assert!(!extension.contains("static inline int boltffi_ruby_bool"));
        assert!(!extension.contains("static VALUE boltffi_ruby_decode_string"));
        assert!(!extension.contains("static VALUE boltffi_ruby_decode_bytes"));
    }

    #[test]
    fn ruby_target_falls_back_to_variadic_above_fixed_arity_cap() {
        // Ruby's VM caps fixed C-function arity at 15 (`rb_add_method_cfunc`);
        // wider argument lists keep the variadic `(argc, argv, self)` convention.
        let params = (0..16)
            .map(|index| format!("a{index}: i32"))
            .collect::<Vec<_>>()
            .join(", ");
        let source = format!("#[export]\npub fn many({params}) -> i32 {{ 0 }}");
        let output = target()
            .render(&bindings(&source))
            .expect("Ruby target should render");
        let extension = extension(&output);

        assert!(extension.contains(
            "static VALUE boltffi_ruby_boltffi_function_demo_many(int argc, VALUE *argv, VALUE self)"
        ));
        assert!(extension.contains("if (argc != 16)"));
        assert!(extension.contains("int32_t a0 = NUM2INT(argv[0]);"));
        assert!(extension.contains("int32_t a15 = NUM2INT(argv[15]);"));
        assert!(extension.contains(
            "rb_define_module_function(native, \"many\", boltffi_ruby_boltffi_function_demo_many, -1);"
        ));
    }

    #[test]
    fn ruby_target_boxes_64_bit_returns_without_pointer_width_truncation() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn wide_signed() -> i64 { i64::MAX }
                #[export]
                pub fn wide_unsigned() -> u64 { u64::MAX }
                "#,
            ))
            .expect("Ruby target should render");
        let extension = extension(&output);

        assert!(extension.contains("return LL2NUM((long long)_ffi_ret);"));
        assert!(extension.contains("return ULL2NUM((unsigned long long)_ffi_ret);"));
        assert!(!extension.contains("rb_int2inum((intptr_t)_ffi_ret)"));
        assert!(!extension.contains("rb_uint2inum((uintptr_t)_ffi_ret)"));
    }

    #[test]
    fn ruby_target_accepts_borrowed_string_parameter() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn len(value: &str) -> u32 { value.len() as u32 }
                "#,
            ))
            .expect("Ruby target should render");
        let extension = extension(&output);

        assert!(extension.contains("uintptr_t value_raw_len = (uintptr_t)RSTRING_LEN(arg0);"));
        assert!(extension.contains("uintptr_t value_len = value_raw_len + sizeof(uint32_t);"));
        assert!(extension.contains("VALUE value_tmp;"));
        assert!(extension.contains(
            "uint8_t *value_ptr = (uint8_t *)RB_ALLOCV_N(uint8_t, value_tmp, value_len);"
        ));
        assert!(
            extension
                .contains("uint32_t _ffi_ret = boltffi_function_demo_len(value_ptr, value_len);")
        );
        // RB_ALLOCV buffer is released after the call and on any unwind.
        assert!(extension.contains("RB_ALLOCV_END(value_tmp);"));
        assert!(!extension.contains("ruby_xmalloc("));
        assert!(!extension.contains("ruby_xfree("));
        assert!(!extension.contains("alloca("));
    }

    #[test]
    fn ruby_target_emits_only_required_helpers() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn enabled(flag: bool) -> bool { flag }
                #[export]
                pub fn label() -> String { String::from("ok") }
                #[export]
                pub fn data() -> Vec<u8> { vec![1, 2] }
                "#,
            ))
            .expect("Ruby target should render");
        let extension = extension(&output);

        assert!(extension.contains("static inline int boltffi_ruby_bool"));
        assert!(extension.contains("static VALUE boltffi_ruby_decode_string"));
        assert!(extension.contains("static VALUE boltffi_ruby_decode_bytes"));
        assert!(extension.contains("buffer.len < sizeof(uint32_t)"));
        assert!(extension.contains("(uintptr_t)wire_len > buffer.len - sizeof(uint32_t)"));
    }

    #[test]
    fn ruby_target_renders_encoded_record_returning_function() {
        let output = target()
            .render(&bindings(
                r#"
                #[data]
                pub struct Sniffed {
                    pub name: String,
                    pub major: u32,
                }

                #[export]
                pub fn sniff(ua: &str) -> Sniffed {
                    Sniffed { name: ua.to_owned(), major: 0 }
                }
                "#,
            ))
            .expect("Ruby target should render encoded record return");
        let extension = extension(&output);

        assert!(extension.contains("typedef struct {\n    char *name_ptr;\n    uintptr_t name_len;\n    uint32_t major;\n} boltffi_ruby_sniffed;"));
        assert!(extension.contains("static const rb_data_type_t boltffi_ruby_sniffed_type"));
        assert!(extension.contains("RUBY_TYPED_EMBEDDABLE"));
        assert!(extension.contains("TypedData_Make_Struct(c_sniffed, boltffi_ruby_sniffed"));
        assert!(extension.contains("rb_undef_alloc_func(c_sniffed);"));
        assert!(extension.contains("FfiBuf_u8 _ffi_ret = boltffi_function_demo_sniff("));
        assert!(extension.contains("return boltffi_ruby_decode_sniffed(_ffi_ret);"));
        assert!(extension.contains("rb_define_class_under(mod, \"Sniffed\", rb_cObject);"));
        assert!(
            extension
                .contains("rb_define_method(c_sniffed, \"name\", boltffi_ruby_sniffed_name, 0);")
        );
        assert!(
            extension
                .contains("rb_define_method(c_sniffed, \"major\", boltffi_ruby_sniffed_major, 0);")
        );
    }

    #[test]
    fn ruby_target_renders_optional_encoded_record_fields() {
        let output = target()
            .render(&bindings(
                r#"
                #[data]
                pub struct Sniffed {
                    pub name: Option<String>,
                    pub major: Option<u32>,
                    pub mobile: Option<bool>,
                    pub bytes: Option<Vec<u8>>,
                }

                #[export]
                pub fn sniff() -> Sniffed {
                    Sniffed { name: None, major: Some(120), mobile: Some(false), bytes: None }
                }
                "#,
            ))
            .expect("Ruby target should render optional encoded record fields");
        let extension = extension(&output);

        assert!(extension.contains("int name_present;"));
        assert!(extension.contains("int major_present;"));
        assert!(extension.contains("int mobile_present;"));
        assert!(extension.contains("int bytes_present;"));
        assert!(extension.contains(
            "if (!boltffi_ruby_wire_read_option_tag(&reader, &data->name_present)) goto fail;"
        ));
        assert!(extension.contains(
            "if (data->name_present) { if (!boltffi_ruby_wire_read_string(&reader, &data->name_ptr, &data->name_len)) goto fail; }"
        ));
        assert!(extension.contains(
            "if (data->major_present) { if (!boltffi_ruby_wire_read_u32(&reader, &data->major)) goto fail; }"
        ));
        assert!(extension.contains("if (!data->name_present) return Qnil;"));
        assert!(extension.contains("if (!data->major_present) return Qnil;"));
        assert!(extension.contains("if (!data->mobile_present) return Qnil;"));
        assert!(extension.contains("if (!data->bytes_present) return Qnil;"));
    }

    #[test]
    fn ruby_target_renders_interned_string_encoded_record_field() {
        let output = target()
            .render(&bindings(
                r#"
                boltffi::interned_string_pool! {
                    pub BrowserName {
                        Ok = "ok",
                        Cached = "cached",
                    }
                }

                #[data]
                pub struct Sniffed {
                    pub label: boltffi::InternedString<BrowserName>,
                    pub major: u32,
                }

                #[export]
                pub fn sniff() -> Sniffed {
                    Sniffed { label: BrowserName::OK, major: 1 }
                }
                "#,
            ))
            .expect("Ruby target should render interned string encoded record field");
        let extension = extension(&output);

        assert!(extension.contains("static VALUE boltffi_ruby_sniffed_label_interned_values[2];"));
        let register = extension
            .find("rb_global_variable(&boltffi_ruby_sniffed_label_interned_values[0]);")
            .expect("interned slot registered with Ruby GC");
        let initialize = extension
            .find("boltffi_ruby_sniffed_label_interned_values[0] = rb_utf8_str_new_static(\"ok\", sizeof(\"ok\") - 1);")
            .expect("interned slot initialized with static UTF-8 literal");
        assert!(register < initialize);
        assert!(
            extension.contains("rb_obj_freeze(boltffi_ruby_sniffed_label_interned_values[0]);")
        );
        assert!(
            extension
                .contains("rb_global_variable(&boltffi_ruby_sniffed_label_interned_values[1]);")
        );
        assert!(extension.contains(
            "boltffi_ruby_sniffed_label_interned_values[1] = rb_utf8_str_new_static(\"cached\", sizeof(\"cached\") - 1);"
        ));
        assert!(
            extension.contains("rb_obj_freeze(boltffi_ruby_sniffed_label_interned_values[1]);")
        );
        assert!(extension.contains("if (!boltffi_ruby_wire_read_u8(reader, &tag)) return 0;"));
        assert!(extension.contains("if (!boltffi_ruby_wire_read_u32(reader, &id)) return 0;"));
        assert!(extension.contains(
            "if (!boltffi_ruby_wire_read_string(reader, &data->label_ptr, &data->label_len)) return 0;"
        ));
        assert!(extension.contains("if (id >= 2) return 3;"));
        assert!(extension.contains("return 2;"));
        assert!(extension.contains("native function returned invalid interned string tag"));
        assert!(extension.contains("native function returned invalid interned string id"));
        assert!(
            extension
                .contains("result = boltffi_ruby_sniffed_label_interned_values[data->label_id];")
        );
        assert!(extension.contains("rb_obj_freeze(result);"));
        assert!(!extension.contains("FfiString"));
    }

    #[test]
    fn ruby_target_escapes_interned_static_string_literals() {
        let output = target()
            .render(&bindings_with_interned_field(
                r#"
                #[data]
                pub struct Sniffed {
                    pub label: String,
                }

                #[export]
                pub fn sniff() -> Sniffed {
                    Sniffed { label: String::from("ok") }
                }
                "#,
                "label",
                &["Firefox\nβ", "quote\"slash\\question?"],
            ))
            .expect("Ruby target should render escaped interned static values");
        let extension = extension(&output);

        assert!(extension.contains(
            "rb_utf8_str_new_static(\"Firefox\\n\\316\\262\", sizeof(\"Firefox\\n\\316\\262\") - 1);"
        ));
        assert!(extension.contains(
            "rb_utf8_str_new_static(\"quote\\\"slash\\\\question\\?\", sizeof(\"quote\\\"slash\\\\question\\?\") - 1);"
        ));
    }

    #[test]
    fn ruby_target_rejects_invalid_interned_static_values() {
        assert_interned_static_values_rejected(&[], "empty interned string pool");
        assert_interned_static_values_rejected(
            &["dup", "dup"],
            "duplicate interned string pool value",
        );
        assert_interned_static_values_rejected(
            &["nul\0value"],
            "interned string pool value containing NUL",
        );
    }

    fn assert_interned_static_values_rejected(static_values: &[&str], expected_shape: &str) {
        let err = target()
            .render(&bindings_with_interned_field(
                r#"
                #[data]
                pub struct Sniffed {
                    pub label: String,
                }

                #[export]
                pub fn sniff() -> Sniffed {
                    Sniffed { label: String::from("ok") }
                }
                "#,
                "label",
                static_values,
            ))
            .expect_err("invalid interned static values should be rejected");

        assert!(
            matches!(
                err,
                Error::UnsupportedTarget {
                    target: "ruby",
                    shape
                } if shape == expected_shape
            ),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn ruby_target_wraps_interned_string_field_with_existing_option_tag() {
        let output = target()
            .render(&bindings_with_interned_field(
                r#"
                #[data]
                pub struct Sniffed {
                    pub label: Option<String>,
                }

                #[export]
                pub fn sniff() -> Sniffed {
                    Sniffed { label: None }
                }
                "#,
                "label",
                &["known"],
            ))
            .expect("Ruby target should render optional interned string encoded record field");
        let extension = extension(&output);

        let option = extension
            .find("boltffi_ruby_wire_read_option_tag(&reader, &data->label_present)")
            .expect("option tag read");
        let interned = extension
            .find("int label_status = boltffi_ruby_sniffed_label_read_interned(&reader, data)")
            .expect("interned tag read after option tag");
        assert!(option < interned);
        assert!(extension.contains("if (!data->label_present) return Qnil;"));
        assert!(extension.contains("if (data->label_present) { int label_status = boltffi_ruby_sniffed_label_read_interned(&reader, data);"));
    }

    #[test]
    fn ruby_target_renders_direct_record_returning_function() {
        let output = target()
            .render(&bindings(
                r#"
                #[repr(C)]
                #[data]
                pub struct Point {
                    pub x: f64,
                    pub y: f64,
                }

                #[export]
                pub fn make_point(x: f64, y: f64) -> Point {
                    Point { x, y }
                }
                "#,
            ))
            .expect("Ruby target should render direct record return");
        let extension = extension(&output);

        assert!(extension.contains("static const rb_data_type_t boltffi_ruby_point_type"));
        assert!(extension.contains("RUBY_TYPED_DEFAULT_FREE"));
        assert!(extension.contains("BOLTFFI_RUBY_HAS_TYPED_EMBEDDABLE"));
        assert!(extension.contains("static VALUE boltffi_ruby_box_point(___Point value)"));
        assert!(extension.contains("___Point _ffi_ret = boltffi_function_demo_make_point(x, y);"));
        assert!(extension.contains("return boltffi_ruby_box_point(_ffi_ret);"));
        assert!(extension.contains("rb_define_class_under(mod, \"Point\", rb_cObject);"));
        assert!(extension.contains("rb_define_method(c_point, \"x\", boltffi_ruby_point_x, 0);"));
        assert!(extension.contains("rb_define_method(c_point, \"y\", boltffi_ruby_point_y, 0);"));
    }

    #[test]
    fn ruby_target_renders_direct_record_borrowed_parameters() {
        let output = target()
            .render(&bindings(
                r#"
                #[repr(C)]
                #[data]
                pub struct Point {
                    pub x: f64,
                    pub y: f64,
                }

                #[export]
                pub fn distance(point: &Point) -> f64 {
                    0.0
                }

                #[export]
                pub fn scale(point: &mut Point, factor: f64) {
                    point.x *= factor;
                }
                "#,
            ))
            .expect("Ruby target should render borrowed direct record params");
        let extension = extension(&output);

        assert!(extension.contains("VALUE point_value = arg0;"));
        assert!(extension.contains("___Point *point = NULL;"));
        assert!(extension.contains(
            "TypedData_Get_Struct(point_value, ___Point, &boltffi_ruby_point_type, point);"
        ));
        assert!(extension.contains("double _ffi_ret = boltffi_function_demo_distance(point);"));
        assert!(extension.contains("boltffi_function_demo_scale(point, factor);"));
        assert!(extension.contains("RB_GC_GUARD(point_value);"));
        assert!(!extension.contains("FfiBuf_u8 point"));
        assert!(!extension.contains("point_out"));
    }

    #[test]
    fn ruby_target_renders_direct_record_by_value_parameter() {
        let output = target()
            .render(&bindings(
                r#"
                #[repr(C)]
                #[data]
                pub struct Point {
                    pub x: f64,
                    pub y: f64,
                }

                #[export]
                pub fn echo(point: Point) -> Point {
                    point
                }
                "#,
            ))
            .expect("Ruby target should render by-value direct record param");
        let extension = extension(&output);

        assert!(extension.contains("___Point point = boltffi_ruby_unwrap_point(arg0);"));
        assert!(extension.contains("___Point _ffi_ret = boltffi_function_demo_echo(point);"));
    }

    #[test]
    fn ruby_target_rejects_mutable_bytes_parameter() {
        // Mutable byte slices need a borrowed write-back ABI that does not exist
        // yet; until then `&mut [u8]` is unsupported rather than silently copied.
        let result = target().render(&bindings(
            r#"
                #[export]
                pub fn fill(out: &mut [u8]) -> u32 { out.len() as u32 }
                "#,
        ));

        assert!(
            matches!(
                result,
                Err(crate::core::Error::UnsupportedTarget { target: "ruby", .. })
            ),
            "expected &mut [u8] to be unsupported, got {result:?}"
        );
    }

    #[test]
    fn ruby_target_range_checks_narrow_integer_parameters() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn clamp(value: u8) -> u8 { value }
                "#,
            ))
            .expect("Ruby target should render");
        let extension = extension(&output);

        assert!(extension.contains("unsigned long long value_wide = NUM2ULL(arg0);"));
        assert!(extension.contains(
            "if (value_wide > UINT8_MAX) rb_raise(rb_eRangeError, \"value out of range for u8\");"
        ));
        assert!(extension.contains("uint8_t value = (uint8_t)value_wide;"));
    }

    #[test]
    fn ruby_target_emits_ractor_safe_only_when_enabled() {
        let bindings = bindings(
            r#"
            #[export]
            pub fn noop() {}
            "#,
        );
        let default = Target::new(
            RubyCExtHost::new(),
            CBridge::new("ext/demo/boltffi.h").expect("C header bridge"),
        )
        .render(&bindings)
        .expect("Ruby target should render");
        assert!(!extension(&default).contains("rb_ext_ractor_safe(true);"));

        let safe = Target::new(
            RubyCExtHost::new().ractor_safe(true),
            CBridge::new("ext/demo/boltffi.h").expect("C header bridge"),
        )
        .render(&bindings)
        .expect("Ruby target should render");
        assert!(extension(&safe).contains("rb_ext_ractor_safe(true);"));
    }
}
