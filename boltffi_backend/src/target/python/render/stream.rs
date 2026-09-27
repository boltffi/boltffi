use boltffi_binding::{
    ByteSize, ClassDecl, DirectValueType, Native, ReadPlan, StreamDecl, StreamItemPlan,
    StreamItemPlanRender, TypeRef, native,
};

use crate::{
    core::Result,
    target::python::{
        codec::Expression as CodecExpression,
        cpython::render::stream as stream_render,
        name_style::Name,
        render::Package,
        syntax::{CallExpression, Expression, Identifier, Statement, TypeAnnotation},
    },
};

use super::{Documentation, type_hint::TypeHint};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassStream {
    pub documentation: Documentation,
    pub python_name: Identifier,
    pub subscribe_method: Identifier,
    pub subscription_class: Identifier,
    pub item_annotation: TypeAnnotation,
    pub pop_batch_body: Vec<Statement>,
    pub wait_method: Identifier,
    pub unsubscribe_method: Identifier,
    pub free_method: Identifier,
    uses_wire_helpers: bool,
}

impl ClassStream {
    pub fn from_declaration(
        declaration: &StreamDecl<Native>,
        owner: &ClassDecl<Native>,
        class_name: &Identifier,
        package: &Package,
    ) -> Result<Self> {
        let symbols = stream_render::Symbols::new(declaration, Some(owner.name()))?;
        let item = StreamItem::from_plan(declaration.item(), package)?;
        let pop_batch_body = item.pop_batch_body(symbols.pop_batch()?)?;
        let uses_wire_helpers = item.uses_wire_helpers;
        Ok(Self {
            documentation: Documentation::new(declaration.meta().doc()),
            python_name: Name::new(declaration.name()).function()?,
            subscribe_method: symbols.subscribe()?,
            subscription_class: Identifier::parse(format!(
                "{}{}Subscription",
                class_name,
                Name::new(declaration.name()).class()
            ))?,
            item_annotation: item.annotation,
            pop_batch_body,
            wait_method: symbols.wait()?,
            unsubscribe_method: symbols.unsubscribe()?,
            free_method: symbols.free()?,
            uses_wire_helpers,
        })
    }

    pub fn uses_wire_helpers(&self) -> bool {
        self.uses_wire_helpers
    }

    pub fn member_name(&self) -> (String, String) {
        (
            self.python_name.to_string(),
            format!("stream `{}`", self.python_name),
        )
    }

    pub fn top_level_name(&self) -> (String, String) {
        (
            self.subscription_class.to_string(),
            format!("stream subscription `{}`", self.subscription_class),
        )
    }
}

struct StreamItem {
    annotation: TypeAnnotation,
    decode: Option<Expression>,
    uses_wire_helpers: bool,
}

impl StreamItem {
    fn from_plan(plan: &StreamItemPlan<Native>, package: &Package) -> Result<Self> {
        plan.render_with(&mut PackageStreamItem { package })
    }

    fn pop_batch_body(&self, method: Identifier) -> Result<Vec<Statement>> {
        let native_call = self.native_call(method)?;
        match &self.decode {
            Some(decode) => {
                let data = Identifier::parse("data")?;
                let decoded = Expression::call(
                    CallExpression::new(Expression::identifier(Identifier::parse(
                        "_boltffi_read_wire",
                    )?))
                    .positional(Expression::identifier(data.clone()))
                    .positional(Expression::lambda(
                        Identifier::parse("reader")?,
                        decode.clone(),
                    )),
                );
                Ok(vec![
                    Statement::assign(data.clone(), native_call),
                    Statement::return_value(Expression::conditional(
                        decoded,
                        Expression::identifier(data),
                        Expression::empty_list(),
                    )),
                ])
            }
            None => Ok(vec![Statement::return_value(native_call)]),
        }
    }

    fn native_call(&self, method: Identifier) -> Result<Expression> {
        let receiver = Expression::call(CallExpression::new(Expression::attribute(
            Expression::identifier(Identifier::parse("self")?),
            Identifier::parse("_require_handle")?,
        )));
        Ok(Expression::call(
            CallExpression::new(Expression::attribute(
                Expression::identifier(Identifier::parse("_native")?),
                method,
            ))
            .positional(receiver)
            .positional(Expression::identifier(Identifier::parse("max_count")?)),
        ))
    }
}

struct PackageStreamItem<'package> {
    package: &'package Package<'package>,
}

impl<'plan, 'package> StreamItemPlanRender<'plan, Native> for PackageStreamItem<'package> {
    type Output = Result<StreamItem>;

    fn direct(&mut self, ty: &'plan DirectValueType, _: ByteSize) -> Self::Output {
        Ok(StreamItem {
            annotation: TypeHint::from_direct_value(ty, self.package)?.into_annotation(),
            decode: None,
            uses_wire_helpers: false,
        })
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        read: &'plan ReadPlan,
        _: native::BufferShape,
    ) -> Self::Output {
        Ok(StreamItem {
            annotation: TypeHint::from_type_ref(ty, self.package)?.into_annotation(),
            decode: Some(CodecExpression::read_sequence(read, self.package)?.into_expression()),
            uses_wire_helpers: true,
        })
    }
}
