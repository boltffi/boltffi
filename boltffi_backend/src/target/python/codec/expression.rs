use boltffi_binding::{CodecNode, ReadPlan, WritePlan};

use crate::{
    core::Result,
    target::python::{
        codec::{read::Reader, value::SelfPositionAccess, write::Writer},
        render::Package,
        syntax::Expression as PythonExpression,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Expression {
    expression: PythonExpression,
}

impl Expression {
    pub fn read(plan: &ReadPlan, package: &Package) -> Result<Self> {
        let mut reader = Reader::new(package);
        Ok(Self {
            expression: plan.render_with(&mut reader)?.into_expression(),
        })
    }

    pub fn read_codec(codec: &CodecNode, package: &Package) -> Result<Self> {
        let mut reader = Reader::new(package);
        Ok(Self {
            expression: codec.render_read_with(&mut reader)?.into_expression(),
        })
    }

    pub fn read_sequence(item: &ReadPlan, package: &Package) -> Result<Self> {
        let mut reader = Reader::new(package);
        let item = item.render_with(&mut reader)?;
        Ok(Self {
            expression: reader.sequence_expression(item)?,
        })
    }

    pub fn write(plan: &WritePlan, package: &Package) -> Result<Self> {
        let mut writer = Writer::new(package);
        Ok(Self {
            expression: Writer::single(plan.render_with(&mut writer))?,
        })
    }

    pub fn write_record_field(plan: &WritePlan, package: &Package) -> Result<Self> {
        let mut writer = Writer::with_self_position_access(package, SelfPositionAccess::Attribute);
        Ok(Self {
            expression: Writer::single(plan.render_with(&mut writer))?,
        })
    }

    pub fn write_argument(plan: &WritePlan, package: &Package) -> Result<Self> {
        Self::write(plan, package)
    }

    pub fn read_return(plan: &ReadPlan, package: &Package) -> Result<Self> {
        Self::read(plan, package)
    }

    pub fn into_expression(self) -> PythonExpression {
        self.expression
    }
}
