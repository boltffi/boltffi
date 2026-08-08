use std::collections::HashSet;

use boltffi_binding::{
    DirectRecordDecl, EncodedRecordDecl, FieldKey, Native, Primitive, RecordDecl, RecordId, TypeRef,
};

use crate::{
    bridge::c::CBridgeContract,
    core::{Emitted, Error, RenderContext, Result},
    target::ruby::{
        name_style::Name,
        render::function::{primitive_box, primitive_c_type},
    },
};

pub struct Record {
    body: String,
}

pub fn encoded_record_supported(record: &EncodedRecordDecl<Native>) -> bool {
    record.fields().iter().all(|field| {
        matches!(field.key(), FieldKey::Named(_)) && encoded_field_supported(field.ty())
    })
}

fn encoded_field_supported(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::String | TypeRef::Bytes | TypeRef::InternedString { .. } => true,
        TypeRef::Primitive(primitive) => wire_read_fn(*primitive).is_ok(),
        TypeRef::Optional(inner) => {
            !matches!(inner.as_ref(), TypeRef::Optional(_)) && encoded_field_supported(inner)
        }
        _ => false,
    }
}

#[derive(Clone, Debug)]
pub struct Symbols {
    pub class_var: String,
    pub class_name: String,
    pub ident: String,
    pub c_type: Option<String>,
    pub data_type: String,
    pub boxer: String,
    pub decoder: Option<String>,
    pub unwrapper: String,
}

impl Record {
    pub fn from_declaration(
        declaration: &RecordDecl<Native>,
        bridge: &CBridgeContract,
        _context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = Symbols::from_decl(declaration, bridge)?;
        let body = match declaration {
            RecordDecl::Direct(record) => render_direct(record, bridge, &symbols)?,
            RecordDecl::Encoded(record) => {
                if record.is_native_opaque() {
                    if !native_opaque_record_supported(record) {
                        return Err(Error::UnsupportedTarget {
                            target: "ruby",
                            shape: "unsupported native opaque record field",
                        });
                    }
                    render_native_opaque(record, &symbols)?
                } else {
                    if !encoded_record_supported(record) {
                        return Err(Error::UnsupportedTarget {
                            target: "ruby",
                            shape: "unsupported encoded record field",
                        });
                    }
                    render_encoded(record, &symbols)?
                }
            }
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "ruby",
                    shape: "unknown record",
                });
            }
        };
        Ok(Self { body })
    }

    pub fn render(self) -> Result<Emitted> {
        Ok(Emitted::primary(self.body))
    }
}

impl Symbols {
    pub fn from_record_id(
        record_id: RecordId,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let record = context.record(record_id).ok_or(Error::UnsupportedTarget {
            target: "ruby",
            shape: "record id without declaration",
        })?;
        Self::from_decl(record, bridge)
    }

    fn from_decl(record: &RecordDecl<Native>, bridge: &CBridgeContract) -> Result<Self> {
        let class_name = Name::new(record.name()).class();
        let ident = Name::new(record.name()).function();
        let c_type = match record {
            RecordDecl::Direct(record) => Some(
                bridge
                    .source_direct_record(record.id())
                    .ok_or(Error::UnsupportedTarget {
                        target: "ruby",
                        shape: "direct record without C typedef",
                    })?
                    .name()
                    .to_owned(),
            ),
            RecordDecl::Encoded(_) => Some(format!("boltffi_ruby_{ident}")),
            _ => None,
        };
        Ok(Self {
            class_var: format!("c_{ident}"),
            class_name,
            ident: ident.clone(),
            c_type,
            data_type: format!("boltffi_ruby_{ident}_type"),
            boxer: format!("boltffi_ruby_box_{ident}"),
            decoder: match record {
                RecordDecl::Encoded(_) => Some(format!("boltffi_ruby_decode_{ident}")),
                _ => None,
            },
            unwrapper: format!("boltffi_ruby_unwrap_{ident}"),
        })
    }
}

fn render_direct(
    record: &DirectRecordDecl<Native>,
    bridge: &CBridgeContract,
    symbols: &Symbols,
) -> Result<String> {
    let c_record = bridge
        .source_direct_record(record.id())
        .ok_or(Error::UnsupportedTarget {
            target: "ruby",
            shape: "direct record without C typedef",
        })?;
    let c_type = c_record.name();
    let ident = &symbols.ident;
    let mut out = String::new();
    out.push_str(&format!(
        "/* boltffi:ruby:record class={} var={} */\n",
        symbols.class_name, symbols.class_var
    ));
    out.push_str(&format!("static VALUE {};\n", symbols.class_var));
    out.push_str(&format!(
        "static size_t boltffi_ruby_{ident}_size(const void *ptr) {{\n    (void)ptr;\n#if BOLTFFI_RUBY_HAS_TYPED_EMBEDDABLE\n    return 0;\n#else\n    return sizeof({c_type});\n#endif\n}}\n"
    ));
    out.push_str(&format!(
        "static const rb_data_type_t {} = {{\n    \"boltffi.ruby.generated.{}\", {{ NULL, RUBY_TYPED_DEFAULT_FREE, boltffi_ruby_{ident}_size, NULL }}, NULL, NULL, RUBY_TYPED_FREE_IMMEDIATELY | BOLTFFI_RUBY_TYPED_EMBEDDABLE\n}};\n",
        symbols.data_type, symbols.class_name
    ));
    out.push_str(&format!(
        "static VALUE {}({c_type} value) {{\n    {c_type} *data;\n    VALUE obj = TypedData_Make_Struct({}, {c_type}, &{}, data);\n    *data = value;\n    return obj;\n}}\n",
        symbols.boxer, symbols.class_var, symbols.data_type
    ));
    out.push_str(&format!(
        "static {c_type} {}(VALUE value) {{\n    {c_type} *data;\n    TypedData_Get_Struct(value, {c_type}, &{}, data);\n    {c_type} copy = *data;\n    RB_GC_GUARD(value);\n    return copy;\n}}\n",
        symbols.unwrapper, symbols.data_type
    ));
    for (source, c_field) in record.fields().iter().zip(c_record.fields()) {
        let field_name = field_name(source.key())?;
        let primitive = source.ty().primitive();
        let expr =
            primitive_box(primitive).replace("_ffi_ret", &format!("data->{}", c_field.name()));
        out.push_str(&format!(
            "static VALUE boltffi_ruby_{ident}_{field_name}(VALUE self) {{\n    {c_type} *data;\n    TypedData_Get_Struct(self, {c_type}, &{}, data);\n    VALUE result = {expr};\n    RB_GC_GUARD(self);\n    return result;\n}}\n",
            symbols.data_type
        ));
    }
    out.push_str(&format!(
        "/* boltffi:ruby:record:init {} */\n",
        symbols.ident
    ));
    out.push_str(&format!(
        "    {} = rb_define_class_under(mod, \"{}\", rb_cObject);\n    rb_undef_alloc_func({});\n",
        symbols.class_var, symbols.class_name, symbols.class_var
    ));
    for (source, _) in record.fields().iter().zip(c_record.fields()) {
        let field_name = field_name(source.key())?;
        out.push_str(&format!(
            "    rb_define_method({}, \"{field_name}\", boltffi_ruby_{ident}_{field_name}, 0);\n",
            symbols.class_var
        ));
    }
    out.push_str("/* boltffi:ruby:record:init:end */\n");
    Ok(out)
}

fn render_encoded(record: &EncodedRecordDecl<Native>, symbols: &Symbols) -> Result<String> {
    let ident = &symbols.ident;
    let c_type = symbols.c_type.as_deref().expect("encoded c type");
    let mut field_defs = Vec::new();
    let mut support_defs = Vec::new();
    let mut free_lines = Vec::new();
    let mut size_lines = Vec::new();
    let mut init_lines = Vec::new();
    let mut extension_init_lines = Vec::new();
    let mut getter_lines = Vec::new();
    let mut decode_lines = Vec::new();
    let mut needs_interned_error = false;

    for field in record.fields() {
        add_encoded_field(
            field.ty(),
            &field_name(field.key())?,
            ident,
            c_type,
            &symbols.data_type,
            EncodedFieldParts {
                field_defs: &mut field_defs,
                support_defs: &mut support_defs,
                free_lines: &mut free_lines,
                size_lines: &mut size_lines,
                init_lines: &mut init_lines,
                extension_init_lines: &mut extension_init_lines,
                getter_lines: &mut getter_lines,
                decode_lines: &mut decode_lines,
                needs_interned_error: &mut needs_interned_error,
            },
        )?;
    }

    let mut out = String::new();
    out.push_str(&format!(
        "/* boltffi:ruby:record class={} var={} */\n",
        symbols.class_name, symbols.class_var
    ));
    out.push_str(&format!(
        "typedef struct {{\n{}\n}} {c_type};\n",
        field_defs.join("\n")
    ));
    out.push_str(&support_defs.join(""));
    out.push_str(&format!("static VALUE {};\n", symbols.class_var));
    out.push_str(&format!(
        "static void boltffi_ruby_{ident}_free(void *ptr) {{\n    {c_type} *data = ({c_type} *)ptr;\n    if (data != NULL) {{\n{}\n#if !BOLTFFI_RUBY_HAS_TYPED_EMBEDDABLE\n        xfree(data);\n#endif\n    }}\n}}\n",
        free_lines.join("\n")
    ));
    out.push_str(&format!(
        "static size_t boltffi_ruby_{ident}_size(const void *ptr) {{\n    const {c_type} *data = (const {c_type} *)ptr;\n    size_t size = 0;\n#if !BOLTFFI_RUBY_HAS_TYPED_EMBEDDABLE\n    size += sizeof({c_type});\n#endif\n    if (data != NULL) {{\n{}\n    }}\n    return size;\n}}\n",
        size_lines.join("\n")
    ));
    out.push_str(&format!(
        "static const rb_data_type_t {} = {{\n    \"boltffi.ruby.generated.{}\", {{ NULL, boltffi_ruby_{ident}_free, boltffi_ruby_{ident}_size, NULL }}, NULL, NULL, RUBY_TYPED_FREE_IMMEDIATELY | BOLTFFI_RUBY_TYPED_EMBEDDABLE\n}};\n",
        symbols.data_type, symbols.class_name
    ));
    out.push_str(&format!(
        "static VALUE boltffi_ruby_{ident}_from_stack(VALUE source) {{\n    {c_type} *data;\n    {c_type} *src = ({c_type} *)(uintptr_t)source;\n    VALUE obj = TypedData_Make_Struct({}, {c_type}, &{}, data);\n    *data = *src;\n    return obj;\n}}\n",
        symbols.class_var, symbols.data_type
    ));
    out.push_str(&getter_lines.join(""));
    let interned_error_decl = if needs_interned_error {
        "    int interned_error = 0;\n"
    } else {
        ""
    };
    let interned_error_label = if needs_interned_error {
        format!(
            "invalid_interned:\n{}\n    boltffi_free_buf(buffer);\n    rb_raise(rb_eRuntimeError, interned_error == 1 ? \"native function returned invalid interned string tag\" : \"native function returned invalid interned string id\");\n",
            free_lines.join("\n")
        )
    } else {
        String::new()
    };
    out.push_str(&format!(
        "static VALUE {}(FfiBuf_u8 buffer) {{\n    {c_type} stack_data;\n    {c_type} *data = &stack_data;\n{}\n{}    boltffi_ruby_wire_reader reader = boltffi_ruby_wire_reader_new(buffer);\n{}\n    boltffi_free_buf(buffer);\n    int error = 0;\n    VALUE obj = rb_protect(boltffi_ruby_{ident}_from_stack, (VALUE)(uintptr_t)&stack_data, &error);\n    if (error) {{\n{}\n        rb_jump_tag(error);\n    }}\n    RB_GC_GUARD(obj);\n    return obj;\n{}fail:\n{}\n    boltffi_free_buf(buffer);\n    rb_raise(rb_eRuntimeError, \"native function returned truncated {} buffer\");\n}}\n",
        symbols.decoder.as_ref().expect("decoder"),
        init_lines.iter().map(|line| format!("    {line}")).collect::<Vec<_>>().join("\n"),
        interned_error_decl,
        decode_lines.iter().map(|line| format!("    {line}")).collect::<Vec<_>>().join("\n"),
        free_lines.join("\n"),
        interned_error_label,
        free_lines.join("\n"),
        symbols.class_name
    ));
    out.push_str(&format!(
        "/* boltffi:ruby:record:init {} */\n",
        symbols.ident
    ));
    out.push_str(&extension_init_lines.join(""));
    out.push_str(&format!(
        "    {} = rb_define_class_under(mod, \"{}\", rb_cObject);\n    rb_undef_alloc_func({});\n",
        symbols.class_var, symbols.class_name, symbols.class_var
    ));
    for field in record.fields() {
        let name = field_name(field.key())?;
        out.push_str(&format!(
            "    rb_define_method({}, \"{name}\", boltffi_ruby_{ident}_{name}, 0);\n",
            symbols.class_var
        ));
    }
    out.push_str("/* boltffi:ruby:record:init:end */\n");
    Ok(out)
}

struct EncodedFieldParts<'a> {
    field_defs: &'a mut Vec<String>,
    support_defs: &'a mut Vec<String>,
    free_lines: &'a mut Vec<String>,
    size_lines: &'a mut Vec<String>,
    init_lines: &'a mut Vec<String>,
    extension_init_lines: &'a mut Vec<String>,
    getter_lines: &'a mut Vec<String>,
    decode_lines: &'a mut Vec<String>,
    needs_interned_error: &'a mut bool,
}

fn add_encoded_field(
    ty: &TypeRef,
    name: &str,
    ident: &str,
    c_type: &str,
    data_type: &str,
    mut parts: EncodedFieldParts<'_>,
) -> Result<()> {
    match ty {
        TypeRef::Optional(inner) => {
            add_encoded_field_inner(inner, name, ident, c_type, data_type, true, &mut parts)
        }
        _ => add_encoded_field_inner(ty, name, ident, c_type, data_type, false, &mut parts),
    }
}

fn add_encoded_field_inner(
    ty: &TypeRef,
    name: &str,
    ident: &str,
    c_type: &str,
    data_type: &str,
    optional: bool,
    parts: &mut EncodedFieldParts<'_>,
) -> Result<()> {
    if optional {
        parts.field_defs.push(format!("    int {name}_present;"));
        parts
            .init_lines
            .push(format!("    data->{name}_present = 0;"));
        parts.decode_lines.push(format!(
            "    if (!boltffi_ruby_wire_read_option_tag(&reader, &data->{name}_present)) goto fail;"
        ));
    }

    let guard = optional.then(|| format!("data->{name}_present"));
    match ty {
        TypeRef::String => {
            parts.field_defs.push(format!("    char *{name}_ptr;"));
            parts.field_defs.push(format!("    uintptr_t {name}_len;"));
            parts
                .free_lines
                .push(format!("    free(data->{name}_ptr);"));
            parts
                .size_lines
                .push(format!("    size += data->{name}_len;"));
            parts
                .init_lines
                .push(format!("    data->{name}_ptr = NULL;"));
            parts.init_lines.push(format!("    data->{name}_len = 0;"));
            let read = format!(
                "if (!boltffi_ruby_wire_read_string(&reader, &data->{name}_ptr, &data->{name}_len)) goto fail;"
            );
            parts.decode_lines.push(match &guard {
                Some(guard) => format!("    if ({guard}) {{ {read} }}"),
                None => format!("    {read}"),
            });
            let nil_guard = if optional {
                format!("    if (!data->{name}_present) return Qnil;\n")
            } else {
                Default::default()
            };
            parts.getter_lines.push(format!(
                "static VALUE boltffi_ruby_{ident}_{name}(VALUE self) {{\n    {c_type} *data;\n    TypedData_Get_Struct(self, {c_type}, &{data_type}, data);\n{nil_guard}    VALUE result = rb_utf8_str_new(data->{name}_ptr, (long)data->{name}_len);\n    RB_ENC_CODERANGE_SET(result, RUBY_ENC_CODERANGE_VALID);\n    RB_GC_GUARD(self);\n    return result;\n}}\n"
            ));
        }
        TypeRef::Bytes => {
            parts.field_defs.push(format!("    uint8_t *{name}_ptr;"));
            parts.field_defs.push(format!("    uintptr_t {name}_len;"));
            parts
                .free_lines
                .push(format!("    free(data->{name}_ptr);"));
            parts
                .size_lines
                .push(format!("    size += data->{name}_len;"));
            parts
                .init_lines
                .push(format!("    data->{name}_ptr = NULL;"));
            parts.init_lines.push(format!("    data->{name}_len = 0;"));
            let read = format!(
                "if (!boltffi_ruby_wire_read_bytes(&reader, &data->{name}_ptr, &data->{name}_len)) goto fail;"
            );
            parts.decode_lines.push(match &guard {
                Some(guard) => format!("    if ({guard}) {{ {read} }}"),
                None => format!("    {read}"),
            });
            let nil_guard = if optional {
                format!("    if (!data->{name}_present) return Qnil;\n")
            } else {
                Default::default()
            };
            parts.getter_lines.push(format!(
                "static VALUE boltffi_ruby_{ident}_{name}(VALUE self) {{\n    {c_type} *data;\n    TypedData_Get_Struct(self, {c_type}, &{data_type}, data);\n{nil_guard}    VALUE result = rb_str_new((const char *)data->{name}_ptr, (long)data->{name}_len);\n    RB_GC_GUARD(self);\n    return result;\n}}\n"
            ));
        }
        TypeRef::InternedString { static_values } => {
            validate_interned_static_values(static_values)?;
            let table = format!("boltffi_ruby_{ident}_{name}_interned_values");
            let read_fn = format!("boltffi_ruby_{ident}_{name}_read_interned");
            let count = static_values.len();
            let table_len = count.max(1);
            parts.field_defs.push(format!("    int {name}_static;"));
            parts.field_defs.push(format!("    uint32_t {name}_id;"));
            parts.field_defs.push(format!("    char *{name}_ptr;"));
            parts.field_defs.push(format!("    uintptr_t {name}_len;"));
            parts.free_lines.push(format!(
                "    if (!data->{name}_static) free(data->{name}_ptr);"
            ));
            parts.size_lines.push(format!(
                "    if (!data->{name}_static) size += data->{name}_len;"
            ));
            parts
                .init_lines
                .push(format!("    data->{name}_static = 1;"));
            parts.init_lines.push(format!("    data->{name}_id = 0;"));
            parts
                .init_lines
                .push(format!("    data->{name}_ptr = NULL;"));
            parts.init_lines.push(format!("    data->{name}_len = 0;"));
            parts
                .support_defs
                .push(format!("static VALUE {table}[{table_len}];\n"));
            parts.support_defs.push(format!(
                "static int {read_fn}(boltffi_ruby_wire_reader *reader, {c_type} *data) {{\n    uint8_t tag;\n    if (!boltffi_ruby_wire_read_u8(reader, &tag)) return 0;\n    data->{name}_static = 0;\n    data->{name}_id = 0;\n    data->{name}_ptr = NULL;\n    data->{name}_len = 0;\n    if (tag == 0) {{\n        uint32_t id;\n        if (!boltffi_ruby_wire_read_u32(reader, &id)) return 0;\n        if (id >= {count}) return 3;\n        data->{name}_static = 1;\n        data->{name}_id = id;\n        return 1;\n    }}\n    if (tag == 1) {{\n        if (!boltffi_ruby_wire_read_string(reader, &data->{name}_ptr, &data->{name}_len)) return 0;\n        return 1;\n    }}\n    return 2;\n}}\n"
            ));
            for (index, value) in static_values.iter().enumerate() {
                let literal = c_string_literal(value);
                parts.extension_init_lines.push(format!(
                    "    BOLTFFI_RUBY_DEFINE_INTERNED_STATIC({table}[{index}], {literal});\n"
                ));
            }
            let read = format!(
                "int {name}_status = {read_fn}(&reader, data); if ({name}_status == 0) goto fail; if ({name}_status == 2) {{ interned_error = 1; goto invalid_interned; }} if ({name}_status == 3) {{ interned_error = 2; goto invalid_interned; }}"
            );
            parts.decode_lines.push(match &guard {
                Some(guard) => format!("    if ({guard}) {{ {read} }}"),
                None => format!("    {read}"),
            });
            *parts.needs_interned_error = true;
            let nil_guard = if optional {
                format!("    if (!data->{name}_present) return Qnil;\n")
            } else {
                Default::default()
            };
            parts.getter_lines.push(format!(
                "static VALUE boltffi_ruby_{ident}_{name}(VALUE self) {{\n    {c_type} *data;\n    TypedData_Get_Struct(self, {c_type}, &{data_type}, data);\n{nil_guard}    VALUE result;\n    if (data->{name}_static) {{\n        if (data->{name}_id >= {count}) rb_raise(rb_eRuntimeError, \"native function returned invalid interned string id\");\n        result = {table}[data->{name}_id];\n    }} else {{\n        result = rb_utf8_str_new(data->{name}_ptr, (long)data->{name}_len);\n        RB_ENC_CODERANGE_SET(result, RUBY_ENC_CODERANGE_VALID);\n        rb_obj_freeze(result);\n    }}\n    RB_GC_GUARD(self);\n    return result;\n}}\n"
            ));
        }
        TypeRef::Primitive(primitive) => {
            let c_field_type = primitive_c_type(*primitive)?;
            parts.field_defs.push(format!("    {c_field_type} {name};"));
            parts.init_lines.push(format!("    data->{name} = 0;"));
            let read_fn = wire_read_fn(*primitive)?;
            let read = format!("if (!{read_fn}(&reader, &data->{name})) goto fail;");
            parts.decode_lines.push(match &guard {
                Some(guard) => format!("    if ({guard}) {{ {read} }}"),
                None => format!("    {read}"),
            });
            let expr = primitive_box(*primitive).replace("_ffi_ret", &format!("data->{name}"));
            let nil_guard = if optional {
                format!("    if (!data->{name}_present) return Qnil;\n")
            } else {
                Default::default()
            };
            parts.getter_lines.push(format!(
                "static VALUE boltffi_ruby_{ident}_{name}(VALUE self) {{\n    {c_type} *data;\n    TypedData_Get_Struct(self, {c_type}, &{data_type}, data);\n{nil_guard}    VALUE result = {expr};\n    RB_GC_GUARD(self);\n    return result;\n}}\n"
            ));
        }
        TypeRef::Optional(_) => {
            return Err(Error::UnsupportedTarget {
                target: "ruby",
                shape: "nested optional encoded record field",
            });
        }
        _ => {
            return Err(Error::UnsupportedTarget {
                target: "ruby",
                shape: "unsupported encoded record field",
            });
        }
    }
    Ok(())
}

pub fn native_opaque_record_supported(record: &EncodedRecordDecl<Native>) -> bool {
    record.fields().iter().all(|field| {
        matches!(field.key(), FieldKey::Named(_)) && native_opaque_field_supported(field.ty())
    })
}

fn native_opaque_field_supported(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::String | TypeRef::Bytes | TypeRef::InternedString { .. } => true,
        TypeRef::Primitive(_) => true,
        TypeRef::Optional(inner) => {
            !matches!(inner.as_ref(), TypeRef::Optional(_)) && native_opaque_field_supported(inner)
        }
        _ => false,
    }
}

fn native_opaque_field_name(key: &FieldKey) -> Result<String> {
    field_name(key)
}

fn render_native_opaque(record: &EncodedRecordDecl<Native>, symbols: &Symbols) -> Result<String> {
    let ident = &symbols.ident;
    let c_type = format!("boltffi_ruby_{ident}");
    let exports = record
        .native_opaque_exports()
        .ok_or(Error::UnsupportedTarget {
            target: "ruby",
            shape: "native opaque record without helper exports",
        })?;
    let drop_symbol = exports.drop().name().as_str();
    let dsize_symbol = exports.dsize().name().as_str();

    // Determine which fields need VALUE caches (not primitives)
    let mut value_cache_fields: Vec<String> = Vec::new();
    let mut cache_initializers: Vec<String> = Vec::new();
    let mut mark_lines: Vec<String> = Vec::new();
    let mut compact_lines: Vec<String> = Vec::new();
    let mut getter_lines: Vec<String> = Vec::new();
    let mut support_defs: Vec<String> = Vec::new();
    let mut extension_init_lines: Vec<String> = Vec::new();
    let mut method_defs: Vec<String> = Vec::new();

    for field in record.fields() {
        let name = native_opaque_field_name(field.key())?;
        if matches!(name.as_str(), "checked" | "close") {
            return Err(Error::UnsupportedTarget {
                target: "ruby",
                shape: "native opaque record field conflicts with generated Ruby lifecycle support",
            });
        }
        let (ty, optional) = match field.ty() {
            TypeRef::Optional(inner) => (inner.as_ref(), true),
            ty => (ty, false),
        };

        let field_export = exports.field(field.key()).ok_or(Error::UnsupportedTarget {
            target: "ruby",
            shape: "native opaque record field without matching exports",
        })?;
        let has_sym: String = if optional {
            field_export
                .has()
                .ok_or(Error::UnsupportedTarget {
                    target: "ruby",
                    shape: "native opaque optional field without has export",
                })?
                .name()
                .as_str()
                .to_owned()
        } else {
            String::new()
        };

        match ty {
            TypeRef::Primitive(primitive) => {
                let get_sym = field_export
                    .get()
                    .ok_or(Error::UnsupportedTarget {
                        target: "ruby",
                        shape: "native opaque primitive field without get export",
                    })?
                    .name()
                    .as_str()
                    .to_owned();
                value_cache_fields.push(name.clone());
                cache_initializers.push(format!("    data->{name}_value = Qundef;"));
                mark_lines.push(format!(
                    "    if (data->{name}_value != Qundef && !RB_SPECIAL_CONST_P(data->{name}_value)) rb_gc_mark_movable(data->{name}_value);"
                ));
                compact_lines.push(format!(
                    "    if (data->{name}_value != Qundef && !RB_SPECIAL_CONST_P(data->{name}_value)) data->{name}_value = rb_gc_location(data->{name}_value);"
                ));
                let c_prim_type = primitive_c_type(*primitive)?;
                let box_expr = primitive_box(*primitive).replace("_ffi_ret", "val");
                if optional {
                    getter_lines.push(format!(
                        "static VALUE boltffi_ruby_{ident}_{name}(VALUE self) {{\n    {c_type} *data = boltffi_ruby_{ident}_checked(self);\n    if (data->{name}_value != Qundef) return data->{name}_value;\n    if (!{has_sym}(data->native)) {{\n        RB_OBJ_WRITE(self, &data->{name}_value, Qnil);\n        return Qnil;\n    }}\n    {c_prim_type} val = {get_sym}(data->native);\n    VALUE result = {box_expr};\n    RB_OBJ_WRITE(self, &data->{name}_value, result);\n    RB_GC_GUARD(self);\n    return data->{name}_value;\n}}\n"
                    ));
                } else {
                    getter_lines.push(format!(
                        "static VALUE boltffi_ruby_{ident}_{name}(VALUE self) {{\n    {c_type} *data = boltffi_ruby_{ident}_checked(self);\n    if (data->{name}_value != Qundef) return data->{name}_value;\n    {c_prim_type} val = {get_sym}(data->native);\n    VALUE result = {box_expr};\n    RB_OBJ_WRITE(self, &data->{name}_value, result);\n    RB_GC_GUARD(self);\n    return data->{name}_value;\n}}\n"
                    ));
                }
            }
            TypeRef::String => {
                let borrow_sym = field_export
                    .borrow()
                    .ok_or(Error::UnsupportedTarget {
                        target: "ruby",
                        shape: "native opaque string field without borrow export",
                    })?
                    .name()
                    .as_str()
                    .to_owned();
                value_cache_fields.push(name.clone());
                cache_initializers.push(format!("    data->{name}_value = Qundef;"));
                mark_lines.push(format!(
                    "    if (data->{name}_value != Qundef && data->{name}_value != Qnil) rb_gc_mark_movable(data->{name}_value);"
                ));
                compact_lines.push(format!(
                    "    if (data->{name}_value != Qundef && data->{name}_value != Qnil) data->{name}_value = rb_gc_location(data->{name}_value);"
                ));
                if optional {
                    getter_lines.push(format!(
                        "static VALUE boltffi_ruby_{ident}_{name}(VALUE self) {{\n    {c_type} *data = boltffi_ruby_{ident}_checked(self);\n    if (data->{name}_value != Qundef) return data->{name}_value;\n    if (!{has_sym}(data->native)) {{\n        RB_OBJ_WRITE(self, &data->{name}_value, Qnil);\n        return Qnil;\n    }}\n    const uint8_t *ptr; uintptr_t len;\n    if (!{borrow_sym}(data->native, &ptr, &len)) rb_raise(rb_eRuntimeError, \"native opaque record field accessor failed\");\n    VALUE result = rb_utf8_str_new((const char *)ptr, (long)len);\n    RB_ENC_CODERANGE_SET(result, RUBY_ENC_CODERANGE_VALID);\n    rb_obj_freeze(result);\n    RB_OBJ_WRITE(self, &data->{name}_value, result);\n    RB_GC_GUARD(self);\n    return data->{name}_value;\n}}\n"
                    ));
                } else {
                    getter_lines.push(format!(
                        "static VALUE boltffi_ruby_{ident}_{name}(VALUE self) {{\n    {c_type} *data = boltffi_ruby_{ident}_checked(self);\n    if (data->{name}_value != Qundef) return data->{name}_value;\n    const uint8_t *ptr; uintptr_t len;\n    if (!{borrow_sym}(data->native, &ptr, &len)) rb_raise(rb_eRuntimeError, \"native opaque record field accessor failed\");\n    VALUE result = rb_utf8_str_new((const char *)ptr, (long)len);\n    RB_ENC_CODERANGE_SET(result, RUBY_ENC_CODERANGE_VALID);\n    rb_obj_freeze(result);\n    RB_OBJ_WRITE(self, &data->{name}_value, result);\n    RB_GC_GUARD(self);\n    return data->{name}_value;\n}}\n"
                    ));
                }
            }
            TypeRef::Bytes => {
                let borrow_sym = field_export
                    .borrow()
                    .ok_or(Error::UnsupportedTarget {
                        target: "ruby",
                        shape: "native opaque bytes field without borrow export",
                    })?
                    .name()
                    .as_str()
                    .to_owned();
                value_cache_fields.push(name.clone());
                cache_initializers.push(format!("    data->{name}_value = Qundef;"));
                mark_lines.push(format!(
                    "    if (data->{name}_value != Qundef && data->{name}_value != Qnil) rb_gc_mark_movable(data->{name}_value);"
                ));
                compact_lines.push(format!(
                    "    if (data->{name}_value != Qundef && data->{name}_value != Qnil) data->{name}_value = rb_gc_location(data->{name}_value);"
                ));
                if optional {
                    getter_lines.push(format!(
                        "static VALUE boltffi_ruby_{ident}_{name}(VALUE self) {{\n    {c_type} *data = boltffi_ruby_{ident}_checked(self);\n    if (data->{name}_value != Qundef) return data->{name}_value;\n    if (!{has_sym}(data->native)) {{\n        RB_OBJ_WRITE(self, &data->{name}_value, Qnil);\n        return Qnil;\n    }}\n    const uint8_t *ptr; uintptr_t len;\n    if (!{borrow_sym}(data->native, &ptr, &len)) rb_raise(rb_eRuntimeError, \"native opaque record field accessor failed\");\n    VALUE result = rb_str_new((const char *)ptr, (long)len);\n    rb_obj_freeze(result);\n    RB_OBJ_WRITE(self, &data->{name}_value, result);\n    RB_GC_GUARD(self);\n    return data->{name}_value;\n}}\n"
                    ));
                } else {
                    getter_lines.push(format!(
                        "static VALUE boltffi_ruby_{ident}_{name}(VALUE self) {{\n    {c_type} *data = boltffi_ruby_{ident}_checked(self);\n    if (data->{name}_value != Qundef) return data->{name}_value;\n    const uint8_t *ptr; uintptr_t len;\n    if (!{borrow_sym}(data->native, &ptr, &len)) rb_raise(rb_eRuntimeError, \"native opaque record field accessor failed\");\n    VALUE result = rb_str_new((const char *)ptr, (long)len);\n    rb_obj_freeze(result);\n    RB_OBJ_WRITE(self, &data->{name}_value, result);\n    RB_GC_GUARD(self);\n    return data->{name}_value;\n}}\n"
                    ));
                }
            }
            TypeRef::InternedString { static_values } => {
                let interned_tag_sym = field_export
                    .interned_tag()
                    .ok_or(Error::UnsupportedTarget {
                        target: "ruby",
                        shape: "native opaque interned string field without interned_tag export",
                    })?
                    .name()
                    .as_str()
                    .to_owned();
                let interned_id_sym = field_export
                    .interned_id()
                    .ok_or(Error::UnsupportedTarget {
                        target: "ruby",
                        shape: "native opaque interned string field without interned_id export",
                    })?
                    .name()
                    .as_str()
                    .to_owned();
                let interned_borrow_sym = field_export
                    .interned_borrow_dynamic()
                    .ok_or(Error::UnsupportedTarget {
                        target: "ruby",
                        shape: "native opaque interned string field without interned_borrow_dynamic export",
                    })?
                    .name()
                    .as_str()
                    .to_owned();
                validate_interned_static_values(static_values)?;
                let table = format!("boltffi_ruby_{ident}_{name}_table");
                let count = static_values.len();
                let table_len = count.max(1);
                value_cache_fields.push(name.clone());
                cache_initializers.push(format!("    data->{name}_value = Qundef;"));
                mark_lines.push(format!(
                    "    if (data->{name}_value != Qundef && data->{name}_value != Qnil) rb_gc_mark_movable(data->{name}_value);"
                ));
                compact_lines.push(format!(
                    "    if (data->{name}_value != Qundef && data->{name}_value != Qnil) data->{name}_value = rb_gc_location(data->{name}_value);"
                ));
                support_defs.push(format!("static VALUE {table}[{table_len}];\n"));
                for (index, value) in static_values.iter().enumerate() {
                    let literal = c_string_literal(value);
                    extension_init_lines.push(format!(
                        "    BOLTFFI_RUBY_DEFINE_INTERNED_STATIC({table}[{index}], {literal});\n"
                    ));
                }
                if optional {
                    getter_lines.push(format!(
                        "static VALUE boltffi_ruby_{ident}_{name}(VALUE self) {{\n    {c_type} *data = boltffi_ruby_{ident}_checked(self);\n    if (data->{name}_value != Qundef) return data->{name}_value;\n    if (!{has_sym}(data->native)) {{\n        RB_OBJ_WRITE(self, &data->{name}_value, Qnil);\n        return Qnil;\n    }}\n    uint8_t tag = {interned_tag_sym}(data->native);\n    VALUE result;\n    if (tag == 0) {{\n        uint32_t id = {interned_id_sym}(data->native);\n        if (id >= {count}) rb_raise(rb_eRuntimeError, \"native opaque record: invalid interned string id\");\n        result = {table}[id];\n    }} else if (tag == 1) {{\n        const uint8_t *ptr; uintptr_t len;\n        if (!{interned_borrow_sym}(data->native, &ptr, &len)) rb_raise(rb_eRuntimeError, \"native opaque record: interned borrow failed\");\n        result = rb_utf8_str_new((const char *)ptr, (long)len);\n        RB_ENC_CODERANGE_SET(result, RUBY_ENC_CODERANGE_VALID);\n        rb_obj_freeze(result);\n    }} else {{\n        rb_raise(rb_eRuntimeError, \"native opaque record: invalid interned string tag\");\n    }}\n    RB_OBJ_WRITE(self, &data->{name}_value, result);\n    RB_GC_GUARD(self);\n    return data->{name}_value;\n}}\n"
                    ));
                } else {
                    getter_lines.push(format!(
                        "static VALUE boltffi_ruby_{ident}_{name}(VALUE self) {{\n    {c_type} *data = boltffi_ruby_{ident}_checked(self);\n    if (data->{name}_value != Qundef) return data->{name}_value;\n    uint8_t tag = {interned_tag_sym}(data->native);\n    VALUE result;\n    if (tag == 0) {{\n        uint32_t id = {interned_id_sym}(data->native);\n        if (id >= {count}) rb_raise(rb_eRuntimeError, \"native opaque record: invalid interned string id\");\n        result = {table}[id];\n    }} else if (tag == 1) {{\n        const uint8_t *ptr; uintptr_t len;\n        if (!{interned_borrow_sym}(data->native, &ptr, &len)) rb_raise(rb_eRuntimeError, \"native opaque record: interned borrow failed\");\n        result = rb_utf8_str_new((const char *)ptr, (long)len);\n        RB_ENC_CODERANGE_SET(result, RUBY_ENC_CODERANGE_VALID);\n        rb_obj_freeze(result);\n    }} else {{\n        rb_raise(rb_eRuntimeError, \"native opaque record: invalid interned string tag\");\n    }}\n    RB_OBJ_WRITE(self, &data->{name}_value, result);\n    RB_GC_GUARD(self);\n    return data->{name}_value;\n}}\n"
                    ));
                }
            }
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "ruby",
                    shape: "unsupported native opaque record field type",
                });
            }
        }

        method_defs.push(format!(
            "    rb_define_method({}, \"{name}\", boltffi_ruby_{ident}_{name}, 0);\n",
            symbols.class_var
        ));
    }

    // Build C struct fields: void *native + VALUE caches
    let mut struct_fields = vec!["    void *native;".to_owned()];
    for cache_field in &value_cache_fields {
        struct_fields.push(format!("    VALUE {cache_field}_value;"));
    }

    let struct_def = format!(
        "typedef struct {{\n{}\n}} {c_type};\n",
        struct_fields.join("\n")
    );

    let mark_body = mark_lines.join("\n");
    let compact_body = compact_lines.join("\n");
    let mark_fn = format!(
        "static void boltffi_ruby_{ident}_mark(void *ptr) {{\n    {c_type} *data = ({c_type} *)ptr;\n    if (data == NULL) return;\n{mark_body}\n}}\n"
    );
    let compact_fn = format!(
        "static void boltffi_ruby_{ident}_compact(void *ptr) {{\n    {c_type} *data = ({c_type} *)ptr;\n    if (data == NULL) return;\n{compact_body}\n}}\n"
    );
    let free_fn = format!(
        "static void boltffi_ruby_{ident}_free(void *ptr) {{\n    {c_type} *data = ({c_type} *)ptr;\n    if (data != NULL) {{\n        void *native = data->native;\n        data->native = NULL;\n        if (native != NULL) {{\n            {drop_symbol}(native);
        }}\n#if !BOLTFFI_RUBY_HAS_TYPED_EMBEDDABLE\n        xfree(data);\n#endif\n    }}\n}}\n"
    );
    let size_fn = format!(
        "static size_t boltffi_ruby_{ident}_size(const void *ptr) {{\n    const {c_type} *data = (const {c_type} *)ptr;\n    size_t size = 0;\n#if !BOLTFFI_RUBY_HAS_TYPED_EMBEDDABLE\n    size += sizeof({c_type});\n#endif\n    if (data != NULL && data->native != NULL) size += {dsize_symbol}(data->native);
    return size;\n}}\n"
    );
    let data_type_def = format!(
        "static const rb_data_type_t {} = {{\n    \"boltffi.ruby.generated.{}\", {{ boltffi_ruby_{ident}_mark, boltffi_ruby_{ident}_free, boltffi_ruby_{ident}_size, boltffi_ruby_{ident}_compact }}, NULL, NULL, RUBY_TYPED_FREE_IMMEDIATELY | BOLTFFI_RUBY_TYPED_EMBEDDABLE\n}};\n",
        symbols.data_type, symbols.class_name
    );

    let cache_init_body = cache_initializers.join("\n");
    let from_native_fn = format!(
        "static VALUE boltffi_ruby_{ident}_from_native(VALUE src) {{\n    void *native = (void *)(uintptr_t)src;\n    {c_type} *data;\n    VALUE obj = TypedData_Make_Struct({}, {c_type}, &{}, data);\n    data->native = native;\n{cache_init_body}\n    return obj;\n}}\n",
        symbols.class_var, symbols.data_type
    );

    let boxer_fn = format!(
        "static VALUE {}(void *native) {{\n    if (native == NULL) rb_raise(rb_eRuntimeError, \"native opaque record returned null\");\n    int error = 0;\n    VALUE obj = rb_protect(boltffi_ruby_{ident}_from_native, (VALUE)(uintptr_t)native, &error);\n    if (error) {{\n        {drop_symbol}(native);
        rb_jump_tag(error);\n    }}\n    RB_GC_GUARD(obj);\n    return obj;\n}}\n",
        symbols.boxer
    );

    // Every field getter dereferences `data->native`, so a single checked
    // unwrap gives uniform "use of released" semantics after `close`/GC drop
    // instead of per-accessor divergence (primitives defaulting, optionals
    // returning nil, strings raising a generic accessor error).
    let checked_fn = format!(
        "static {c_type} *boltffi_ruby_{ident}_checked(VALUE self) {{\n    {c_type} *data = ({c_type} *)RTYPEDDATA_DATA(self);\n    if (data == NULL || data->native == NULL) rb_raise(rb_eRuntimeError, \"use of released {}\");\n    return data;\n}}\n",
        symbols.class_name
    );
    // Deterministic release: move+clear before drop so a reentrant `close`
    // (or a callback invoked from the record's Drop) observes a null handle
    // and cannot double-drop. Idempotent; safe to call after GC free.
    let close_fn = format!(
        "static VALUE boltffi_ruby_{ident}_close(VALUE self) {{\n    {c_type} *data = ({c_type} *)RTYPEDDATA_DATA(self);\n    if (data != NULL && data->native != NULL) {{\n        void *native = data->native;\n        data->native = NULL;\n        {drop_symbol}(native);\n    }}\n    return Qnil;\n}}\n"
    );

    let method_defs_str = method_defs.join("");
    let extension_init_str = extension_init_lines.join("");
    let support_defs_str = support_defs.join("");
    let getter_defs_str = getter_lines.join("\n");
    let class_var = &symbols.class_var;
    let class_name = &symbols.class_name;

    // Format: declarations body + init markers (parsed by parse_record_rendering)
    let mut out = format!("/* boltffi:ruby:record class={class_name} var={class_var} */\n");
    out.push_str(&format!("static VALUE {class_var};\n"));
    out.push_str(&support_defs_str);
    out.push_str(&struct_def);
    out.push('\n');
    out.push_str(&mark_fn);
    out.push('\n');
    out.push_str(&compact_fn);
    out.push('\n');
    out.push_str(&free_fn);
    out.push('\n');
    out.push_str(&size_fn);
    out.push('\n');
    out.push_str(&data_type_def);
    out.push('\n');
    out.push_str(&from_native_fn);
    out.push('\n');
    out.push_str(&boxer_fn);
    out.push('\n');
    out.push_str(&checked_fn);
    out.push('\n');
    out.push_str(&close_fn);
    out.push('\n');
    out.push_str(&getter_defs_str);
    out.push('\n');
    // Init section (picked up by parse_record_rendering)
    out.push_str(&format!("/* boltffi:ruby:record:init {ident} */\n"));
    out.push_str(&extension_init_str);
    out.push_str(&format!(
        "    {class_var} = rb_define_class_under(mod, \"{class_name}\", rb_cObject);\n    rb_undef_alloc_func({class_var});\n    rb_define_method({class_var}, \"close\", boltffi_ruby_{ident}_close, 0);\n"
    ));
    out.push_str(&method_defs_str);
    out.push_str("/* boltffi:ruby:record:init:end */\n");
    Ok(out)
}

fn field_name(key: &FieldKey) -> Result<String> {
    match key {
        FieldKey::Named(name) => Ok(Name::new(name).function()),
        FieldKey::Position(_) | _ => Err(Error::UnsupportedTarget {
            target: "ruby",
            shape: "tuple record field",
        }),
    }
}

fn validate_interned_static_values(values: &[String]) -> Result<()> {
    if values.is_empty() {
        return Err(Error::UnsupportedTarget {
            target: "ruby",
            shape: "empty interned string pool",
        });
    }

    let mut seen = HashSet::new();
    for value in values {
        if value.as_bytes().contains(&0) {
            return Err(Error::UnsupportedTarget {
                target: "ruby",
                shape: "interned string pool value containing NUL",
            });
        }
        if !seen.insert(value) {
            return Err(Error::UnsupportedTarget {
                target: "ruby",
                shape: "duplicate interned string pool value",
            });
        }
    }
    Ok(())
}

fn c_string_literal(value: &str) -> String {
    let mut out = String::from("\"");
    for byte in value.bytes() {
        match byte {
            b'\\' => out.push_str("\\\\"),
            b'\"' => out.push_str("\\\""),
            b'?' => out.push_str("\\?"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7e => out.push(byte as char),
            _ => out.push_str(&format!("\\{:03o}", byte)),
        }
    }
    out.push('"');
    out
}

fn wire_read_fn(primitive: Primitive) -> Result<&'static str> {
    Ok(match primitive {
        Primitive::Bool => "boltffi_ruby_wire_read_bool",
        Primitive::I8 => "boltffi_ruby_wire_read_i8",
        Primitive::U8 => "boltffi_ruby_wire_read_u8",
        Primitive::I16 => "boltffi_ruby_wire_read_i16",
        Primitive::U16 => "boltffi_ruby_wire_read_u16",
        Primitive::I32 => "boltffi_ruby_wire_read_i32",
        Primitive::U32 => "boltffi_ruby_wire_read_u32",
        Primitive::I64 => "boltffi_ruby_wire_read_i64",
        Primitive::U64 => "boltffi_ruby_wire_read_u64",
        Primitive::F32 => "boltffi_ruby_wire_read_f32",
        Primitive::F64 => "boltffi_ruby_wire_read_f64",
        _ => {
            return Err(Error::UnsupportedTarget {
                target: "ruby",
                shape: "unsupported encoded record primitive",
            });
        }
    })
}
