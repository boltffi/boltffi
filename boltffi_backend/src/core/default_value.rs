use boltffi_binding::{
    CanonicalName, CustomTypeId, DirectFieldDecl, EncodedFieldDecl, FieldKey, RecordDecl, RecordId,
    Surface, TypeRef,
};

use crate::core::{Error, RenderContext, Result};

pub enum Representation<'bindings> {
    Transparent(&'bindings TypeRef),
    Record(Record<'bindings>),
}

pub struct Record<'bindings> {
    name: &'bindings CanonicalName,
    field: Field<'bindings>,
}

pub enum Field<'bindings> {
    Direct(&'bindings DirectFieldDecl),
    Encoded(&'bindings EncodedFieldDecl),
}

impl<'bindings> Representation<'bindings> {
    pub fn resolve<S: Surface>(
        custom_type: CustomTypeId,
        context: &'bindings RenderContext<S>,
    ) -> Result<Self> {
        if context.custom_type_mapping(custom_type).is_some() {
            return Err(Error::UnsupportedTarget {
                target: context.target(),
                shape: "mapped custom type default",
            });
        }

        match context
            .custom_type(custom_type)
            .ok_or(Error::BrokenBridgeContract {
                bridge: context.target(),
                invariant: "missing custom type default declaration",
            })?
            .representation()
        {
            TypeRef::Record(record) => Self::record(*record, context),
            representation => Ok(Self::Transparent(representation)),
        }
    }

    fn record<S: Surface>(record: RecordId, context: &'bindings RenderContext<S>) -> Result<Self> {
        let declaration = context.record(record).ok_or(Error::BrokenBridgeContract {
            bridge: context.target(),
            invariant: "missing custom type default representation",
        })?;
        let field = match declaration {
            RecordDecl::Direct(record) => match record.fields() {
                [field] => Field::Direct(field),
                _ => return Self::unsupported_record(context),
            },
            RecordDecl::Encoded(record) => match record.fields() {
                [field] => Field::Encoded(field),
                _ => return Self::unsupported_record(context),
            },
            _ => return Self::unsupported_record(context),
        };
        Ok(Self::Record(Record {
            name: declaration.name(),
            field,
        }))
    }

    fn unsupported_record<S: Surface>(context: &RenderContext<S>) -> Result<Self> {
        Err(Error::UnsupportedTarget {
            target: context.target(),
            shape: "custom type default with non-single-field representation",
        })
    }
}

impl<'bindings> Record<'bindings> {
    pub fn name(&self) -> &'bindings CanonicalName {
        self.name
    }

    pub fn field(&self) -> &Field<'bindings> {
        &self.field
    }
}

impl<'bindings> Field<'bindings> {
    pub fn key(&self) -> &'bindings FieldKey {
        match self {
            Self::Direct(field) => field.key(),
            Self::Encoded(field) => field.key(),
        }
    }
}
