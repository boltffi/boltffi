use super::super::ast::{
    CSharpArgumentList, CSharpBinaryOp, CSharpClassName, CSharpExpression, CSharpIdentity,
    CSharpLiteral, CSharpLocalName, CSharpMethodName, CSharpParamName, CSharpParameter,
    CSharpPropertyName, CSharpType, CSharpTypeReference,
};
use super::CFunctionName;
use super::callable::CSharpWireWriterPlan;

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

#[derive(Debug, Clone)]
pub enum CSharpCallbackBridgeParamPlan {
    Direct {
        public_param: CSharpParameter,
        native_param: CSharpParameter,
        decoded_arg: CSharpExpression,
        proxy_arg: CSharpExpression,
    },
    WireEncoded {
        public_param: CSharpParameter,
        native_ptr_param: CSharpParameter,
        native_len_param: CSharpParameter,
        reader_local: CSharpLocalName,
        decoded_arg: CSharpExpression,
        writer: CSharpWireWriterPlan,
        pin_local: CSharpLocalName,
        ptr_local: CSharpLocalName,
    },
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

impl CSharpCallbackBridgeParamPlan {
    pub fn public_param(&self) -> &CSharpParameter {
        match self {
            Self::Direct { public_param, .. } | Self::WireEncoded { public_param, .. } => {
                public_param
            }
        }
    }

    pub fn native_params(&self) -> Vec<CSharpParameter> {
        match self {
            Self::Direct { native_param, .. } => vec![native_param.clone()],
            Self::WireEncoded {
                native_ptr_param,
                native_len_param,
                ..
            } => vec![native_ptr_param.clone(), native_len_param.clone()],
        }
    }

    pub fn decoded_arg(&self) -> &CSharpExpression {
        match self {
            Self::Direct { decoded_arg, .. } | Self::WireEncoded { decoded_arg, .. } => decoded_arg,
        }
    }

    pub fn proxy_args(&self) -> CSharpArgumentList {
        match self {
            Self::Direct { proxy_arg, .. } => vec![proxy_arg.clone()].into(),
            Self::WireEncoded {
                writer, ptr_local, ..
            } => vec![
                local_expr(ptr_local),
                CSharpExpression::Cast {
                    target: CSharpType::UIntPtr,
                    inner: Box::new(bytes_length_expr(&writer.bytes_binding_name)),
                },
            ]
            .into(),
        }
    }

    pub fn decode_setup_lines(&self) -> Vec<String> {
        match self {
            Self::Direct { .. } => vec![],
            Self::WireEncoded {
                native_ptr_param,
                native_len_param,
                reader_local,
                ..
            } => vec![format!(
                "var {reader_local} = new WireReader({}, {});",
                native_ptr_param.name, native_len_param.name
            )],
        }
    }

    pub fn proxy_setup_lines(&self) -> Vec<String> {
        match self {
            Self::Direct { .. } => vec![],
            Self::WireEncoded {
                writer,
                pin_local,
                ptr_local,
                ..
            } => {
                let mut lines = vec![
                    format!("byte[] {};", writer.bytes_binding_name),
                    format!(
                        "using (var {} = new WireWriter({}))",
                        writer.binding_name, writer.size_expr
                    ),
                    "{".to_string(),
                ];
                for stmt in &writer.encode_stmts {
                    lines.push(format!("    {stmt};"));
                }
                lines.push(format!(
                    "    {} = {};",
                    writer.bytes_binding_name,
                    CSharpExpression::MethodCall {
                        receiver: Box::new(local_expr(&writer.binding_name)),
                        method: CSharpMethodName::new("ToArray"),
                        type_args: vec![],
                        args: CSharpArgumentList::empty(),
                    }
                ));
                lines.push("}".to_string());
                lines.push(format!(
                    "{} {pin_local} = {};",
                    named_type("GCHandle"),
                    CSharpExpression::Literal(CSharpLiteral::Default)
                ));
                lines.push(format!(
                    "{} {ptr_local} = {};",
                    CSharpType::IntPtr,
                    int_ptr_zero()
                ));
                lines
            }
        }
    }

    pub fn proxy_pin_lines(&self) -> Vec<String> {
        match self {
            Self::Direct { .. } => vec![],
            Self::WireEncoded {
                writer,
                pin_local,
                ptr_local,
                ..
            } => {
                let bytes = local_expr(&writer.bytes_binding_name);
                let cond = CSharpExpression::Binary {
                    op: CSharpBinaryOp::Ne,
                    left: Box::new(bytes_length_expr(&writer.bytes_binding_name)),
                    right: Box::new(CSharpExpression::Literal(CSharpLiteral::Int(0))),
                };
                let alloc = CSharpExpression::MethodCall {
                    receiver: Box::new(type_expr("GCHandle")),
                    method: CSharpMethodName::new("Alloc"),
                    type_args: vec![],
                    args: vec![
                        bytes,
                        CSharpExpression::MemberAccess {
                            receiver: Box::new(type_expr("GCHandleType")),
                            name: CSharpPropertyName::from_source("pinned"),
                        },
                    ]
                    .into(),
                };
                let addr = CSharpExpression::MethodCall {
                    receiver: Box::new(local_expr(pin_local)),
                    method: CSharpMethodName::new("AddrOfPinnedObject"),
                    type_args: vec![],
                    args: CSharpArgumentList::empty(),
                };
                vec![
                    format!("if ({cond})"),
                    "{".to_string(),
                    format!("    {pin_local} = {alloc};"),
                    format!("    {ptr_local} = {addr};"),
                    "}".to_string(),
                ]
            }
        }
    }

    pub fn proxy_cleanup_lines(&self) -> Vec<String> {
        match self {
            Self::Direct { .. } => vec![],
            Self::WireEncoded { pin_local, .. } => {
                let cond = CSharpExpression::MemberAccess {
                    receiver: Box::new(local_expr(pin_local)),
                    name: CSharpPropertyName::from_source("is_allocated"),
                };
                let free = CSharpExpression::MethodCall {
                    receiver: Box::new(local_expr(pin_local)),
                    method: CSharpMethodName::new("Free"),
                    type_args: vec![],
                    args: CSharpArgumentList::empty(),
                };
                vec![format!("if ({cond}) {free};")]
            }
        }
    }

    pub fn needs_wire_reader(&self) -> bool {
        matches!(self, Self::WireEncoded { .. })
    }

    pub fn needs_wire_writer(&self) -> bool {
        matches!(self, Self::WireEncoded { .. })
    }
}

fn local_expr(name: &CSharpLocalName) -> CSharpExpression {
    CSharpExpression::Identity(CSharpIdentity::Local(name.clone()))
}

fn type_expr(name: &str) -> CSharpExpression {
    CSharpExpression::TypeRef(CSharpTypeReference::Plain(CSharpClassName::new(name)))
}

fn named_type(name: &str) -> CSharpType {
    CSharpType::Named(CSharpTypeReference::Plain(CSharpClassName::new(name)))
}

fn bytes_length_expr(bytes_local: &CSharpLocalName) -> CSharpExpression {
    CSharpExpression::MemberAccess {
        receiver: Box::new(local_expr(bytes_local)),
        name: CSharpPropertyName::from_source("length"),
    }
}

fn int_ptr_zero() -> CSharpExpression {
    CSharpExpression::MemberAccess {
        receiver: Box::new(type_expr("IntPtr")),
        name: CSharpPropertyName::from_source("zero"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::ast::CSharpStatement;
    use super::*;

    fn param_expr(name: &CSharpParamName) -> CSharpExpression {
        CSharpExpression::Identity(CSharpIdentity::Param(name.clone()))
    }

    fn local(name: &str) -> CSharpLocalName {
        CSharpLocalName::new(name)
    }

    #[test]
    fn direct_bridge_param_exposes_typed_signature_and_args() {
        let name = CSharpParamName::from_source("value");
        let plan = CSharpCallbackBridgeParamPlan::Direct {
            public_param: CSharpParameter::bare(CSharpType::Int, name.clone()),
            native_param: CSharpParameter::bare(CSharpType::Int, name.clone()),
            decoded_arg: param_expr(&name),
            proxy_arg: param_expr(&name),
        };

        assert_eq!(plan.public_param().to_string(), "int value");
        assert_eq!(
            plan.native_params()
                .into_iter()
                .map(|param| param.to_string())
                .collect::<Vec<_>>(),
            vec!["int value"]
        );
        assert_eq!(plan.decoded_arg().to_string(), "value");
        assert_eq!(plan.proxy_args().to_string(), "value");
        assert!(plan.decode_setup_lines().is_empty());
        assert!(plan.proxy_setup_lines().is_empty());
        assert!(!plan.needs_wire_reader());
        assert!(!plan.needs_wire_writer());
    }

    #[test]
    fn wire_encoded_bridge_param_derives_reader_writer_pin_and_cleanup_lines() {
        let value = CSharpParamName::from_source("value");
        let value_len = CSharpParamName::new("valueLen");
        let writer = CSharpWireWriterPlan {
            binding_name: local("_valueWire"),
            bytes_binding_name: local("_valueBytes"),
            param_name: value.clone(),
            size_expr: CSharpExpression::Literal(CSharpLiteral::Int(4)),
            encode_stmts: vec![CSharpStatement::Expression(CSharpExpression::MethodCall {
                receiver: Box::new(local_expr(&local("_valueWire"))),
                method: CSharpMethodName::new("WriteI32"),
                type_args: vec![],
                args: vec![param_expr(&value)].into(),
            })],
        };
        let plan = CSharpCallbackBridgeParamPlan::WireEncoded {
            public_param: CSharpParameter::bare(CSharpType::String, value.clone()),
            native_ptr_param: CSharpParameter::bare(CSharpType::IntPtr, value.clone()),
            native_len_param: CSharpParameter::bare(CSharpType::UIntPtr, value_len),
            reader_local: local("__boltffiValueReader"),
            decoded_arg: local_expr(&local("__boltffiValueReader")),
            writer,
            pin_local: local("_valuePin"),
            ptr_local: local("_valuePtr"),
        };

        assert_eq!(plan.public_param().to_string(), "string value");
        assert_eq!(
            plan.native_params()
                .into_iter()
                .map(|param| param.to_string())
                .collect::<Vec<_>>(),
            vec!["IntPtr value", "UIntPtr valueLen"]
        );
        assert_eq!(
            plan.decode_setup_lines(),
            vec!["var __boltffiValueReader = new WireReader(value, valueLen);"]
        );
        assert_eq!(
            plan.proxy_args().to_string(),
            "_valuePtr, (UIntPtr)_valueBytes.Length"
        );
        assert_eq!(
            plan.proxy_setup_lines(),
            vec![
                "byte[] _valueBytes;",
                "using (var _valueWire = new WireWriter(4))",
                "{",
                "    _valueWire.WriteI32(value);",
                "    _valueBytes = _valueWire.ToArray();",
                "}",
                "GCHandle _valuePin = default;",
                "IntPtr _valuePtr = IntPtr.Zero;",
            ]
        );
        assert_eq!(
            plan.proxy_pin_lines(),
            vec![
                "if (_valueBytes.Length != 0)",
                "{",
                "    _valuePin = GCHandle.Alloc(_valueBytes, GCHandleType.Pinned);",
                "    _valuePtr = _valuePin.AddrOfPinnedObject();",
                "}",
            ]
        );
        assert_eq!(
            plan.proxy_cleanup_lines(),
            vec!["if (_valuePin.IsAllocated) _valuePin.Free();"]
        );
        assert!(plan.needs_wire_reader());
        assert!(plan.needs_wire_writer());
    }
}
