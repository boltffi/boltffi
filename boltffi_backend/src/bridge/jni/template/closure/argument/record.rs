use crate::bridge::{
    c::{Identifier, Statement},
    jni::ClosureRecordArgument,
};

pub struct ClosureRecordArgumentView {
    pub array: Identifier,
    pub parameter: Identifier,
    pub value: Identifier,
    pub value_declaration: Statement,
}

impl ClosureRecordArgumentView {
    pub fn from_argument(argument: &ClosureRecordArgument) -> Self {
        Self {
            array: argument.array().clone(),
            parameter: argument.parameter().name().clone(),
            value: argument.value().clone(),
            value_declaration: argument.value_declaration(),
        }
    }
}
