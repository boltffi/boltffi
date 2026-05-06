use super::super::ast::{
    CSharpClassName, CSharpLocalName, CSharpMethodName, CSharpParamName, CSharpType,
};
use super::CFunctionName;

#[derive(Debug, Clone)]
pub struct CSharpCallbackPlan {
    pub public_name: CSharpClassName,
    pub bridge_name: CSharpClassName,
    pub methods: Vec<CSharpCallbackMethodPlan>,
    pub register_fn: CFunctionName,
    pub create_fn: CFunctionName,
    pub has_async_methods: bool,
    pub needs_wire_reader: bool,
    pub needs_wire_writer: bool,
    pub needs_ffi_buf: bool,
}

#[derive(Debug, Clone)]
pub struct CSharpClosurePlan {
    pub public_name: CSharpClassName,
    pub bridge_name: CSharpClassName,
    pub method: CSharpClosureMethodPlan,
    pub needs_wire_reader: bool,
    pub needs_wire_writer: bool,
    pub needs_ffi_buf: bool,
}

#[derive(Debug, Clone)]
pub struct CSharpCallbackMethodPlan {
    pub name: CSharpMethodName,
    pub vtable_field: CSharpLocalName,
    pub return_type: CSharpType,
    pub is_async: bool,
    pub public_params: Vec<CSharpCallbackParamPlan>,
    pub entry_source: String,
    pub proxy_source: String,
    pub delegate_source: String,
}

#[derive(Debug, Clone)]
pub struct CSharpClosureMethodPlan {
    pub return_type: CSharpType,
    pub public_params: Vec<CSharpCallbackParamPlan>,
    pub native_return_type: String,
    pub native_decls: Vec<String>,
    pub marshals_return_bool: bool,
    pub decode_setup: Vec<String>,
    pub invoke_body: String,
}

#[derive(Debug, Clone)]
pub struct CSharpCallbackParamPlan {
    pub csharp_type: CSharpType,
    pub name: CSharpParamName,
}

impl CSharpCallbackMethodPlan {
    pub fn public_return_type(&self) -> String {
        if !self.is_async {
            return self.return_type.to_string();
        }
        if self.return_type.is_void() {
            "global::System.Threading.Tasks.Task".to_string()
        } else {
            format!("global::System.Threading.Tasks.Task<{}>", self.return_type)
        }
    }
}
