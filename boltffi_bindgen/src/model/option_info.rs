use super::{Module, Primitive, Type};

#[derive(Debug, Clone)]
pub struct OptionInfo {
    pub inner: Type,
    pub ffi_type: String,
    pub is_vec: bool,
}

impl OptionInfo {
    pub fn from_type(inner: &Type) -> Self {
        let is_vec = inner.is_vec();
        let ffi_type = Self::compute_ffi_type(inner);

        Self {
            inner: inner.clone(),
            ffi_type,
            is_vec,
        }
    }

    fn compute_ffi_type(inner: &Type) -> String {
        match inner {
            Type::Primitive(p) => format!("FfiOption_{}", p.rust_name()),
            Type::String => "FfiOption_FfiString".to_string(),
            Type::Record(name) | Type::Enum(name) => format!("FfiOption_{}", name),
            Type::Vec(_) => String::new(),
            _ => String::new(),
        }
    }

    pub fn inner_primitive(&self) -> Option<Primitive> {
        self.effective_inner().primitive()
    }

    pub fn is_primitive(&self) -> bool {
        self.effective_inner().is_primitive()
    }

    pub fn is_string(&self) -> bool {
        self.effective_inner().is_string()
    }

    pub fn is_record(&self) -> bool {
        self.effective_inner().is_record()
    }

    pub fn is_enum(&self) -> bool {
        self.effective_inner().is_enum()
    }

    pub fn effective_inner(&self) -> &Type {
        if self.is_vec { self.inner.vec_inner().unwrap_or(&self.inner) } else { &self.inner }
    }

    pub fn inner_name(&self) -> Option<&str> {
        self.effective_inner().named_type()
    }

    pub fn struct_size(&self, module: &Module) -> usize {
        self.inner_name().map(|name| module.struct_size(name)).unwrap_or(0)
    }

    pub fn is_data_enum(&self, module: &Module) -> bool {
        self.inner_name().map(|name| module.is_data_enum(name)).unwrap_or(false)
    }
}
