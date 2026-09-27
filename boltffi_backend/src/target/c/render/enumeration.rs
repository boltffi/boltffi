//! Ergonomic package-prefixed C-style enum callables.
use super::{callable, prefix::PackagePrefix};
use crate::{
    bridge::c,
    core::{Emitted, RenderContext, Result},
    target::c::name_style::Name,
};
use boltffi_binding::{DataEnumDecl, EnumDecl, Native, TypeRef};
pub fn render(
    decl: &EnumDecl<Native>,
    bridge: &c::CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Emitted> {
    let prefix = PackagePrefix::from_context(context);
    let owner = Name::new(decl.name()).member();
    let ty = prefix.type_name(decl.name());
    let mut out = match decl {
        EnumDecl::Data(enumeration) => Emitted::primary(codec(enumeration, context)?),
        _ => Emitted::primary(""),
    };
    for init in decl.initializers() {
        let abi = bridge.function(init.symbol())?;
        let name = prefix.member(&format!("{}_{}", owner, Name::new(init.name()).member()));
        out.append(callable::render(
            abi,
            init.callable(),
            &name,
            &format!("{}{}", ty, Name::new(init.name()).r#type()),
            callable::Receiver::None,
            context,
        )?)
    }
    for method in decl.methods() {
        let abi = bridge.function(method.target())?;
        let name = prefix.member(&format!("{}_{}", owner, Name::new(method.name()).member()));
        let receiver = match method.callable().receiver() {
            Some(receive) => match decl {
                EnumDecl::Data(enumeration) => callable::Receiver::Encoded {
                    ty: TypeRef::Enum(enumeration.id()),
                    codec: enumeration.read(),
                    receive,
                },
                _ => callable::Receiver::DirectValue {
                    c_type: &ty,
                    receive,
                },
            },
            None => callable::Receiver::None,
        };
        out.append(callable::render(
            abi,
            method.callable(),
            &name,
            &format!("{}{}", ty, Name::new(method.name()).r#type()),
            receiver,
            context,
        )?)
    }
    Ok(out)
}

pub fn helper_name(
    id: boltffi_binding::EnumId,
    context: &RenderContext<Native>,
    operation: &str,
) -> Result<String> {
    let enumeration = context
        .enumeration(id)
        .ok_or(crate::core::Error::BrokenBridgeContract {
            bridge: "c",
            invariant: "missing enum codec declaration",
        })?;
    Ok(format!(
        "boltffi_c_{}_{}_{}",
        Name::new(context.bindings().package().name()).member(),
        operation,
        Name::new(enumeration.name()).member()
    ))
}

pub fn variant_tag(
    enumeration: &DataEnumDecl<Native>,
    variant: &boltffi_binding::DataVariantDecl,
    context: &RenderContext<Native>,
) -> String {
    PackagePrefix::from_context(context).constant(&format!(
        "{}_{}",
        Name::new(enumeration.name()).constant(),
        Name::new(variant.name()).constant()
    ))
}

fn codec(enumeration: &DataEnumDecl<Native>, context: &RenderContext<Native>) -> Result<String> {
    let ty = TypeRef::Enum(enumeration.id());
    let owned_type = super::surface::value_type(&ty, context, super::surface::ValueUse::Return)?;
    let package = Name::new(context.bindings().package().name()).member();
    let decode_name = helper_name(enumeration.id(), context, "decode")?;
    let free_name = helper_name(enumeration.id(), context, "free")?;
    let encoding = [super::surface::ValueUse::Param, super::surface::ValueUse::Return]
        .into_iter()
        .map(|usage| -> Result<String> {
            let view_type = super::surface::value_type(&ty, context, usage)?;
            let size_name = helper_name(enumeration.id(), context, &usage.codec_operation("size"))?;
            let encode_name = helper_name(enumeration.id(), context, &usage.codec_operation("encode"))?;
            let mut sizes = String::new();
            let mut writes = String::new();
            enumeration.variants().iter().try_for_each(|variant| -> Result<()> {
                let tag = variant_tag(enumeration, variant, context);
                let member = crate::bridge::c::Identifier::escape(Name::new(variant.name()).member())?;
                sizes.push_str(&format!("case {tag}: {{\n"));
                writes.push_str(&format!("case {tag}: {{\n"));
                variant.payload().fields().iter().try_for_each(|field| -> Result<()> {
                    let field_name = Name::field(field.key())?;
                    let value = format!("value->data.{member}.{field_name}");
                    let mut sizer = crate::target::c::codec::size::Sizer::new(context, &value, usage);
                    sizes.push_str(&sizer.measure(&field.read().write_self_value(), "size")?);
                    sizes.push('\n');
                    let mut writer = crate::target::c::codec::write::Writer::new(context, "writer", &value, usage);
                    writes.push_str(&writer.encode(&field.read().write_self_value())?);
                    writes.push('\n');
                    Ok(())
                })?;
                sizes.push_str("break;\n}\n");
                writes.push_str("break;\n}\n");
                Ok(())
            })?;
            Ok(format!("static inline uintptr_t {size_name}(const {view_type} *value) {{\nuintptr_t size = 4;\nswitch (value->tag) {{\n{sizes}default: return 0;\n}}\nreturn size;\n}}\n\
                static inline bool {encode_name}(BoltFFICWireWriter *writer, const {view_type} *value) {{\nboltffi_c_{package}_write_u32(writer, (uint32_t)value->tag);\nswitch (value->tag) {{\n{writes}default: writer->ok = false; break;\n}}\nreturn writer->ok;\n}}\n"))
        })
        .collect::<Result<Vec<_>>>()?
        .join("\n");
    let mut reads = String::new();
    let mut releases = String::new();
    enumeration
        .variants()
        .iter()
        .try_for_each(|variant| -> Result<()> {
            let tag = variant_tag(enumeration, variant, context);
            let member = crate::bridge::c::Identifier::escape(Name::new(variant.name()).member())?
                .to_string();
            reads.push_str(&format!("case {tag}: {{\n"));
            releases.push_str(&format!("case {tag}: {{\n"));
            variant
                .payload()
                .fields()
                .iter()
                .try_for_each(|field| -> Result<()> {
                    let field_name = Name::field(field.key())?.to_string();
                    let value = format!("value->data.{member}.{field_name}");
                    let mut reader = crate::target::c::codec::read::Reader::new(context, "reader");
                    reads.push_str(&format!(
                        "{{\n{}\n}}\n",
                        reader.decode(field.read(), &value)?
                    ));
                    releases.push_str(&super::surface::free_owned(field.ty(), &value, context)?);
                    Ok(())
                })?;
            reads.push_str("break;\n}\n");
            releases.push_str("break;\n}\n");
            Ok(())
        })?;
    Ok(format!(
        "{encoding}\n\
         static inline bool {decode_name}(BoltFFICWireReader *reader, {owned_type} *value) {{\nmemset(value, 0, sizeof(*value));\nvalue->tag = ({owned_type}Tag)boltffi_c_{package}_read_u32(reader);\nswitch (value->tag) {{\n{reads}default: reader->ok = false; break;\n}}\nif (!reader->ok) {{ {free_name}(value); }}\nreturn reader->ok;\n}}\n\
         static inline void {free_name}({owned_type} *value) {{\nif (value == NULL) return;\nswitch (value->tag) {{\n{releases}default: break;\n}}\nmemset(value, 0, sizeof(*value));\n}}\n"
    ))
}
