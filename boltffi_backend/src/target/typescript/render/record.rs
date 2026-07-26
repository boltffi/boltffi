use askama::Template as AskamaTemplate;
use boltffi_binding::{DirectRecordDecl, EncodedFieldDecl, EncodedRecordDecl, RecordDecl, Wasm32};

use crate::core::{Diagnostic, Emitted, Error, RenderContext, Result};

use super::super::{
    codec::{Reader, Sizer, Writer},
    name_style::Name,
    primitive::Scalar,
    syntax::{
        ArgumentList, Expression, Identifier, MethodDeclaration, PropertyKey, Statement, TypeName,
    },
};
use super::{Function, Type};

#[derive(AskamaTemplate)]
#[template(path = "target/typescript/record.ts", escape = "none")]
pub struct Record {
    name: TypeName,
    codec: Identifier,
    fields: Vec<Field>,
    size: Expression,
    writes: Vec<Statement>,
    reads: Vec<Statement>,
    methods: Vec<MethodDeclaration>,
    diagnostics: Vec<Diagnostic>,
    error: bool,
}

struct Field {
    key: PropertyKey,
    local: Identifier,
    ty: TypeName,
}

struct DirectParts {
    offset: u64,
    fields: Vec<Field>,
    writes: Vec<Statement>,
    reads: Vec<Statement>,
}

impl Record {
    pub fn from_declaration(
        declaration: &RecordDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        match declaration {
            RecordDecl::Direct(record) => Self::direct(record, context),
            RecordDecl::Encoded(record) => Self::encoded(record, context),
            _ => Err(Self::error("unknown record declaration")),
        }
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(AskamaTemplate::render(self)?)
            .with_diagnostics(self.diagnostics.clone()))
    }

    fn direct(record: &DirectRecordDecl<Wasm32>, context: &RenderContext<Wasm32>) -> Result<Self> {
        let writer = Identifier::known("writer");
        let reader = Identifier::known("reader");
        let value = Expression::identifier(Identifier::known("value"));
        let mut parts = record
            .fields()
            .iter()
            .zip(record.layout().fields())
            .try_fold(
                DirectParts {
                    offset: 0,
                    fields: Vec::new(),
                    writes: Vec::new(),
                    reads: Vec::new(),
                },
                |mut parts, (field, layout)| {
                    if field.key() != layout.key() {
                        return Err(Self::error("direct record field layout mismatch"));
                    }
                    let key = PropertyKey::from_field(field.key())?;
                    let local = key.local()?;
                    let padding = layout
                        .offset()
                        .get()
                        .checked_sub(parts.offset)
                        .ok_or_else(|| Self::error("direct record field layout moves backwards"))?;
                    parts.skip(padding, &writer, &reader);
                    let scalar = Scalar::new(field.ty().primitive())?;
                    parts.writes.push(Statement::expression(Expression::call(
                        Expression::identifier(writer.clone()),
                        scalar.write_method(),
                        [key.access(value.clone())?]
                            .into_iter()
                            .collect::<ArgumentList>(),
                    )));
                    parts.reads.push(Statement::constant(
                        local.clone(),
                        Expression::call(
                            Expression::identifier(reader.clone()),
                            scalar.read_method(),
                            ArgumentList::default(),
                        ),
                    ));
                    parts.offset = layout.offset().get() + field.ty().byte_size().get();
                    parts.fields.push(Field {
                        key,
                        local,
                        ty: scalar.ty(),
                    });
                    Ok(parts)
                },
            )?;
        let tail = record
            .layout()
            .size()
            .get()
            .checked_sub(parts.offset)
            .ok_or_else(|| Self::error("direct record size is smaller than its fields"))?;
        parts.skip(tail, &writer, &reader);
        let (methods, diagnostics) = Function::record_methods(
            record.id(),
            record.initializers(),
            record.methods(),
            context,
        )?;
        Ok(Self {
            name: Name::new(record.name()).type_name(),
            codec: Name::new(record.name()).codec_identifier()?,
            fields: parts.fields,
            size: Expression::integer(record.layout().size().get()),
            writes: parts.writes,
            reads: parts.reads,
            methods,
            diagnostics,
            error: record.is_error_payload(),
        })
    }

    fn encoded(
        record: &EncodedRecordDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let writer = Identifier::known("writer");
        let reader = Identifier::known("reader");
        let value = Expression::identifier(Identifier::known("value"));
        let size = record
            .fields()
            .iter()
            .map(|field| {
                field
                    .write()
                    .size_with(&mut Sizer::new(value.clone(), context))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|size| size.into_expression())
            .reduce(Expression::add)
            .unwrap_or_else(|| Expression::integer(0));
        let writes = record
            .fields()
            .iter()
            .flat_map(|field| {
                field
                    .write()
                    .render_with(&mut Writer::new(writer.clone(), value.clone(), context))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|write| write.into_statement())
            .collect();
        let fields = record
            .fields()
            .iter()
            .map(|field| Self::encoded_field(field, context))
            .collect::<Result<Vec<_>>>()?;
        let reads = record
            .fields()
            .iter()
            .zip(fields.iter())
            .map(|(field, rendered)| {
                field
                    .read()
                    .render_with(&mut Reader::new(reader.clone(), context))
                    .map(|read| Statement::constant(rendered.local.clone(), read.into_expression()))
            })
            .collect::<Result<Vec<_>>>()?;
        let (methods, diagnostics) = Function::record_methods(
            record.id(),
            record.initializers(),
            record.methods(),
            context,
        )?;
        Ok(Self {
            name: Name::new(record.name()).type_name(),
            codec: Name::new(record.name()).codec_identifier()?,
            fields,
            size,
            writes,
            reads,
            methods,
            diagnostics,
            error: record.is_error_payload(),
        })
    }

    fn encoded_field(field: &EncodedFieldDecl, context: &RenderContext<Wasm32>) -> Result<Field> {
        let key = PropertyKey::from_field(field.key())?;
        Ok(Field {
            local: key.local()?,
            key,
            ty: Type::from_ref(field.ty(), context)?,
        })
    }

    fn error(shape: &'static str) -> Error {
        Error::UnsupportedTarget {
            target: "typescript",
            shape,
        }
    }
}

impl DirectParts {
    fn skip(&mut self, bytes: u64, writer: &Identifier, reader: &Identifier) {
        if bytes == 0 {
            return;
        }
        [(writer, &mut self.writes), (reader, &mut self.reads)]
            .into_iter()
            .for_each(|(receiver, statements)| {
                statements.push(Statement::expression(Expression::call(
                    Expression::identifier(receiver.clone()),
                    Identifier::known("skip"),
                    [Expression::integer(bytes)]
                        .into_iter()
                        .collect::<ArgumentList>(),
                )));
            });
    }
}
