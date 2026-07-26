use askama::Template;
use boltffi_binding::{
    CStyleEnumDecl, CanonicalName, DataEnumDecl, DataVariantPayload, DirectValueType, EnumDecl,
    FieldKey, Native, Primitive,
};

use crate::{
    bridge::c::{CBridgeContract, Type as CBridgeType},
    core::{AuxChunk, Diagnostic, Emitted, Error, RenderContext, Result},
};

use super::super::{
    codec::{ReadExpression, Reader, ValueScope, Writer},
    name_style::{Name, Namespace},
    syntax::{Expression, Identifier, Statement, TypeFragment},
    type_name,
};
use super::{Documentation, Function, WireTemplate, primitive_type};

#[derive(Template)]
#[template(path = "target/csharp/enumeration.cs", escape = "none")]
struct EnumerationTemplate<'enumeration> {
    enumeration: &'enumeration Enumeration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::target::csharp) struct Enumeration {
    documentation: Documentation,
    namespace: Namespace,
    name: Identifier,
    c_style: bool,
    error_payload: bool,
    underlying_type: TypeFragment,
    variants: Vec<Variant>,
    data_variants: Vec<DataVariant>,
    methods: Vec<Function>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Variant {
    documentation: Documentation,
    name: Identifier,
    discriminant: i128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DataVariant {
    documentation: Documentation,
    name: Identifier,
    tag: u32,
    fields: Vec<DataField>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DataField {
    parameter_documentation: Documentation,
    key: FieldKey,
    name: Identifier,
    ty: TypeFragment,
    read: Expression,
    write: Vec<Statement>,
}

impl Enumeration {
    pub(in crate::target::csharp) fn from_declaration(
        declaration: &EnumDecl<Native>,
        namespace: Namespace,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match declaration {
            EnumDecl::CStyle(enumeration) => {
                Self::from_c_style(enumeration, namespace, bridge, context)
            }
            EnumDecl::Data(enumeration) => Self::from_data(enumeration, namespace, bridge, context),
            _ => Err(Error::UnexpectedBindingShape {
                layer: "csharp enum",
                shape: "unknown enum declaration",
            }),
        }
    }

    fn from_c_style(
        declaration: &CStyleEnumDecl<Native>,
        namespace: Namespace,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let primitive = declaration.repr().primitive();
        if matches!(primitive, Primitive::ISize | Primitive::USize) {
            return Err(Error::UnsupportedTarget {
                target: "csharp",
                shape: "pointer-width enum representation",
            });
        }
        let c_enum =
            bridge
                .source_c_style_enum(declaration.id())
                .ok_or(Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "C-style enum is missing from the C bridge",
                })?;
        if c_enum.repr() != &CBridgeType::primitive(primitive)?
            || c_enum.variants().len() != declaration.variants().len()
        {
            return Err(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "C-style enum does not match the C bridge",
            });
        }
        let variants = declaration
            .variants()
            .iter()
            .zip(c_enum.variants())
            .map(|(variant, c_variant)| {
                if variant.discriminant().get() != c_variant.value() {
                    return Err(Error::BrokenBridgeContract {
                        bridge: "c",
                        invariant: "C-style enum discriminant does not match the C bridge",
                    });
                }
                Ok(Variant {
                    documentation: Documentation::summary(variant.meta().doc(), "        "),
                    name: Name::new(variant.name()).pascal()?,
                    discriminant: variant.discriminant().get(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let name = Name::new(declaration.name()).pascal()?;
        let owner = DirectValueType::Enum(declaration.id());
        let mut methods = Vec::new();
        let mut diagnostics = Vec::new();
        for initializer in declaration.initializers() {
            collect_associated(
                &mut methods,
                &mut diagnostics,
                "initializer",
                initializer.name(),
                Function::from_initializer(
                    initializer,
                    owner.clone(),
                    &name,
                    true,
                    Some(&namespace),
                    bridge,
                    context,
                ),
            )?;
        }
        for method in declaration.methods() {
            collect_associated(
                &mut methods,
                &mut diagnostics,
                "method",
                method.name(),
                Function::from_method(
                    method,
                    owner.clone(),
                    &name,
                    true,
                    Some(&namespace),
                    bridge,
                    context,
                ),
            )?;
        }
        Ok(Self {
            documentation: Documentation::summary(declaration.meta().doc(), "    "),
            namespace,
            name,
            c_style: true,
            error_payload: declaration.is_error_payload(),
            underlying_type: primitive_type(primitive),
            variants,
            data_variants: Vec::new(),
            methods,
            diagnostics,
        })
    }

    fn from_data(
        declaration: &DataEnumDecl<Native>,
        namespace: Namespace,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let reader = Identifier::parse("reader")?;
        let writer = Identifier::parse("writer")?;
        let data_variants = declaration
            .variants()
            .iter()
            .map(|variant| {
                let fields = match variant.payload() {
                    DataVariantPayload::Unit => Vec::new(),
                    DataVariantPayload::Tuple(fields) | DataVariantPayload::Struct(fields) => {
                        let scope = ValueScope::fields(
                            fields
                                .iter()
                                .map(|field| {
                                    Ok((
                                        field.key().clone(),
                                        Expression::new(format!(
                                            "value.{}",
                                            data_field_name(field.key())?
                                        )),
                                    ))
                                })
                                .collect::<Result<Vec<_>>>()?,
                        );
                        fields
                            .iter()
                            .map(|field| {
                                Ok(DataField {
                                    parameter_documentation: Documentation::parameter(
                                        field.meta().doc(),
                                        data_field_name(field.key())?.as_str(),
                                        "        ",
                                    ),
                                    key: field.key().clone(),
                                    name: data_field_name(field.key())?,
                                    ty: type_name::type_ref_qualified(
                                        field.ty(),
                                        &namespace,
                                        context,
                                    )?,
                                    read: field
                                        .read()
                                        .render_with(
                                            &mut Reader::new(reader.clone(), context)
                                                .qualified(&namespace),
                                        )
                                        .map(ReadExpression::into_expression)?,
                                    write: field
                                        .write()
                                        .render_with(&mut Writer::new(
                                            writer.clone(),
                                            scope.clone(),
                                            context,
                                        ))
                                        .into_iter()
                                        .collect::<Result<Vec<_>>>()?,
                                })
                            })
                            .collect::<Result<Vec<_>>>()?
                    }
                    _ => {
                        return Err(Error::UnexpectedBindingShape {
                            layer: "csharp enum",
                            shape: "unknown data enum payload",
                        });
                    }
                };
                Ok(DataVariant {
                    documentation: Documentation::summary(variant.meta().doc(), "        "),
                    name: Name::new(variant.name()).pascal()?,
                    tag: variant.tag().get(),
                    fields,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let name = Name::new(declaration.name()).pascal()?;
        let owner = DirectValueType::Enum(declaration.id());
        let mut methods = Vec::new();
        let mut diagnostics = Vec::new();
        for initializer in declaration.initializers() {
            collect_associated(
                &mut methods,
                &mut diagnostics,
                "initializer",
                initializer.name(),
                Function::from_initializer_qualified(
                    initializer,
                    owner.clone(),
                    &name,
                    &namespace,
                    bridge,
                    context,
                ),
            )?;
        }
        for method in declaration.methods() {
            collect_associated(
                &mut methods,
                &mut diagnostics,
                "method",
                method.name(),
                Function::from_encoded_method(
                    method,
                    owner.clone(),
                    &name,
                    declaration.read(),
                    declaration.write(),
                    Some(&namespace),
                    bridge,
                    context,
                ),
            )?;
        }
        Ok(Self {
            documentation: Documentation::summary(declaration.meta().doc(), "    "),
            namespace,
            name,
            c_style: false,
            error_payload: declaration.is_error_payload(),
            underlying_type: TypeFragment::void(),
            variants: Vec::new(),
            data_variants,
            methods,
            diagnostics,
        })
    }

    pub(in crate::target::csharp) fn render(&self) -> Result<Emitted> {
        let mut emitted = Emitted::primary(EnumerationTemplate { enumeration: self }.render()?)
            .with_diagnostics(self.diagnostics.iter().cloned());
        for method in &self.methods {
            let (_, aux, diagnostics) = method.render()?.into_parts();
            for chunk in aux {
                emitted = emitted.with_aux(chunk);
            }
            emitted = emitted.with_diagnostics(diagnostics);
        }
        if !self.c_style {
            emitted = emitted.with_aux(AuxChunk::ForwardDecl(WireTemplate.render()?.into()));
        }
        Ok(emitted)
    }
}

fn data_field_name(key: &FieldKey) -> Result<Identifier> {
    match key {
        FieldKey::Named(name) => Name::new(name).pascal(),
        FieldKey::Position(position) => Identifier::parse(format!("Field{position}")),
        _ => Err(Error::UnexpectedBindingShape {
            layer: "csharp enum",
            shape: "unknown data enum field key",
        }),
    }
}

fn collect_associated(
    methods: &mut Vec<Function>,
    diagnostics: &mut Vec<Diagnostic>,
    kind: &'static str,
    name: &CanonicalName,
    result: Result<Function>,
) -> Result<()> {
    match result {
        Ok(function) => methods.push(function),
        Err(Error::UnsupportedTarget { shape, .. } | Error::UnsupportedCAbi { shape }) => {
            diagnostics.push(Diagnostic::new(format!(
                "{kind} {}: {shape}",
                Name::new(name).pascal()?
            )));
        }
        Err(error) => return Err(error),
    }
    Ok(())
}
