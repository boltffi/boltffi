use std::collections::{HashMap, HashSet};

use boltffi_binding::{
    BuiltinType, ClosureReturn, DeclarationId, DeclarationRef, DirectValueType,
    DirectVectorElementType, Direction, ErrorChannel, HandlePresence, HandleTarget, Native,
    ParamDirection, ParamPlanRender, Primitive, RecordDecl, ReturnPlanRender, ReturnValueSlot,
    TypeRef, native,
};

use crate::{
    bridge::c::Identifier,
    core::{Error, RenderContext, Result},
};

use super::{
    name_style::Name,
    render::surface::{self, ValueUse},
};

pub struct ValueTypes<'context, 'bindings> {
    context: &'context RenderContext<'bindings, Native>,
    declared: HashMap<String, TypeRef>,
    definitions: String,
    destructors: String,
}

impl<'context, 'bindings> ValueTypes<'context, 'bindings> {
    pub fn render(
        context: &'context RenderContext<'bindings, Native>,
        rendered: &HashSet<DeclarationId>,
    ) -> Result<Self> {
        let mut types = Self {
            context,
            declared: HashMap::new(),
            definitions: String::new(),
            destructors: String::new(),
        };
        [TypeRef::String, TypeRef::Bytes]
            .iter()
            .try_for_each(|ty| types.declare_owned_and_borrowed(ty))?;
        context
            .bindings()
            .decls()
            .iter()
            .try_for_each(|declaration| -> Result<()> {
                let reference = DeclarationRef::from(declaration);
                if !rendered.contains(&reference.id()) {
                    return Ok(());
                }
                match reference {
                    DeclarationRef::Record(record) => {
                        types.declare_owned_and_borrowed(&TypeRef::Record(record.id()))?
                    }
                    DeclarationRef::Enum(enumeration) => {
                        types.declare_owned_and_borrowed(&TypeRef::Enum(enumeration.id()))?
                    }
                    DeclarationRef::Class(class) => {
                        types.declare_owned_and_borrowed(&TypeRef::Class(class.id()))?
                    }
                    DeclarationRef::Callback(callback) => {
                        let name = super::render::prefix::PackagePrefix::from_context(context)
                            .type_name(callback.name());
                        types.register(&name, &TypeRef::Callback(callback.id()))?;
                        types.register(
                            &format!("{name}Handle"),
                            &TypeRef::Callback(callback.id()),
                        )?;
                    }
                    DeclarationRef::CustomType(custom) => {
                        types.declare_owned_and_borrowed(&TypeRef::Custom(custom.id()))?
                    }
                    _ => {}
                }
                declaration
                    .exported_callables()
                    .try_for_each(|callable| -> Result<()> {
                        callable
                            .params()
                            .iter()
                            .filter_map(|parameter| parameter.payload().as_value())
                            .try_for_each(|plan| plan.render_with(&mut types))?;
                        callable.returns().plan().render_with(&mut types)?;
                        if let ErrorChannel::Encoded { ty, .. } = callable.error().channel() {
                            types.declare_owned_and_borrowed(ty)?;
                        }
                        Ok(())
                    })?;
                declaration
                    .imported_callables()
                    .try_for_each(|callable| -> Result<()> {
                        callable
                            .params()
                            .iter()
                            .filter_map(|parameter| parameter.payload().as_value())
                            .try_for_each(|plan| plan.render_with(&mut types))?;
                        callable.returns().plan().render_with(&mut types)?;
                        if let ErrorChannel::Encoded { ty, .. } = callable.error().channel() {
                            types.declare_owned_and_borrowed(ty)?;
                        }
                        Ok(())
                    })
            })?;
        Ok(types)
    }

    pub fn definitions(&self) -> &str {
        &self.definitions
    }

    pub fn destructors(&self) -> &str {
        &self.destructors
    }

    fn declare_owned_and_borrowed(&mut self, ty: &TypeRef) -> Result<()> {
        self.declare(ty, ValueUse::Param)?;
        self.declare(ty, ValueUse::Return)?;
        Ok(())
    }

    fn register(&mut self, name: &str, ty: &TypeRef) -> Result<bool> {
        let representation = match ty {
            TypeRef::Builtin(BuiltinType::Url) => &TypeRef::String,
            _ => ty,
        };
        match self.declared.get(name) {
            Some(existing) if existing == representation => Ok(false),
            Some(_) => Err(Error::CNameCollision {
                name: name.to_owned(),
            }),
            None => {
                self.declared
                    .insert(name.to_owned(), representation.clone());
                Ok(true)
            }
        }
    }

    fn declare(&mut self, ty: &TypeRef, usage: ValueUse) -> Result<String> {
        let name = surface::value_type(ty, self.context, usage)?;
        if !self.register(&name, ty)? {
            return Ok(name);
        }
        match ty {
            TypeRef::Record(id) => {
                if let Some(RecordDecl::Encoded(record)) = self.context.record(*id) {
                    let mut fields = String::new();
                    record.fields().iter().try_for_each(|field| -> Result<()> {
                        let field_type = self.declare(field.ty(), usage)?;
                        let field_name = Name::field(field.key())?.to_string();
                        fields.push_str(&format!("    {field_type} {field_name};\n"));
                        Ok(())
                    })?;
                    self.definitions
                        .push_str(&format!("struct {name} {{\n{fields}}};\n"));
                }
            }
            TypeRef::Optional(inner) => {
                let inner_type = self.declare(inner, usage)?;
                let predefined = match inner.as_ref() {
                    TypeRef::Primitive(_) | TypeRef::String => true,
                    TypeRef::Record(id) => {
                        matches!(self.context.record(*id), Some(RecordDecl::Direct(_)))
                    }
                    TypeRef::Enum(id) => matches!(
                        self.context.enumeration(*id),
                        Some(boltffi_binding::EnumDecl::CStyle(_))
                    ),
                    _ => false,
                };
                if !predefined {
                    self.definitions.push_str(&format!(
                        "typedef struct {{ bool has_value; {inner_type} value; }} {name};\n"
                    ));
                }
            }
            TypeRef::Sequence(element) => {
                let element_type = self.declare(element, usage)?;
                if !matches!(
                    element.as_ref(),
                    TypeRef::Primitive(_) | TypeRef::String | TypeRef::Record(_)
                ) {
                    let qualifier = if matches!(usage, ValueUse::Param) {
                        "const "
                    } else {
                        ""
                    };
                    self.definitions.push_str(&format!(
                        "typedef struct {{ {qualifier}{element_type} *ptr; uintptr_t len; }} {name};\n"
                    ));
                }
            }
            TypeRef::Tuple(elements) => {
                let mut fields = String::new();
                elements
                    .iter()
                    .enumerate()
                    .try_for_each(|(index, element)| -> Result<()> {
                        fields.push_str(&format!(
                            "{} field_{index};\n",
                            self.declare(element, usage)?
                        ));
                        Ok(())
                    })?;
                if elements.is_empty() {
                    fields.push_str("uint8_t unit;");
                }
                self.definitions
                    .push_str(&format!("typedef struct {{\n{fields}}} {name};\n"));
            }
            TypeRef::Result { ok, err } => {
                let success = self.declare(ok, usage)?;
                let error = self.declare(err, usage)?;
                self.definitions.push_str(&format!("typedef struct {{ bool ok; union {{ {success} value; {error} error; }} data; }} {name};\n"));
            }
            TypeRef::Map { key, value } => {
                self.register(&format!("{name}Entry"), ty)?;
                let key = self.declare(key, usage)?;
                let value = self.declare(value, usage)?;
                let qualifier = if matches!(usage, ValueUse::Param) {
                    "const "
                } else {
                    ""
                };
                self.definitions.push_str(&format!(
                    "typedef struct {{ {key} key; {value} value; }} {name}Entry;\n"
                ));
                self.definitions.push_str(&format!(
                    "typedef struct {{ {qualifier}{name}Entry *ptr; uintptr_t len; }} {name};\n"
                ));
            }
            TypeRef::Enum(id) => {
                if let Some(boltffi_binding::EnumDecl::Data(enumeration)) =
                    self.context.enumeration(*id)
                {
                    let owned_name = surface::value_type(ty, self.context, ValueUse::Return)?;
                    let tag_name = format!("{owned_name}Tag");
                    if self.register(&tag_name, ty)? {
                        self.definitions.push_str(&format!(
                            "typedef ___{} {tag_name};\n",
                            Name::new(enumeration.name()).r#type()
                        ));
                        enumeration.variants().iter().for_each(|variant| {
                            let public = super::render::enumeration::variant_tag(
                                enumeration,
                                variant,
                                self.context,
                            );
                            let raw = format!(
                                "{}_{}",
                                Name::new(enumeration.name()).constant(),
                                Name::new(variant.name()).constant()
                            );
                            if public != raw {
                                self.definitions
                                    .push_str(&format!("#define {public} {raw}\n"));
                            }
                        });
                    }
                    let mut variants = String::new();
                    enumeration
                        .variants()
                        .iter()
                        .try_for_each(|variant| -> Result<()> {
                            if variant.payload().is_unit() {
                                return Ok(());
                            }
                            variants.push_str("struct {\n");
                            variant.payload().fields().iter().try_for_each(
                                |field| -> Result<()> {
                                    let field_type = self.declare(field.ty(), usage)?;
                                    let field_name = Name::field(field.key())?.to_string();
                                    variants.push_str(&format!("{field_type} {field_name};\n"));
                                    Ok(())
                                },
                            )?;
                            variants.push_str(&format!(
                                "}} {};\n",
                                Identifier::escape(Name::new(variant.name()).member())?
                            ));
                            Ok(())
                        })?;
                    self.definitions.push_str(&format!(
                        "struct {name} {{ {tag_name} tag; union {{\n{variants}}} data; }};\n"
                    ));
                }
            }
            TypeRef::Builtin(kind) => {
                let fields = match kind {
                    BuiltinType::Duration => "uint64_t seconds; uint32_t nanoseconds;",
                    BuiltinType::SystemTime => "int64_t seconds; uint32_t nanoseconds;",
                    BuiltinType::Uuid => "uint8_t bytes[16];",
                    BuiltinType::Url => return Ok(name),
                };
                self.definitions
                    .push_str(&format!("typedef struct {{ {fields} }} {name};\n"));
            }
            TypeRef::Custom(id) => {
                let custom = self
                    .context
                    .custom_type(*id)
                    .ok_or(Error::BrokenBridgeContract {
                        bridge: "c",
                        invariant: "missing custom C type definition",
                    })?;
                let representation = self.declare(custom.representation(), usage)?;
                self.definitions
                    .push_str(&format!("typedef {representation} {name};\n"));
            }
            TypeRef::Primitive(_) | TypeRef::String | TypeRef::Bytes | TypeRef::Class(_) => {}
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "c",
                    shape: "C value declaration",
                });
            }
        }
        let needs_destructor = match ty {
            TypeRef::Optional(_)
            | TypeRef::Tuple(_)
            | TypeRef::Result { .. }
            | TypeRef::Map { .. }
            | TypeRef::Custom(_)
            | TypeRef::Enum(_) => surface::type_owns(ty, self.context),
            TypeRef::Sequence(element) => !matches!(
                element.as_ref(),
                TypeRef::Primitive(_) | TypeRef::String | TypeRef::Record(_)
            ),
            _ => false,
        };
        if matches!(usage, ValueUse::Return) && needs_destructor {
            let free_name = surface::value_free_name(ty, self.context)?;
            let release = surface::free_owned(ty, "(*value)", self.context)?;
            self.destructors.push_str(&format!(
                "static inline void {free_name}({name} *value) {{\n\
                 if (value == NULL) return;\n{release}\nmemset(value, 0, sizeof(*value));\n}}\n"
            ));
        }
        Ok(name)
    }
}

impl<'plan, D: Direction> ParamPlanRender<'plan, Native, D> for ValueTypes<'_, '_> {
    type Output = Result<()>;

    fn direct(&mut self, _: &'plan DirectValueType, _: D::Receive) -> Self::Output {
        Ok(())
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        _: &'plan D::Codec,
        _: native::BufferShape,
        _: D::Receive,
    ) -> Self::Output {
        self.declare_owned_and_borrowed(ty)
    }

    fn handle(
        &mut self,
        _: &'plan HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
        _: D::Receive,
    ) -> Self::Output {
        Ok(())
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(())
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType, _: D::Receive) -> Self::Output {
        Ok(())
    }
}

impl<'plan, D: Direction> ReturnPlanRender<'plan, Native, D> for ValueTypes<'_, '_>
where
    D::Opposite: ParamDirection<Native>,
{
    type Output = Result<()>;

    fn void(&mut self) -> Self::Output {
        Ok(())
    }

    fn direct(&mut self, _: ReturnValueSlot, _: &'plan DirectValueType) -> Self::Output {
        Ok(())
    }

    fn encoded(
        &mut self,
        _: ReturnValueSlot,
        ty: &'plan TypeRef,
        _: &'plan D::Codec,
        _: native::BufferShape,
    ) -> Self::Output {
        self.declare_owned_and_borrowed(ty)
    }

    fn handle(
        &mut self,
        _: ReturnValueSlot,
        _: &'plan HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        Ok(())
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(())
    }

    fn scalar_option_enum(&mut self, _: Primitive, target: &'plan TypeRef) -> Self::Output {
        self.declare_owned_and_borrowed(&TypeRef::Optional(Box::new(target.clone())))
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {
        Ok(())
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, D>) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "c",
            shape: "closure value declarations",
        })
    }
}
