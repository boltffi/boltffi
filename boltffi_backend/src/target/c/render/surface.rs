//! Package-prefixed C value types and the private wire codec used by the facade.

use boltffi_binding::{Bindings, EnumDecl, FieldKey, Native, Primitive, RecordDecl, TypeRef};

use crate::{
    bridge::c::Identifier,
    core::{Error, RenderContext, Result},
    target::c::name_style::Name,
};

use super::prefix::PackagePrefix;

pub fn render(
    bindings: &Bindings<Native>,
    context: &RenderContext<Native>,
    rendered: &std::collections::HashSet<boltffi_binding::DeclarationId>,
) -> Result<String> {
    // Partial coverage may skip declarations. Everything record-shaped uses
    // the closure of rendered records: a rendered encoded record may embed
    // another record whose codec the emitted helpers call, so those records
    // must be included even when their own declaration was skipped.
    let record_ids = included_record_ids(bindings, rendered);
    let is_rendered = |decl: &boltffi_binding::Decl<Native>| {
        rendered.contains(&boltffi_binding::DeclarationRef::from(decl).id())
    };
    let prefix = PackagePrefix::from_context(context);
    let mut out = String::new();
    out.push_str("#include <stdlib.h>\n#include <string.h>\n\n");
    out.push_str(&format!(
        "typedef struct {{ const uint8_t *ptr; uintptr_t len; }} {p}BytesView;\n\
         typedef struct {{ const char *ptr; uintptr_t len; }} {p}StringView;\n\
         typedef struct {{ uint8_t *ptr; uintptr_t len; }} {p}Bytes;\n\
         typedef struct {{ char *ptr; uintptr_t len; }} {p}String;\n\
         typedef struct {{ bool has_value; uint32_t value; }} {p}OptionU32;\n\
         typedef struct {{ bool has_value; float value; }} {p}OptionF32;\n\
         typedef struct {{ bool has_value; {p}StringView value; }} {p}OptionStringView;\n\
         typedef struct {{ bool has_value; {p}String value; }} {p}OptionString;\n\
         typedef struct {{ const {p}StringView *ptr; uintptr_t len; }} {p}StringSlice;\n\
         typedef struct {{ {p}String *ptr; uintptr_t len; }} {p}StringSequence;\n",
        p = package_pascal(context)
    ));

    // Raw bridge declarations precede this facade, so direct aliases are safe here.
    for decl in bindings.decls() {
        match boltffi_binding::DeclarationRef::from(decl) {
            boltffi_binding::DeclarationRef::Enum(EnumDecl::CStyle(enumeration)) => {
                if !is_rendered(decl) {
                    continue;
                }
                out.push_str(&format!(
                    "typedef ___{} {};\n",
                    Name::new(enumeration.name()).r#type(),
                    type_name(&prefix, enumeration.name())
                ));
            }
            boltffi_binding::DeclarationRef::Record(RecordDecl::Direct(record)) => {
                if !is_rendered(decl) {
                    continue;
                }
                out.push_str(&format!(
                    "typedef ___{} {};\n",
                    Name::new(record.name()).r#type(),
                    type_name(&prefix, record.name())
                ));
            }
            boltffi_binding::DeclarationRef::Class(class) => {
                if !is_rendered(decl) {
                    continue;
                }
                let ty = type_name(&prefix, class.name());
                out.push_str(&format!(
                    "typedef struct {{ uint64_t _boltffi_handle; }} {ty};\n"
                ));
            }
            _ => {}
        }
    }

    // Encoded records are named structs so mutually-referenced slice declarations work.
    for decl in bindings.decls() {
        if let boltffi_binding::DeclarationRef::Record(RecordDecl::Encoded(record)) =
            boltffi_binding::DeclarationRef::from(decl)
        {
            if !record_ids.contains(&record.id()) {
                continue;
            }
            let ty = type_name(&prefix, record.name());
            out.push_str(&format!(
                "typedef struct {ty}View {ty}View;\ntypedef struct {ty} {ty};\n"
            ));
        }
    }

    // Public primitive views and owned sequences. A Slice is borrowed; a Sequence is
    // returned by value and released by its package-prefixed free helper.
    for primitive in all_primitives() {
        let stem = primitive_stem(*primitive);
        let c = primitive_c(*primitive);
        out.push_str(&format!(
            "typedef struct {{ const {c} *ptr; uintptr_t len; }} {p}{stem}Slice;\n\
             typedef struct {{ {c} *ptr; uintptr_t len; }} {p}{stem}MutSlice;\n\
             typedef struct {{ {c} *ptr; uintptr_t len; }} {p}{stem}Sequence;\n",
            p = package_pascal(context)
        ));
    }
    for decl in bindings.decls() {
        if let boltffi_binding::DeclarationRef::Record(record) =
            boltffi_binding::DeclarationRef::from(decl)
        {
            if !record_ids.contains(&record.id()) {
                continue;
            }
            let ty = type_name(&prefix, record.name());
            let slice_element = if matches!(record, RecordDecl::Encoded(_)) {
                format!("{ty}View")
            } else {
                ty.clone()
            };
            out.push_str(&format!(
                "typedef struct {{ const {slice_element} *ptr; uintptr_t len; }} {ty}Slice;\n\
                 typedef struct {{ {ty} *ptr; uintptr_t len; }} {ty}MutSlice;\n\
                 typedef struct {{ {ty} *ptr; uintptr_t len; }} {ty}Sequence;\n"
            ));
        }
    }

    for decl in bindings.decls() {
        if let boltffi_binding::DeclarationRef::Record(RecordDecl::Encoded(record)) =
            boltffi_binding::DeclarationRef::from(decl)
        {
            if !record_ids.contains(&record.id()) {
                continue;
            }
            let ty = type_name(&prefix, record.name());
            out.push_str(&format!("struct {ty}View {{\n"));
            for field in record.fields() {
                let field_name = match field.key() {
                    FieldKey::Named(name) => {
                        Identifier::escape(Name::new(name).member())?.to_string()
                    }
                    _ => return unsupported("tuple encoded record fields"),
                };
                let field_ty = view_type(field.ty(), context)?;
                out.push_str(&format!("    {field_ty} {field_name};\n"));
            }
            out.push_str("};\n");
            out.push_str(&format!("struct {ty} {{\n"));
            for field in record.fields() {
                let field_name = match field.key() {
                    FieldKey::Named(name) => {
                        Identifier::escape(Name::new(name).member())?.to_string()
                    }
                    FieldKey::Position(_) => {
                        return Err(Error::UnsupportedTarget {
                            target: "c",
                            shape: "tuple encoded record fields",
                        });
                    }
                    _ => return unsupported("unknown encoded record field key"),
                };
                let field_ty = value_type(field.ty(), context, ValueUse::Field)?;
                out.push_str(&format!("    {field_ty} {field_name};\n"));
            }
            out.push_str("};\n");
        }
    }

    out.push_str(&wire_runtime(context));
    for decl in bindings.decls() {
        if let boltffi_binding::DeclarationRef::Record(RecordDecl::Encoded(record)) =
            boltffi_binding::DeclarationRef::from(decl)
        {
            if !record_ids.contains(&record.id()) {
                continue;
            }
            out.push_str(&record_codec(record, context)?);
        }
    }
    out.push_str(&free_helpers(bindings, context, &record_ids)?);
    Ok(out)
}

/// Collects the records whose surface must be emitted: every rendered record
/// plus the encoded records they transitively reference through fields.
fn included_record_ids(
    bindings: &Bindings<Native>,
    rendered: &std::collections::HashSet<boltffi_binding::DeclarationId>,
) -> std::collections::HashSet<boltffi_binding::RecordId> {
    use std::collections::{HashMap, HashSet};

    let encoded: HashMap<boltffi_binding::RecordId, &boltffi_binding::EncodedRecordDecl<Native>> =
        bindings
            .decls()
            .iter()
            .filter_map(|decl| match boltffi_binding::DeclarationRef::from(decl) {
                boltffi_binding::DeclarationRef::Record(RecordDecl::Encoded(record)) => {
                    Some((record.id(), &**record))
                }
                _ => None,
            })
            .collect();
    let mut included: HashSet<boltffi_binding::RecordId> = rendered
        .iter()
        .filter_map(|id| match id {
            boltffi_binding::DeclarationId::Record(record) => Some(*record),
            _ => None,
        })
        .collect();
    let mut queue: Vec<boltffi_binding::RecordId> = included.iter().copied().collect();
    while let Some(id) = queue.pop() {
        let Some(record) = encoded.get(&id) else {
            continue;
        };
        for field in record.fields() {
            collect_record_refs(field.ty(), &encoded, &mut included, &mut queue);
        }
    }
    included
}

fn collect_record_refs(
    ty: &TypeRef,
    encoded: &std::collections::HashMap<
        boltffi_binding::RecordId,
        &boltffi_binding::EncodedRecordDecl<Native>,
    >,
    included: &mut std::collections::HashSet<boltffi_binding::RecordId>,
    queue: &mut Vec<boltffi_binding::RecordId>,
) {
    match ty {
        TypeRef::Record(id) if encoded.contains_key(id) && !included.contains(id) => {
            included.insert(*id);
            queue.push(*id);
        }
        TypeRef::Optional(inner) => collect_record_refs(inner, encoded, included, queue),
        TypeRef::Sequence(element) => collect_record_refs(element, encoded, included, queue),
        _ => {}
    }
}

#[derive(Clone, Copy)]
pub enum ValueUse {
    Param,
    /// Borrowed but mutable direct-vector parameter (`&mut [T]`).
    ParamMut,
    Field,
    Return,
}

pub fn value_type(ty: &TypeRef, context: &RenderContext<Native>, use_: ValueUse) -> Result<String> {
    let p = package_pascal(context);
    let prefix = PackagePrefix::from_context(context);
    match ty {
        TypeRef::Primitive(primitive) => Ok(primitive_c(*primitive).to_owned()),
        TypeRef::String => Ok(format!(
            "{p}String{}",
            if matches!(use_, ValueUse::Param) {
                "View"
            } else {
                ""
            }
        )),
        TypeRef::Bytes => Ok(format!(
            "{p}Bytes{}",
            if matches!(use_, ValueUse::Param) {
                "View"
            } else {
                ""
            }
        )),
        TypeRef::Record(id) => context
            .record(*id)
            .map(|record| {
                let name = type_name(&prefix, record.name());
                if matches!(use_, ValueUse::Param) && matches!(record, RecordDecl::Encoded(_)) {
                    format!("{name}View")
                } else {
                    name
                }
            })
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing record declaration for semantic C type",
            }),
        TypeRef::Enum(id) => context
            .enumeration(*id)
            .map(|e| type_name(&prefix, e.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing enum declaration for semantic C type",
            }),
        TypeRef::Class(id) => context
            .class(*id)
            .map(|class| type_name(&prefix, class.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing class declaration for semantic C type",
            }),
        TypeRef::Optional(inner) if **inner == TypeRef::Primitive(Primitive::U32) => {
            Ok(format!("{p}OptionU32"))
        }
        TypeRef::Optional(inner) if **inner == TypeRef::Primitive(Primitive::F32) => {
            Ok(format!("{p}OptionF32"))
        }
        TypeRef::Optional(inner) if **inner == TypeRef::String => Ok(format!(
            "{p}OptionString{}",
            if matches!(use_, ValueUse::Param) {
                "View"
            } else {
                ""
            }
        )),
        TypeRef::Sequence(element) => sequence_type(element, context, use_),
        _ => unsupported("encoded value type"),
    }
}

fn view_type(ty: &TypeRef, context: &RenderContext<Native>) -> Result<String> {
    let p = package_pascal(context);
    let prefix = PackagePrefix::from_context(context);
    match ty {
        TypeRef::String => Ok(format!("{p}StringView")),
        TypeRef::Bytes => Ok(format!("{p}BytesView")),
        TypeRef::Primitive(v) => Ok(primitive_c(*v).into()),
        TypeRef::Enum(id) => context
            .enumeration(*id)
            .map(|e| type_name(&prefix, e.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing view enum",
            }),
        TypeRef::Record(id) => context
            .record(*id)
            .map(|r| {
                let n = type_name(&prefix, r.name());
                if matches!(r, RecordDecl::Encoded(_)) {
                    format!("{n}View")
                } else {
                    n
                }
            })
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing view record",
            }),
        TypeRef::Optional(inner) if **inner == TypeRef::Primitive(Primitive::U32) => {
            Ok(format!("{p}OptionU32"))
        }
        TypeRef::Optional(inner) if **inner == TypeRef::Primitive(Primitive::F32) => {
            Ok(format!("{p}OptionF32"))
        }
        TypeRef::Optional(inner) if **inner == TypeRef::String => {
            Ok(format!("{p}OptionStringView"))
        }
        TypeRef::Sequence(element) => sequence_type(element, context, ValueUse::Param),
        _ => unsupported("encoded record view field"),
    }
}
pub fn sequence_type(
    element: &TypeRef,
    context: &RenderContext<Native>,
    use_: ValueUse,
) -> Result<String> {
    let suffix = match use_ {
        ValueUse::Param | ValueUse::ParamMut => "Slice",
        _ => "Sequence",
    };
    let p = package_pascal(context);
    let prefix = PackagePrefix::from_context(context);
    match element {
        TypeRef::String => Ok(format!("{p}String{suffix}")),
        TypeRef::Primitive(primitive) => Ok(format!("{p}{}{suffix}", primitive_stem(*primitive))),
        TypeRef::Record(id) => context
            .record(*id)
            .map(|record| format!("{}{suffix}", type_name(&prefix, record.name())))
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing sequence record",
            }),
        _ => unsupported("sequence element type"),
    }
}

pub fn direct_value_type(
    ty: &boltffi_binding::DirectValueType,
    context: &RenderContext<Native>,
) -> Result<String> {
    let prefix = PackagePrefix::from_context(context);
    match ty {
        boltffi_binding::DirectValueType::Primitive(p) => Ok(primitive_c(*p).to_owned()),
        boltffi_binding::DirectValueType::Record(id) => context
            .record(*id)
            .map(|r| type_name(&prefix, r.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing direct record",
            }),
        boltffi_binding::DirectValueType::Enum(id) => context
            .enumeration(*id)
            .map(|e| type_name(&prefix, e.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing direct enum",
            }),
        _ => unsupported("direct value type"),
    }
}

pub fn direct_vector_element_type(
    element: &boltffi_binding::DirectVectorElementType,
    context: &RenderContext<Native>,
) -> Result<String> {
    let prefix = PackagePrefix::from_context(context);
    match element {
        boltffi_binding::DirectVectorElementType::Primitive(p) => {
            Ok(primitive_c(p.primitive()).to_owned())
        }
        boltffi_binding::DirectVectorElementType::Record(id) => context
            .record(*id)
            .map(|r| type_name(&prefix, r.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing vector record",
            }),
        _ => unsupported("direct vector element type"),
    }
}
pub fn direct_vector_type(
    element: &boltffi_binding::DirectVectorElementType,
    context: &RenderContext<Native>,
    use_: ValueUse,
) -> Result<String> {
    let suffix = match use_ {
        ValueUse::Param => "Slice",
        ValueUse::ParamMut => "MutSlice",
        _ => "Sequence",
    };
    let p = package_pascal(context);
    let prefix = PackagePrefix::from_context(context);
    match element {
        boltffi_binding::DirectVectorElementType::Primitive(v) => {
            Ok(format!("{p}{}{suffix}", primitive_stem(v.primitive())))
        }
        boltffi_binding::DirectVectorElementType::Record(id) => context
            .record(*id)
            .map(|r| format!("{}{suffix}", type_name(&prefix, r.name())))
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing vector record",
            }),
        _ => unsupported("direct vector type"),
    }
}

pub fn record_helper_name(
    id: boltffi_binding::RecordId,
    context: &RenderContext<Native>,
    op: &str,
) -> Result<String> {
    let record = context.record(id).ok_or(Error::BrokenBridgeContract {
        bridge: "c",
        invariant: "missing encoded record helper declaration",
    })?;
    Ok(format!(
        "boltffi_c_{}_{}_{}",
        package_member(context),
        op,
        Name::new(record.name()).member()
    ))
}

fn record_codec(
    record: &boltffi_binding::EncodedRecordDecl<Native>,
    context: &RenderContext<Native>,
) -> Result<String> {
    let prefix = PackagePrefix::from_context(context);
    let ty = type_name(&prefix, record.name());
    let member = Name::new(record.name()).member();
    let base = format!("boltffi_c_{}", package_member(context));
    let mut size = Code::new();
    size.line("(void)boltffi_value;");
    size.line("uintptr_t boltffi_size = 0;");
    let mut encode = Code::new();
    let mut decode = Code::new();
    decode.line("memset(boltffi_value, 0, sizeof(*boltffi_value));");
    let mut free = Code::new();
    for field in record.fields() {
        let field_name = match field.key() {
            FieldKey::Named(name) => Identifier::escape(Name::new(name).member())?.to_string(),
            _ => return unsupported("tuple encoded record fields"),
        };
        size_value(
            field.ty(),
            &format!("boltffi_value->{field_name}"),
            &mut size,
            context,
        )?;
        encode_value(
            field.ty(),
            &format!("boltffi_value->{field_name}"),
            &mut encode,
            context,
        )?;
        decode_value(
            field.ty(),
            &format!("boltffi_value->{field_name}"),
            &mut decode,
            context,
        )?;
        free_value(
            field.ty(),
            &format!("boltffi_value->{field_name}"),
            &mut free,
            context,
        )?;
    }
    size.line("return boltffi_size;");
    encode.line("return boltffi_writer->ok;");
    decode.line(format!(
        "if (!boltffi_reader->ok) {{ {base}_free_{member}(boltffi_value); return false; }}"
    ));
    decode.line("return true;");
    decode.text = decode.text.replace(
        "return false;",
        &format!("{{ {base}_free_{member}(boltffi_value); return false; }}"),
    );
    free.line("memset(boltffi_value, 0, sizeof(*boltffi_value));");
    Ok(format!(
        "static inline void {base}_free_{member}({ty} *boltffi_value);\n\
         static inline uintptr_t {base}_size_{member}(const {ty}View *boltffi_value) {{\n{size}}}\n\
         static inline bool {base}_encode_{member}(BoltFFICWireWriter *boltffi_writer, const {ty}View *boltffi_value) {{\n{encode}}}\n\
         static inline bool {base}_decode_{member}(BoltFFICWireReader *boltffi_reader, {ty} *boltffi_value) {{\n{decode}}}\n\
         static inline void {base}_free_{member}({ty} *boltffi_value) {{\n    if (boltffi_value == NULL) return;\n{free}}}\n",
    ))
}

fn wire_runtime(context: &RenderContext<Native>) -> String {
    let p = package_member(context);
    format!(
        r#"
typedef struct {{ uint8_t *ptr; uintptr_t len; uintptr_t offset; bool ok; }} BoltFFICWireWriter;
typedef struct {{ const uint8_t *ptr; uintptr_t len; uintptr_t offset; bool ok; }} BoltFFICWireReader;
static inline void boltffi_c_{p}_write(BoltFFICWireWriter *w, const void *src, uintptr_t n) {{
    if (!w->ok || n > w->len - w->offset) {{ w->ok = false; return; }}
    if (n != 0) memcpy(w->ptr + w->offset, src, n);
    w->offset += n;
}}
static inline void boltffi_c_{p}_read(BoltFFICWireReader *r, void *dst, uintptr_t n) {{
    if (!r->ok || n > r->len - r->offset) {{ r->ok = false; return; }}
    if (n != 0) memcpy(dst, r->ptr + r->offset, n);
    r->offset += n;
}}
static inline void boltffi_c_{p}_write_u8(BoltFFICWireWriter *w, uint8_t v) {{ boltffi_c_{p}_write(w, &v, 1); }}
static inline void boltffi_c_{p}_write_u32(BoltFFICWireWriter *w, uint32_t v) {{
    uint8_t b[4] = {{(uint8_t)v, (uint8_t)(v >> 8), (uint8_t)(v >> 16), (uint8_t)(v >> 24)}};
    boltffi_c_{p}_write(w, b, 4);
}}
static inline uint8_t boltffi_c_{p}_read_u8(BoltFFICWireReader *r) {{ uint8_t v = 0; boltffi_c_{p}_read(r, &v, 1); return v; }}
static inline uint32_t boltffi_c_{p}_read_u32(BoltFFICWireReader *r) {{
    uint8_t b[4] = {{0,0,0,0}}; boltffi_c_{p}_read(r, b, 4);
    return (uint32_t)b[0] | ((uint32_t)b[1] << 8) | ((uint32_t)b[2] << 16) | ((uint32_t)b[3] << 24);
}}
static inline void boltffi_c_{p}_write_le16(BoltFFICWireWriter *w, uint16_t v) {{
    uint8_t b[2] = {{(uint8_t)v, (uint8_t)(v >> 8)}};
    boltffi_c_{p}_write(w, b, 2);
}}
static inline uint16_t boltffi_c_{p}_read_le16(BoltFFICWireReader *r) {{
    uint8_t b[2] = {{0,0}}; boltffi_c_{p}_read(r, b, 2);
    return (uint16_t)b[0] | ((uint16_t)b[1] << 8);
}}
static inline int8_t boltffi_c_{p}_read_i8(BoltFFICWireReader *r) {{
    uint8_t u = boltffi_c_{p}_read_u8(r); int8_t v; memcpy(&v, &u, 1); return v;
}}
static inline int16_t boltffi_c_{p}_read_i16(BoltFFICWireReader *r) {{
    uint16_t u = boltffi_c_{p}_read_le16(r); int16_t v; memcpy(&v, &u, 2); return v;
}}
static inline int32_t boltffi_c_{p}_read_i32(BoltFFICWireReader *r) {{
    uint32_t u = boltffi_c_{p}_read_u32(r); int32_t v; memcpy(&v, &u, 4); return v;
}}
static inline void boltffi_c_{p}_write_le64(BoltFFICWireWriter *w, uint64_t v) {{
    uint8_t b[8] = {{(uint8_t)v, (uint8_t)(v >> 8), (uint8_t)(v >> 16), (uint8_t)(v >> 24),
                     (uint8_t)(v >> 32), (uint8_t)(v >> 40), (uint8_t)(v >> 48), (uint8_t)(v >> 56)}};
    boltffi_c_{p}_write(w, b, 8);
}}
static inline uint64_t boltffi_c_{p}_read_le64(BoltFFICWireReader *r) {{
    uint8_t b[8] = {{0,0,0,0,0,0,0,0}}; boltffi_c_{p}_read(r, b, 8);
    return (uint64_t)b[0] | ((uint64_t)b[1] << 8) | ((uint64_t)b[2] << 16) | ((uint64_t)b[3] << 24)
         | ((uint64_t)b[4] << 32) | ((uint64_t)b[5] << 40) | ((uint64_t)b[6] << 48) | ((uint64_t)b[7] << 56);
}}
static inline int64_t boltffi_c_{p}_read_i64(BoltFFICWireReader *r) {{
    uint64_t u = boltffi_c_{p}_read_le64(r); int64_t v; memcpy(&v, &u, 8); return v;
}}
static inline void boltffi_c_{p}_write_f32(BoltFFICWireWriter *w, float v) {{
    uint32_t u; memcpy(&u, &v, 4); boltffi_c_{p}_write_u32(w, u);
}}
static inline float boltffi_c_{p}_read_f32(BoltFFICWireReader *r) {{
    uint32_t u = boltffi_c_{p}_read_u32(r); float v; memcpy(&v, &u, 4); return v;
}}
static inline void boltffi_c_{p}_write_f64(BoltFFICWireWriter *w, double v) {{
    uint64_t u; memcpy(&u, &v, 8); boltffi_c_{p}_write_le64(w, u);
}}
static inline double boltffi_c_{p}_read_f64(BoltFFICWireReader *r) {{
    uint64_t u = boltffi_c_{p}_read_le64(r); double v; memcpy(&v, &u, 8); return v;
}}
static inline void boltffi_c_{p}_write_usize(BoltFFICWireWriter *w, uintptr_t v) {{
    boltffi_c_{p}_write_le64(w, (uint64_t)v);
}}
static inline uintptr_t boltffi_c_{p}_read_usize(BoltFFICWireReader *r) {{
    return (uintptr_t)boltffi_c_{p}_read_le64(r);
}}
static inline void boltffi_c_{p}_write_isize(BoltFFICWireWriter *w, intptr_t v) {{
    boltffi_c_{p}_write_le64(w, (uint64_t)v);
}}
static inline intptr_t boltffi_c_{p}_read_isize(BoltFFICWireReader *r) {{
    return (intptr_t)boltffi_c_{p}_read_i64(r);
}}
static inline void *boltffi_c_{p}_alloc(uintptr_t count, uintptr_t size) {{
    if (count == 0) return NULL;
    if (size != 0 && count > (uintptr_t)-1 / size) return NULL;
    return calloc((size_t)count, (size_t)size);
}}
static inline bool boltffi_c_{p}_copy_string(BoltFFICWireReader *r, {package_type_prefix}String *out) {{
    uint32_t n = boltffi_c_{p}_read_u32(r); char *copy;
    if (!r->ok || (uintptr_t)n > r->len - r->offset) return false;
    copy = (char *)boltffi_c_{p}_alloc((uintptr_t)n + 1, 1);
    if (copy == NULL) {{ r->ok = false; return false; }}
    boltffi_c_{p}_read(r, copy, n); copy[n] = '\0'; out->ptr = copy; out->len = n; return r->ok;
}}
static inline bool boltffi_c_{p}_copy_bytes(BoltFFICWireReader *r, {package_type_prefix}Bytes *out) {{
    uint32_t n = boltffi_c_{p}_read_u32(r); uint8_t *copy;
    if (!r->ok || (uintptr_t)n > r->len - r->offset) return false;
    copy = (uint8_t *)boltffi_c_{p}_alloc(n, 1);
    if (n != 0 && copy == NULL) {{ r->ok = false; return false; }}
    boltffi_c_{p}_read(r, copy, n); out->ptr = copy; out->len = n; return r->ok;
}}
"#,
        package_type_prefix = package_pascal(context)
    )
}

struct Code {
    text: String,
    indent: usize,
    next: usize,
}
impl Code {
    fn new() -> Self {
        Self {
            text: String::new(),
            indent: 1,
            next: 0,
        }
    }
    fn line(&mut self, line: impl AsRef<str>) {
        self.text.push_str(&"    ".repeat(self.indent));
        self.text.push_str(line.as_ref());
        self.text.push('\n');
    }
    fn open(&mut self, line: impl AsRef<str>) {
        self.line(line);
        self.indent += 1;
    }
    fn close(&mut self, line: impl AsRef<str>) {
        self.indent -= 1;
        self.line(line);
    }
    fn var(&mut self, stem: &str) -> String {
        let v = format!("boltffi_{stem}_{}", self.next);
        self.next += 1;
        v
    }
}
impl std::fmt::Display for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

fn size_value(
    ty: &TypeRef,
    expr: &str,
    code: &mut Code,
    context: &RenderContext<Native>,
) -> Result<()> {
    match ty {
        TypeRef::Primitive(p) => code.line(format!("boltffi_size += {};", primitive_wire_size(*p))),
        TypeRef::String | TypeRef::Bytes => code.line(format!("boltffi_size += 4 + {expr}.len;")),
        TypeRef::Enum(id) => code.line(format!(
            "boltffi_size += {};",
            enum_wire_size(*id, context)?
        )),
        TypeRef::Record(id) => match context.record(*id) {
            Some(RecordDecl::Direct(record)) => code.line(format!(
                "boltffi_size += sizeof({});",
                type_name(&PackagePrefix::from_context(context), record.name())
            )),
            Some(RecordDecl::Encoded(_)) => code.line(format!(
                "boltffi_size += {}(&{expr});",
                record_helper_name(*id, context, "size")?
            )),
            _ => return unsupported("record size codec"),
        },
        TypeRef::Optional(inner) => {
            code.line("boltffi_size += 1;");
            code.open(format!("if ({expr}.has_value) {{"));
            size_value(inner, &format!("{expr}.value"), code, context)?;
            code.close("}");
        }
        TypeRef::Sequence(element) => {
            code.line("boltffi_size += 4;");
            let i = code.var("i");
            code.open(format!(
                "for (uintptr_t {i} = 0; {i} < {expr}.len; ++{i}) {{"
            ));
            size_value(element, &format!("{expr}.ptr[{i}]"), code, context)?;
            code.close("}");
        }
        _ => return unsupported("wire size type"),
    }
    Ok(())
}

fn encode_value(
    ty: &TypeRef,
    expr: &str,
    code: &mut Code,
    context: &RenderContext<Native>,
) -> Result<()> {
    let p = package_member(context);
    match ty {
        TypeRef::Primitive(primitive) => {
            code.line(wire_write_stmt(*primitive, expr, &p));
        }
        TypeRef::String | TypeRef::Bytes => {
            code.line(format!(
                "boltffi_c_{p}_write_u32(boltffi_writer, (uint32_t){expr}.len);"
            ));
            code.line(format!(
                "boltffi_c_{p}_write(boltffi_writer, {expr}.ptr, {expr}.len);"
            ));
        }
        TypeRef::Enum(id) => {
            let repr = match context.enumeration(*id) {
                Some(EnumDecl::CStyle(enumeration)) => enumeration.repr().primitive(),
                _ => return unsupported("data enum codec"),
            };
            code.line(wire_write_stmt(repr, expr, &p));
        }
        TypeRef::Record(id) => match context.record(*id) {
            Some(RecordDecl::Direct(_)) => code.line(format!(
                "boltffi_c_{p}_write(boltffi_writer, &{expr}, sizeof({expr}));"
            )),
            Some(RecordDecl::Encoded(_)) => code.line(format!(
                "{}(boltffi_writer, &{expr});",
                record_helper_name(*id, context, "encode")?
            )),
            _ => return unsupported("record encode codec"),
        },
        TypeRef::Optional(inner) => {
            code.line(format!(
                "boltffi_c_{p}_write_u8(boltffi_writer, {expr}.has_value ? 1 : 0);"
            ));
            code.open(format!("if ({expr}.has_value) {{"));
            encode_value(inner, &format!("{expr}.value"), code, context)?;
            code.close("}");
        }
        TypeRef::Sequence(element) => {
            code.line(format!(
                "boltffi_c_{p}_write_u32(boltffi_writer, (uint32_t){expr}.len);"
            ));
            let i = code.var("i");
            code.open(format!("for (uintptr_t {i}=0; {i} < {expr}.len; ++{i}) {{"));
            encode_value(element, &format!("{expr}.ptr[{i}]"), code, context)?;
            code.close("}");
        }
        _ => return unsupported("wire encode type"),
    }
    Ok(())
}

fn decode_value(
    ty: &TypeRef,
    expr: &str,
    code: &mut Code,
    context: &RenderContext<Native>,
) -> Result<()> {
    let p = package_member(context);
    match ty {
        TypeRef::Primitive(primitive) => {
            code.line(wire_read_stmt(*primitive, expr, &p));
        }
        TypeRef::String => code.line(format!(
            "if (!boltffi_c_{p}_copy_string(boltffi_reader, &{expr})) return false;"
        )),
        TypeRef::Bytes => code.line(format!(
            "if (!boltffi_c_{p}_copy_bytes(boltffi_reader, &{expr})) return false;"
        )),
        TypeRef::Enum(id) => {
            let (repr, enum_ty) = match context.enumeration(*id) {
                Some(EnumDecl::CStyle(enumeration)) => (
                    enumeration.repr().primitive(),
                    type_name(&PackagePrefix::from_context(context), enumeration.name()),
                ),
                _ => return unsupported("data enum codec"),
            };
            code.line(format!("{expr} = ({enum_ty}){};", wire_read_expr(repr, &p)));
        }
        TypeRef::Record(id) => match context.record(*id) {
            Some(RecordDecl::Direct(_)) => code.line(format!(
                "boltffi_c_{p}_read(boltffi_reader, &{expr}, sizeof({expr}));"
            )),
            Some(RecordDecl::Encoded(_)) => code.line(format!(
                "if (!{}(boltffi_reader, &{expr})) return false;",
                record_helper_name(*id, context, "decode")?
            )),
            _ => return unsupported("record decode codec"),
        },
        TypeRef::Optional(inner) => {
            let tag = code.var("tag");
            code.line(format!(
                "uint8_t {tag}=boltffi_c_{p}_read_u8(boltffi_reader);"
            ));
            code.line(format!("{expr}.has_value = ({tag} == 1);"));
            code.open(format!("if ({tag} == 1) {{"));
            decode_value(inner, &format!("{expr}.value"), code, context)?;
            code.close(format!(
                "}} else if ({tag} != 0) {{ boltffi_reader->ok=false; return false; }}"
            ));
        }
        TypeRef::Sequence(element) => {
            let n = code.var("count");
            let i = code.var("i");
            let elem_ty = value_type(element, context, ValueUse::Field)?;
            code.line(format!(
                "uint32_t {n}=boltffi_c_{p}_read_u32(boltffi_reader);"
            ));
            code.line(format!(
                "{expr}.ptr=({elem_ty} *)boltffi_c_{p}_alloc({n}, sizeof({elem_ty}));"
            ));
            code.line(format!("{expr}.len={n};"));
            code.line(format!(
                "if ({n} != 0 && {expr}.ptr == NULL) {{ boltffi_reader->ok=false; return false; }}"
            ));
            code.open(format!("for (uintptr_t {i}=0; {i} < {n}; ++{i}) {{"));
            decode_value(element, &format!("{expr}.ptr[{i}]"), code, context)?;
            code.close("}");
        }
        _ => return unsupported("wire decode type"),
    }
    Ok(())
}

fn free_value(
    ty: &TypeRef,
    expr: &str,
    code: &mut Code,
    context: &RenderContext<Native>,
) -> Result<()> {
    match ty {
        TypeRef::String | TypeRef::Bytes => {
            code.line(format!("free((void *){expr}.ptr);"));
            code.line(format!("{expr}.ptr=NULL; {expr}.len=0;"));
        }
        TypeRef::Record(id) => {
            if matches!(context.record(*id), Some(RecordDecl::Encoded(_))) {
                code.line(format!(
                    "{}(&{expr});",
                    record_helper_name(*id, context, "free")?
                ));
            }
        }
        TypeRef::Optional(inner) => {
            code.open(format!("if ({expr}.has_value) {{"));
            free_value(inner, &format!("{expr}.value"), code, context)?;
            code.close("}");
            code.line(format!("{expr}.has_value=false;"));
        }
        TypeRef::Sequence(element) => {
            let i = code.var("i");
            code.open(format!("for (uintptr_t {i}=0; {i} < {expr}.len; ++{i}) {{"));
            free_value(element, &format!("{expr}.ptr[{i}]"), code, context)?;
            code.close("}");
            code.line(format!(
                "free((void *){expr}.ptr); {expr}.ptr=NULL; {expr}.len=0;"
            ));
        }
        TypeRef::Primitive(_) | TypeRef::Enum(_) => {}
        _ => return unsupported("wire free type"),
    }
    Ok(())
}

/// Emits one fixed-width little-endian wire write statement for a primitive.
pub(super) fn wire_write_stmt(primitive: Primitive, expr: &str, package: &str) -> String {
    let call = |name: &str, cast: &str| {
        format!("boltffi_c_{package}_{name}(boltffi_writer, ({cast}){expr});")
    };
    match primitive {
        Primitive::Bool | Primitive::I8 | Primitive::U8 => call("write_u8", "uint8_t"),
        Primitive::I16 | Primitive::U16 => call("write_le16", "uint16_t"),
        Primitive::I32 | Primitive::U32 => call("write_u32", "uint32_t"),
        Primitive::I64 | Primitive::U64 => call("write_le64", "uint64_t"),
        Primitive::F32 => format!("boltffi_c_{package}_write_f32(boltffi_writer, {expr});"),
        Primitive::F64 => format!("boltffi_c_{package}_write_f64(boltffi_writer, {expr});"),
        Primitive::ISize => format!("boltffi_c_{package}_write_isize(boltffi_writer, {expr});"),
        Primitive::USize => format!("boltffi_c_{package}_write_usize(boltffi_writer, {expr});"),
        _ => format!("(void){expr};"),
    }
}

/// Emits the read expression for a primitive's fixed-width little-endian wire form.
pub(super) fn wire_read_expr(primitive: Primitive, package: &str) -> String {
    match primitive {
        Primitive::Bool | Primitive::U8 => format!("boltffi_c_{package}_read_u8(boltffi_reader)"),
        Primitive::I8 => format!("boltffi_c_{package}_read_i8(boltffi_reader)"),
        Primitive::I16 => format!("boltffi_c_{package}_read_i16(boltffi_reader)"),
        Primitive::U16 => format!("boltffi_c_{package}_read_le16(boltffi_reader)"),
        Primitive::I32 => format!("boltffi_c_{package}_read_i32(boltffi_reader)"),
        Primitive::U32 => format!("boltffi_c_{package}_read_u32(boltffi_reader)"),
        Primitive::I64 => format!("boltffi_c_{package}_read_i64(boltffi_reader)"),
        Primitive::U64 => format!("boltffi_c_{package}_read_le64(boltffi_reader)"),
        Primitive::F32 => format!("boltffi_c_{package}_read_f32(boltffi_reader)"),
        Primitive::F64 => format!("boltffi_c_{package}_read_f64(boltffi_reader)"),
        Primitive::ISize => format!("boltffi_c_{package}_read_isize(boltffi_reader)"),
        Primitive::USize => format!("boltffi_c_{package}_read_usize(boltffi_reader)"),
        _ => "0".to_owned(),
    }
}

fn wire_read_stmt(primitive: Primitive, expr: &str, package: &str) -> String {
    format!("{expr} = {};", wire_read_expr(primitive, package))
}

fn free_helpers(
    bindings: &Bindings<Native>,
    context: &RenderContext<Native>,
    record_ids: &std::collections::HashSet<boltffi_binding::RecordId>,
) -> Result<String> {
    let mut out = String::new();
    let p = package_member(context);
    let package_type_prefix = package_pascal(context);
    out.push_str(&format!("static inline {package_type_prefix}StringView {p}_string_view(const char *ptr, uintptr_t len) {{ {package_type_prefix}StringView v={{ptr,len}}; return v; }}\nstatic inline {package_type_prefix}BytesView {p}_bytes_view(const uint8_t *ptr, uintptr_t len) {{ {package_type_prefix}BytesView v={{ptr,len}}; return v; }}\n"));
    out.push_str(&format!("static inline void {p}_string_free({package_type_prefix}String *v) {{ if (v == NULL) return; free((void *)v->ptr); v->ptr=NULL; v->len=0; }}\nstatic inline void {p}_string_sequence_free({package_type_prefix}StringSequence *v) {{ if (v == NULL) return; for (uintptr_t i=0;i<v->len;++i) {p}_string_free(&v->ptr[i]); free((void *)v->ptr); v->ptr=NULL; v->len=0; }}\nstatic inline void {p}_bytes_free({package_type_prefix}Bytes *v) {{ if (v == NULL) return; free((void *)v->ptr); v->ptr=NULL; v->len=0; }}\n"));
    for primitive in all_primitives() {
        let stem = primitive_stem(*primitive);
        let member = stem.to_ascii_lowercase();
        out.push_str(&format!("static inline void {p}_{member}_sequence_free({package_type_prefix}{stem}Sequence *v) {{ if (v == NULL) return; free((void *)v->ptr); v->ptr=NULL; v->len=0; }}\n"));
    }
    let prefix = PackagePrefix::from_context(context);
    for decl in bindings.decls() {
        if let boltffi_binding::DeclarationRef::Record(record) =
            boltffi_binding::DeclarationRef::from(decl)
        {
            if !record_ids.contains(&record.id()) {
                continue;
            }
            let ty = type_name(&prefix, record.name());
            let m = Name::new(record.name()).member();
            if matches!(record, RecordDecl::Encoded(encoded) if encoded.fields().iter().any(|field| type_owns(field.ty(), context)))
            {
                out.push_str(&format!(
                    "static inline void {p}_{m}_free({ty} *v) {{ {}(v); }}\n",
                    record_helper_name(record.id(), context, "free")?
                ));
            }
            out.push_str(&format!("static inline void {p}_{m}_sequence_free({ty}Sequence *v) {{ if (v == NULL) return;"));
            if matches!(record, RecordDecl::Encoded(_)) {
                out.push_str(&format!(
                    " for (uintptr_t i=0;i<v->len;++i) {}(&v->ptr[i]);",
                    record_helper_name(record.id(), context, "free")?
                ));
            }
            out.push_str(" free((void *)v->ptr); v->ptr=NULL; v->len=0; }\n");
        }
    }
    Ok(out)
}

fn type_owns(ty: &TypeRef, context: &RenderContext<Native>) -> bool {
    match ty {
        TypeRef::String | TypeRef::Bytes | TypeRef::Sequence(_) => true,
        TypeRef::Optional(inner) => type_owns(inner, context),
        TypeRef::Record(id) => match context.record(*id) {
            Some(RecordDecl::Encoded(r)) => r.fields().iter().any(|f| type_owns(f.ty(), context)),
            _ => false,
        },
        _ => false,
    }
}

fn enum_wire_size(id: boltffi_binding::EnumId, context: &RenderContext<Native>) -> Result<usize> {
    match context.enumeration(id) {
        Some(EnumDecl::CStyle(e)) => Ok(primitive_wire_size(e.repr().primitive())),
        _ => unsupported("data enum codec"),
    }
}
fn primitive_wire_size(p: Primitive) -> usize {
    match p {
        Primitive::Bool | Primitive::I8 | Primitive::U8 => 1,
        Primitive::I16 | Primitive::U16 => 2,
        Primitive::I32 | Primitive::U32 | Primitive::F32 => 4,
        Primitive::I64 | Primitive::U64 | Primitive::ISize | Primitive::USize | Primitive::F64 => 8,
        _ => 0,
    }
}
pub fn primitive_c(p: Primitive) -> &'static str {
    match p {
        Primitive::Bool => "bool",
        Primitive::I8 => "int8_t",
        Primitive::U8 => "uint8_t",
        Primitive::I16 => "int16_t",
        Primitive::U16 => "uint16_t",
        Primitive::I32 => "int32_t",
        Primitive::U32 => "uint32_t",
        Primitive::I64 => "int64_t",
        Primitive::U64 => "uint64_t",
        Primitive::ISize => "intptr_t",
        Primitive::USize => "uintptr_t",
        Primitive::F32 => "float",
        Primitive::F64 => "double",
        _ => "void",
    }
}
fn primitive_stem(p: Primitive) -> &'static str {
    match p {
        Primitive::Bool => "Bool",
        Primitive::I8 => "I8",
        Primitive::U8 => "U8",
        Primitive::I16 => "I16",
        Primitive::U16 => "U16",
        Primitive::I32 => "I32",
        Primitive::U32 => "U32",
        Primitive::I64 => "I64",
        Primitive::U64 => "U64",
        Primitive::ISize => "ISize",
        Primitive::USize => "USize",
        Primitive::F32 => "F32",
        Primitive::F64 => "F64",
        _ => "Unsupported",
    }
}
fn all_primitives() -> &'static [Primitive] {
    &[
        Primitive::Bool,
        Primitive::I8,
        Primitive::U8,
        Primitive::I16,
        Primitive::U16,
        Primitive::I32,
        Primitive::U32,
        Primitive::I64,
        Primitive::U64,
        Primitive::ISize,
        Primitive::USize,
        Primitive::F32,
        Primitive::F64,
    ]
}
fn type_name(prefix: &PackagePrefix, name: &boltffi_binding::CanonicalName) -> String {
    prefix.type_name(&Name::new(name).r#type())
}
fn package_pascal(context: &RenderContext<Native>) -> String {
    Name::new(context.bindings().package().name()).r#type()
}
fn package_member(context: &RenderContext<Native>) -> String {
    Name::new(context.bindings().package().name()).member()
}
fn unsupported<T>(shape: &'static str) -> Result<T> {
    Err(Error::UnsupportedTarget { target: "c", shape })
}
