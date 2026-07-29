use boltffi_binding::{
    ConstantOwner, DirectFieldDecl, DirectRecordDecl, EncodedFieldDecl, EncodedRecordDecl,
    ExportedMethodDecl, FieldKey, InitializerDecl, Native, NativeSymbol, Receive,
};

use crate::{
    core::{Error, Result},
    target::python::{
        codec::{Expression as CodecExpression, FixedStruct},
        cpython::render::{function, record as record_render},
        name_style::Name,
        render::Package,
        syntax::{Expression, Identifier, TypeAnnotation},
    },
};

use super::{
    AssociatedCallable, ConstantStub, NameScope, constant::DefaultExpression, type_hint::TypeHint,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordClass {
    pub class_name: Identifier,
    pub exception_name: Option<Identifier>,
    pub register_method: Identifier,
    pub fields: Vec<RecordField>,
    pub constants: Vec<ConstantStub>,
    pub wire: RecordWire,
    pub constructors: Vec<AssociatedCallable>,
    pub static_methods: Vec<AssociatedCallable>,
    pub instance_methods: Vec<AssociatedCallable>,
}

impl RecordClass {
    pub fn from_direct(record: &DirectRecordDecl<Native>, package: &Package) -> Result<Self> {
        let c_record =
            package
                .bridge
                .source_direct_record(record.id())
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "direct record package without C typedef",
                })?;
        let symbols = record_render::Symbols::from_direct(record, c_record)?;
        Ok(Self {
            class_name: symbols.class_name().clone(),
            exception_name: record
                .is_error_payload()
                .then(|| package.exception_name(symbols.class_name()))
                .transpose()?,
            register_method: symbols.register_method().clone(),
            fields: record
                .fields()
                .iter()
                .map(|field| RecordField::from_direct(field, package))
                .collect::<Result<Vec<_>>>()?,
            constants: package.constants_for_owner(ConstantOwner::Record(record.id()))?,
            wire: RecordWire::Fixed(FixedStruct::from_layout(
                symbols.class_name(),
                record.fields(),
                record.layout(),
            )?),
            constructors: Self::constructors(record.initializers(), &symbols, package)?,
            static_methods: Self::static_methods(record.methods(), &symbols, package)?,
            instance_methods: Self::instance_methods(record.methods(), &symbols, package)?,
        })
    }

    pub fn from_encoded(record: &EncodedRecordDecl<Native>, package: &Package) -> Result<Self> {
        let symbols = record_render::Symbols::from_encoded(record)?;
        let fields = record
            .fields()
            .iter()
            .map(|field| RecordField::from_encoded(field, package))
            .collect::<Result<Vec<_>>>()?;
        let wire_fields = record
            .fields()
            .iter()
            .map(|field| EncodedRecordField::from_field(field, package))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            class_name: symbols.class_name().clone(),
            exception_name: record
                .is_error_payload()
                .then(|| package.exception_name(symbols.class_name()))
                .transpose()?,
            register_method: symbols.register_method().clone(),
            fields,
            constants: package.constants_for_owner(ConstantOwner::Record(record.id()))?,
            wire: RecordWire::Fields(wire_fields),
            constructors: Self::constructors(record.initializers(), &symbols, package)?,
            static_methods: Self::static_methods(record.methods(), &symbols, package)?,
            instance_methods: Self::instance_methods(record.methods(), &symbols, package)?,
        })
    }

    pub fn uses_async_helpers(&self) -> bool {
        self.callables().any(AssociatedCallable::uses_async_helpers)
    }

    pub fn uses_sequence_annotations(&self) -> bool {
        self.callables()
            .any(AssociatedCallable::uses_sequence_annotations)
    }

    pub fn uses_callable_annotations(&self) -> bool {
        self.callables()
            .any(AssociatedCallable::uses_callable_annotations)
    }

    pub fn validate_names(&self) -> Result<()> {
        NameScope::new(format!("record `{}`", self.class_name))
            .insert_all(self.fields.iter().map(RecordField::field_name))
            .and_then(|scope| {
                scope.insert_all(self.constants.iter().map(ConstantStub::member_name))
            })
            .and_then(|scope| {
                scope.insert_all(self.callables().map(AssociatedCallable::member_name))
            })
            .map(|_| ())?;
        RecordField::validate_default_order(&self.fields, "record default before required field")?;
        self.callables()
            .try_for_each(|callable| callable.validate_names(&self.class_name))
    }

    pub fn top_level_name(&self) -> (String, String) {
        (
            self.class_name.to_string(),
            format!("record `{}`", self.class_name),
        )
    }

    pub fn exception_top_level_name(&self) -> Option<(String, String)> {
        self.exception_name.as_ref().map(|exception_name| {
            (
                exception_name.to_string(),
                format!("record error `{}`", exception_name),
            )
        })
    }

    fn callables(&self) -> impl Iterator<Item = &AssociatedCallable> {
        self.constructors
            .iter()
            .chain(&self.static_methods)
            .chain(&self.instance_methods)
    }

    fn constructors(
        initializers: &[InitializerDecl<Native>],
        symbols: &record_render::Symbols,
        package: &Package,
    ) -> Result<Vec<AssociatedCallable>> {
        initializers
            .iter()
            .filter(|initializer| function::Function::can_render(initializer.callable()))
            .map(|initializer| {
                AssociatedCallable::from_value_initializer(
                    initializer,
                    symbols.initializer(initializer.name())?,
                    package,
                )
            })
            .collect()
    }

    fn static_methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        symbols: &record_render::Symbols,
        package: &Package,
    ) -> Result<Vec<AssociatedCallable>> {
        methods
            .iter()
            .filter(|method| {
                function::Function::can_render(method.callable())
                    && method.callable().receiver().is_none()
            })
            .map(|method| {
                AssociatedCallable::from_value_method(
                    method,
                    symbols.method(method.name())?,
                    None,
                    None,
                    package,
                )
            })
            .collect()
    }

    fn instance_methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        symbols: &record_render::Symbols,
        package: &Package,
    ) -> Result<Vec<AssociatedCallable>> {
        methods
            .iter()
            .filter(|method| {
                function::Function::can_render(method.callable())
                    && method.callable().receiver().is_some()
            })
            .map(|method| {
                AssociatedCallable::from_value_method(
                    method,
                    symbols.method(method.name())?,
                    Some(Expression::identifier(Identifier::parse("self")?)),
                    method
                        .callable()
                        .receiver()
                        .filter(|receiver| matches!(receiver, Receive::ByMutRef))
                        .map(|_| TypeAnnotation::identifier(symbols.class_name().clone())),
                    package,
                )
            })
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordField {
    pub name: Identifier,
    pub annotation: TypeAnnotation,
    pub default: Option<Expression>,
}

impl RecordField {
    pub fn from_encoded(field: &EncodedFieldDecl, package: &Package) -> Result<Self> {
        Ok(Self {
            name: Self::name(field.key())?,
            annotation: TypeHint::from_type_ref(field.ty(), package)?.into_annotation(),
            default: field
                .meta()
                .default()
                .map(|value| DefaultExpression::new(value, package))
                .transpose()?
                .map(DefaultExpression::into_expression),
        })
    }

    pub fn field_name(&self) -> (String, String) {
        (self.name.to_string(), format!("field `{}`", self.name))
    }

    pub fn validate_default_order(fields: &[Self], shape: &'static str) -> Result<()> {
        match fields
            .iter()
            .scan(false, |default_seen, field| {
                let required_after_default = *default_seen && field.default.is_none();
                *default_seen |= field.default.is_some();
                Some(required_after_default)
            })
            .any(|required_after_default| required_after_default)
        {
            true => Err(Error::UnsupportedTarget {
                target: "python",
                shape,
            }),
            false => Ok(()),
        }
    }

    pub fn name(key: &FieldKey) -> Result<Identifier> {
        match key {
            FieldKey::Named(name) => Name::new(name).function(),
            FieldKey::Position(position) => Name::position_field(*position),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown record field annotation",
            }),
        }
    }

    fn from_direct(field: &DirectFieldDecl, package: &Package) -> Result<Self> {
        Ok(Self {
            name: Self::name(field.key())?,
            annotation: TypeHint::from_primitive(field.ty().primitive())?.into_annotation(),
            default: field
                .meta()
                .default()
                .map(|value| DefaultExpression::new(value, package))
                .transpose()?
                .map(DefaultExpression::into_expression),
        })
    }
}

/// How a record class moves through wire bytes.
///
/// The variants mirror the record declaration split: a direct record packs
/// its padded memory image through one precompiled `struct.Struct`, and an
/// encoded record walks its fields through per-field codec expressions.
/// Rendering matches on the variant, so a record cannot silently fall off
/// its lane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecordWire {
    Fixed(FixedStruct),
    Fields(Vec<EncodedRecordField>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedRecordField {
    pub name: Identifier,
    pub encode: Expression,
    pub decode: Expression,
}

impl EncodedRecordField {
    pub fn from_field(field: &EncodedFieldDecl, package: &Package) -> Result<Self> {
        let name = RecordField::name(field.key())?;
        Ok(Self {
            encode: CodecExpression::write_record_field(field.write(), package)?.into_expression(),
            decode: CodecExpression::read(field.read(), package)?.into_expression(),
            name,
        })
    }
}
