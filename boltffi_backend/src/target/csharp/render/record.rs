use askama::Template;
use boltffi_binding::{
    CanonicalName, DirectRecordDecl, DirectValueType, FieldKey, Native, RecordDecl,
};

use crate::{
    bridge::c::{CBridgeContract, Type as CBridgeType},
    core::{Diagnostic, Emitted, Error, RenderContext, Result},
};

use super::super::{
    name_style::{Name, Namespace},
    syntax::{Identifier, TypeFragment},
};
use super::{Function, primitive_type};

#[derive(Template)]
#[template(path = "target/csharp/record.cs", escape = "none")]
struct RecordTemplate<'record> {
    record: &'record Record,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::target::csharp) struct Record {
    namespace: Namespace,
    name: Identifier,
    fields: Vec<Field>,
    methods: Vec<Function>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Field {
    name: Identifier,
    ty: TypeFragment,
    marshal_i1: bool,
}

impl Record {
    pub(in crate::target::csharp) fn from_declaration(
        declaration: &RecordDecl<Native>,
        namespace: Namespace,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match declaration {
            RecordDecl::Direct(record) => Self::from_direct(record, namespace, bridge, context),
            RecordDecl::Encoded(_) => Err(Error::UnsupportedTarget {
                target: "csharp",
                shape: "encoded records",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: "csharp record",
                shape: "unknown record declaration",
            }),
        }
    }

    fn from_direct(
        declaration: &DirectRecordDecl<Native>,
        namespace: Namespace,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let c_record =
            bridge
                .source_direct_record(declaration.id())
                .ok_or(Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "direct record is missing from the C bridge",
                })?;
        if declaration.fields().len() != c_record.fields().len() {
            return Err(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "direct record field count does not match the C bridge",
            });
        }
        let fields = declaration
            .fields()
            .iter()
            .zip(c_record.fields())
            .map(|(field, c_field)| {
                let primitive = field.ty().primitive();
                if c_field.ty() != &CBridgeType::primitive(primitive)? {
                    return Err(Error::BrokenBridgeContract {
                        bridge: "c",
                        invariant: "direct record field type does not match the C bridge",
                    });
                }
                Ok(Field {
                    name: field_name(field.key())?,
                    ty: primitive_type(primitive),
                    marshal_i1: matches!(primitive, boltffi_binding::Primitive::Bool),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let name = Name::new(declaration.name()).pascal()?;
        let owner = DirectValueType::Record(declaration.id());
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
                    false,
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
                Function::from_method(method, owner.clone(), &name, false, bridge, context),
            )?;
        }
        Ok(Self {
            namespace,
            name,
            fields,
            methods,
            diagnostics,
        })
    }

    pub(in crate::target::csharp) fn render(&self) -> Result<Emitted> {
        let mut emitted = Emitted::primary(RecordTemplate { record: self }.render()?)
            .with_diagnostics(self.diagnostics.iter().cloned());
        for method in &self.methods {
            let (_, aux, diagnostics) = method.render()?.into_parts();
            for chunk in aux {
                emitted = emitted.with_aux(chunk);
            }
            emitted = emitted.with_diagnostics(diagnostics);
        }
        Ok(emitted)
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

fn field_name(key: &FieldKey) -> Result<Identifier> {
    match key {
        FieldKey::Named(name) => Name::new(name).pascal(),
        FieldKey::Position(position) => Identifier::parse(format!("Field{position}")),
        _ => Err(Error::UnexpectedBindingShape {
            layer: "csharp record",
            shape: "unknown field key",
        }),
    }
}
