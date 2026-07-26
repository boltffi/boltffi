use crate::bridge::{c::Identifier, jni::NativeParameter};

#[derive(Clone)]
pub struct DirectBufferParameterView {
    pub name: Identifier,
    pub pointer: Identifier,
    pub length: Identifier,
    pub writeback: Option<DirectBufferWritebackView>,
}

#[derive(Clone)]
pub struct DirectBufferWritebackView {
    pub local: Identifier,
}

impl DirectBufferParameterView {
    pub fn from_parameter(parameter: &NativeParameter) -> Option<Self> {
        parameter.bytes().map(|parameter| Self {
            name: parameter.name().clone(),
            pointer: parameter.pointer().clone(),
            length: parameter.length().clone(),
            writeback: parameter
                .writeback()
                .map(|writeback| DirectBufferWritebackView {
                    local: writeback.local().clone(),
                }),
        })
    }
}
