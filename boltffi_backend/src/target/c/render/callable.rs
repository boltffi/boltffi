//! Semantic sync callable rendering for the package-prefixed C facade.

use super::{prefix::PackagePrefix, surface};
use crate::{
    bridge::c,
    core::{Emitted, Error, RenderContext, Result},
    target::c::name_style::Name,
};
use boltffi_binding::{
    ErrorChannel, ErrorPlacement, ExportedCallable, HandleTarget, Native, ParamPlan, Primitive,
    Receive, ReturnPlan, TypeRef,
};

pub enum Receiver<'a> {
    None,
    Class {
        c_type: &'a str,
        receive: Receive,
    },
    EncodedRecord {
        c_type: &'a str,
        id: boltffi_binding::RecordId,
        receive: Receive,
    },
    DirectRecord {
        c_type: &'a str,
        receive: Receive,
    },
}

pub fn render(
    abi: &c::Function,
    callable: &ExportedCallable<Native>,
    wrapper_name: &str,
    result_stem: &str,
    receiver: Receiver<'_>,
    context: &RenderContext<Native>,
) -> Result<Emitted> {
    if callable.execution().uses_async_execution() {
        return unsupported("async callable");
    }
    let mut state = State::new(abi, context);
    state.receiver(receiver)?;
    for param in callable.params() {
        let Some(plan) = param.payload().as_value() else {
            return unsupported("closure callable parameter");
        };
        state.param(Name::new(param.name()).member(), plan)?;
    }
    let params = if state.params.is_empty() {
        "void".to_owned()
    } else {
        state.params.join(", ")
    };
    let args = state.args.join(", ");
    let error = callable.error().channel();
    let output = match error {
        ErrorChannel::None => {
            state.render_infallible(callable.returns().plan(), wrapper_name, &params, &args)?
        }
        ErrorChannel::Status => return unsupported("status-channel semantic callable"),
        ErrorChannel::Encoded {
            placement: ErrorPlacement::ReturnSlot,
            ty: TypeRef::String,
            ..
        } => state.render_result(
            callable.returns().plan(),
            wrapper_name,
            result_stem,
            &params,
            &args,
        )?,
        ErrorChannel::Encoded { .. } => return unsupported("fallible callable error shape"),
        _ => return unsupported("unknown callable error channel"),
    };
    Ok(Emitted::primary(output))
}

struct State<'a> {
    abi: &'a c::Function,
    context: &'a RenderContext<'a, Native>,
    params: Vec<String>,
    args: Vec<String>,
    setup: Vec<String>,
    cleanup: Vec<String>,
    next: usize,
}
impl<'a> State<'a> {
    fn new(abi: &'a c::Function, context: &'a RenderContext<'a, Native>) -> Self {
        Self {
            abi,
            context,
            params: vec![],
            args: vec![],
            setup: vec![],
            cleanup: vec![],
            next: 0,
        }
    }
    fn local(&mut self, stem: &str) -> String {
        let n = format!("boltffi_{stem}_{}", self.next);
        self.next += 1;
        n
    }
    fn receiver(&mut self, receiver: Receiver<'_>) -> Result<()> {
        match receiver {
            Receiver::None => {}
            Receiver::Class { c_type, receive } => {
                let q = if receive == Receive::ByRef {
                    "const "
                } else {
                    ""
                };
                self.params.push(format!("{q}{c_type} *receiver"));
                self.args.push("receiver->_boltffi_handle".into());
            }
            Receiver::EncodedRecord {
                c_type,
                id,
                receive,
            } => {
                if receive == Receive::ByMutRef {
                    return unsupported("mutable encoded record receiver");
                }
                let q = "const ";
                self.params.push(format!("{q}{c_type} *receiver"));
                self.pack_record("receiver", id, true)?;
            }
            Receiver::DirectRecord { c_type, receive } => match receive {
                Receive::ByRef => {
                    self.params.push(format!("const {c_type} *receiver"));
                    self.args.push("*receiver".into())
                }
                Receive::ByValue => {
                    self.params.push(format!("{c_type} receiver"));
                    self.args.push("receiver".into())
                }
                Receive::ByMutRef => return unsupported("mutable direct record receiver"),
                _ => return unsupported("unknown receiver"),
            },
        }
        Ok(())
    }
    fn param(
        &mut self,
        name: String,
        plan: &ParamPlan<Native, boltffi_binding::IntoRust>,
    ) -> Result<()> {
        match plan {
            ParamPlan::Direct { ty, receive } => {
                let c = surface::direct_value_type(ty, self.context)?;
                match (ty, receive) {
                    (boltffi_binding::DirectValueType::Record(_), Receive::ByRef) => {
                        self.params.push(format!("const {c} *{name}"));
                        self.args.push(name)
                    }
                    (boltffi_binding::DirectValueType::Record(_), Receive::ByMutRef) => {
                        return unsupported("mutable direct record parameter");
                    }
                    _ => {
                        self.params.push(format!("{c} {name}"));
                        self.args.push(name)
                    }
                }
            }
            ParamPlan::Encoded { ty, receive, .. } => {
                if *receive == Receive::ByMutRef {
                    return unsupported("mutable encoded parameter");
                }
                match ty {
                    TypeRef::String | TypeRef::Bytes => {
                        let c = surface::value_type(ty, self.context, surface::ValueUse::Param)?;
                        self.params.push(format!("{c} {name}"));
                        self.pack_view(&name);
                    }
                    TypeRef::Record(id) => {
                        let c = surface::value_type(ty, self.context, surface::ValueUse::Param)?;
                        let by_ptr = *receive == Receive::ByRef;
                        self.params.push(if by_ptr {
                            format!("const {c} *{name}")
                        } else {
                            format!("{c} {name}")
                        });
                        self.pack_record(&name, *id, by_ptr)?;
                    }
                    TypeRef::Optional(inner) if **inner == TypeRef::String => {
                        let c = surface::value_type(ty, self.context, surface::ValueUse::Param)?;
                        self.params.push(format!("{c} {name}"));
                        self.pack_option_string(&name);
                    }
                    TypeRef::Sequence(element) => {
                        let c = surface::value_type(ty, self.context, surface::ValueUse::Param)?;
                        self.params.push(format!("{c} {name}"));
                        self.pack_sequence(&name, element)?;
                    }
                    _ => return unsupported("encoded callable parameter"),
                }
            }
            ParamPlan::ScalarOption { primitive }
                if matches!(primitive, Primitive::U32 | Primitive::F32) =>
            {
                let c = surface::value_type(
                    &TypeRef::Optional(Box::new(TypeRef::Primitive(*primitive))),
                    self.context,
                    surface::ValueUse::Param,
                )?;
                self.params.push(format!("{c} {name}"));
                self.pack_scalar_option(&name, *primitive);
            }
            ParamPlan::DirectVec { element, .. } => {
                let c =
                    surface::direct_vector_type(element, self.context, surface::ValueUse::Param)?;
                self.params.push(format!("{c} {name}"));
                self.args.push(format!("{name}.ptr"));
                self.args.push(format!("{name}.len"));
            }
            ParamPlan::Handle {
                target: HandleTarget::Callback(id),
                ..
            } => {
                let c = super::callback::handle_type_name(*id, self.context)?;
                self.params.push(format!("{c} {name}"));
                self.args.push(format!("{name}.raw"));
            }
            ParamPlan::Handle {
                target: HandleTarget::Class(id),
                ..
            } => {
                let class = self.context.class(*id).ok_or(Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "missing class parameter",
                })?;
                let c = PackagePrefix::from_context(self.context)
                    .type_name(&Name::new(class.name()).r#type());
                self.params.push(format!("const {c} *{name}"));
                self.args.push(format!("{name}->_boltffi_handle"));
            }
            _ => return unsupported("callable parameter plan"),
        }
        Ok(())
    }
    fn pack_record(
        &mut self,
        name: &str,
        id: boltffi_binding::RecordId,
        is_ptr: bool,
    ) -> Result<()> {
        if !matches!(
            self.context.record(id),
            Some(boltffi_binding::RecordDecl::Encoded(_))
        ) {
            return unsupported("encoded parameter references direct record");
        }
        let size = self.local("size");
        let buf = self.local("buf");
        let writer = self.local("writer");
        let value = if is_ptr {
            name.to_owned()
        } else {
            format!("&{name}")
        };
        self.setup.push(format!(
            "    uintptr_t {size} = {}({value});",
            surface::record_helper_name(id, self.context, "size")?
        ));
        self.setup.push(format!(
            "    FfiBuf_u8 {buf} = boltffi_buf_with_len({size});"
        ));
        self.setup.push(format!(
            "    BoltFFICWireWriter {writer} = {{ {buf}.ptr, {buf}.len, 0, true }};"
        ));
        self.setup.push(format!(
            "    {}(&{writer}, {value});",
            surface::record_helper_name(id, self.context, "encode")?
        ));
        self.args.push(format!("{buf}.ptr"));
        self.args.push(format!("{buf}.len"));
        self.cleanup.push(format!("    boltffi_free_buf({buf});"));
        Ok(())
    }
    fn pack_view(&mut self, name: &str) {
        let buf = self.local("buf");
        let writer = self.local("writer");
        let p = package_member(self.context);
        self.setup.push(format!(
            "    FfiBuf_u8 {buf}=boltffi_buf_with_len(4+{name}.len);"
        ));
        self.setup.push(format!(
            "    BoltFFICWireWriter {writer}={{{buf}.ptr,{buf}.len,0,true}};"
        ));
        self.setup.push(format!("    boltffi_c_{p}_write_u32(&{writer},(uint32_t){name}.len); boltffi_c_{p}_write(&{writer},{name}.ptr,{name}.len);"));
        self.args.push(format!("{buf}.ptr"));
        self.args.push(format!("{buf}.len"));
        self.cleanup.push(format!("    boltffi_free_buf({buf});"));
    }
    fn pack_sequence(&mut self, name: &str, element: &TypeRef) -> Result<()> {
        let size = self.local("size");
        let i = self.local("i");
        let buf = self.local("buf");
        let writer = self.local("writer");
        let p = package_member(self.context);
        let element_size = match element {
            TypeRef::Primitive(v) => format!(
                "{}",
                match v {
                    Primitive::Bool | Primitive::I8 | Primitive::U8 => 1,
                    Primitive::I16 | Primitive::U16 => 2,
                    Primitive::I32 | Primitive::U32 | Primitive::F32 => 4,
                    _ => 8,
                }
            ),
            TypeRef::Record(id)
                if matches!(
                    self.context.record(*id),
                    Some(boltffi_binding::RecordDecl::Direct(_))
                ) =>
            {
                format!(
                    "sizeof({})",
                    surface::value_type(element, self.context, surface::ValueUse::Field)?
                )
            }
            TypeRef::Record(id) => format!(
                "{}(&{name}.ptr[{i}])",
                surface::record_helper_name(*id, self.context, "size")?
            ),
            _ => return unsupported("encoded sequence parameter element"),
        };
        self.setup.push(format!("    uintptr_t {size}=4; for (uintptr_t {i}=0; {i}<{name}.len; ++{i}) {size}+={element_size};"));
        self.setup.push(format!("    FfiBuf_u8 {buf}=boltffi_buf_with_len({size}); BoltFFICWireWriter {writer}={{{buf}.ptr,{buf}.len,0,true}}; boltffi_c_{p}_write_u32(&{writer},(uint32_t){name}.len);"));
        match element {
            TypeRef::String => self.setup.push(format!("    for (uintptr_t {i}=0; {i}<{name}.len; ++{i}) {{ boltffi_c_{p}_write_u32(&{writer},(uint32_t){name}.ptr[{i}].len); boltffi_c_{p}_write(&{writer},{name}.ptr[{i}].ptr,{name}.ptr[{i}].len); }}")),
            TypeRef::Record(id) if matches!(self.context.record(*id),Some(boltffi_binding::RecordDecl::Encoded(_)))=>self.setup.push(format!("    for (uintptr_t {i}=0; {i}<{name}.len; ++{i}) {}(&{writer},&{name}.ptr[{i}]);",surface::record_helper_name(*id,self.context,"encode")?)),
            _=>self.setup.push(format!("    boltffi_c_{p}_write(&{writer},{name}.ptr,{name}.len*({element_size}));"))
        }
        self.args.push(format!("{buf}.ptr"));
        self.args.push(format!("{buf}.len"));
        self.cleanup.push(format!("    boltffi_free_buf({buf});"));
        Ok(())
    }
    fn pack_scalar_option(&mut self, name: &str, _primitive: Primitive) {
        let buf = self.local("buf");
        let writer = self.local("writer");
        let p = package_member(self.context);
        self.setup.push(format!(
            "    FfiBuf_u8 {buf}=boltffi_buf_with_len({name}.has_value ? 5 : 1);"
        ));
        self.setup.push(format!(
            "    BoltFFICWireWriter {writer}={{{buf}.ptr,{buf}.len,0,true}};"
        ));
        self.setup.push(format!(
            "    boltffi_c_{p}_write_u8(&{writer},{name}.has_value ? 1 : 0);"
        ));
        self.setup.push(format!(
            "    if ({name}.has_value) boltffi_c_{p}_write(&{writer},&{name}.value,4);"
        ));
        self.args.push(format!("{buf}.ptr"));
        self.args.push(format!("{buf}.len"));
        self.cleanup.push(format!("    boltffi_free_buf({buf});"));
    }
    fn pack_option_string(&mut self, name: &str) {
        let buf = self.local("buf");
        let writer = self.local("writer");
        let p = package_member(self.context);
        self.setup.push(format!("    FfiBuf_u8 {buf}=boltffi_buf_with_len(1 + ({name}.has_value ? 4 + {name}.value.len : 0));"));
        self.setup.push(format!(
            "    BoltFFICWireWriter {writer}={{{buf}.ptr,{buf}.len,0,true}};"
        ));
        self.setup.push(format!(
            "    boltffi_c_{p}_write_u8(&{writer},{name}.has_value ? 1 : 0);"
        ));
        self.setup.push(format!("    if ({name}.has_value) {{ boltffi_c_{p}_write_u32(&{writer},(uint32_t){name}.value.len); boltffi_c_{p}_write(&{writer},{name}.value.ptr,{name}.value.len); }}"));
        self.args.push(format!("{buf}.ptr"));
        self.args.push(format!("{buf}.len"));
        self.cleanup.push(format!("    boltffi_free_buf({buf});"));
    }
    fn render_infallible(
        &mut self,
        plan: &ReturnPlan<Native, boltffi_binding::OutOfRust>,
        name: &str,
        params: &str,
        args: &str,
    ) -> Result<String> {
        let semantic = return_type(plan, self.context)?;
        let mut b = format!("static inline {semantic} {name}({params}) {{\n");
        for l in &self.setup {
            b.push_str(l);
            b.push('\n')
        }
        match plan {
            ReturnPlan::Void => {
                b.push_str(&format!("    {}({args});\n", self.abi.name()));
                for l in &self.cleanup {
                    b.push_str(l);
                    b.push('\n')
                }
            }
            ReturnPlan::DirectViaReturnSlot { .. } => {
                b.push_str(&format!(
                    "    {semantic} boltffi_result = ({semantic}){}({args});\n",
                    self.abi.name()
                ));
                for l in &self.cleanup {
                    b.push_str(l);
                    b.push('\n')
                }
                b.push_str("    return boltffi_result;\n")
            }
            ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Class(_),
                ..
            } => {
                b.push_str(&format!(
                    "    {semantic} boltffi_result; boltffi_result._boltffi_handle = {}({args});\n",
                    self.abi.name()
                ));
                for l in &self.cleanup {
                    b.push_str(l);
                    b.push('\n')
                }
                b.push_str("    return boltffi_result;\n")
            }
            ReturnPlan::EncodedViaReturnSlot { ty, .. } => {
                b.push_str(&format!(
                    "    FfiBuf_u8 boltffi_raw = {}({args});\n",
                    self.abi.name()
                ));
                for l in &self.cleanup {
                    b.push_str(l);
                    b.push('\n')
                }
                b.push_str(&decode_owned(
                    ty,
                    "boltffi_raw",
                    "boltffi_result",
                    self.context,
                    true,
                )?);
                b.push_str("    return boltffi_result;\n")
            }
            ReturnPlan::ScalarOptionViaReturnSlot {
                primitive: Primitive::U32 | Primitive::F32,
                ..
            } => {
                let p = package_member(self.context);
                b.push_str(&format!(
                    "    FfiBuf_u8 boltffi_raw = {}({args});\n",
                    self.abi.name()
                ));
                for l in &self.cleanup {
                    b.push_str(l);
                    b.push('\n');
                }
                b.push_str(&format!("    {semantic} boltffi_result; memset(&boltffi_result,0,sizeof(boltffi_result)); BoltFFICWireReader boltffi_reader={{boltffi_raw.ptr,boltffi_raw.len,0,true}}; uint8_t boltffi_tag=boltffi_c_{p}_read_u8(&boltffi_reader); if (boltffi_tag==1) {{ boltffi_result.has_value=true; boltffi_c_{p}_read(&boltffi_reader,&boltffi_result.value,4); }} boltffi_free_buf(boltffi_raw); return boltffi_result;\n"));
            }
            ReturnPlan::DirectVecViaReturnSlot { element } => {
                b.push_str(&format!(
                    "    FfiBuf_u8 boltffi_raw = {}({args});\n",
                    self.abi.name()
                ));
                for l in &self.cleanup {
                    b.push_str(l);
                    b.push('\n')
                }
                let elem = surface::direct_vector_element_type(element, self.context)?;
                b.push_str(&format!("    {semantic} boltffi_result; boltffi_result.len=boltffi_raw.len/sizeof({elem}); boltffi_result.ptr=({elem} *)malloc(boltffi_raw.len); if (boltffi_raw.len) memcpy(boltffi_result.ptr,boltffi_raw.ptr,boltffi_raw.len); boltffi_free_buf(boltffi_raw);\n    return boltffi_result;\n"));
            }
            _ => return unsupported("callable return plan"),
        }
        b.push_str("}\n");
        Ok(b)
    }
    fn render_result(
        &mut self,
        plan: &ReturnPlan<Native, boltffi_binding::OutOfRust>,
        name: &str,
        stem: &str,
        params: &str,
        args: &str,
    ) -> Result<String> {
        let ok = return_type(plan, self.context)?;
        let package_type_prefix = package_pascal(self.context);
        let result_ty = format!("{stem}Result");
        let raw_decl = raw_success_decl(plan)?;
        let mut call_args = args.to_owned();
        if !call_args.is_empty() {
            call_args.push_str(", ")
        }
        call_args.push_str("&boltffi_success");
        let mut b = format!(
            "typedef struct {{ bool ok; union {{ {ok} value; {package_type_prefix}String error; }} data; }} {result_ty};\nstatic inline {result_ty} {name}({params}) {{\n"
        );
        for l in &self.setup {
            b.push_str(l);
            b.push('\n')
        }
        b.push_str(&format!(
            "    {raw_decl} boltffi_success;\n    FfiBuf_u8 boltffi_error = {}({call_args});\n",
            self.abi.name()
        ));
        for l in &self.cleanup {
            b.push_str(l);
            b.push('\n')
        }
        b.push_str(&format!("    {result_ty} boltffi_result; memset(&boltffi_result,0,sizeof(boltffi_result));\n    if (boltffi_error.len == 0) {{ boltffi_result.ok=true;\n"));
        b.push_str(&decode_success(
            plan,
            "boltffi_success",
            "boltffi_result.data.value",
            self.context,
        )?);
        let package = package_member(self.context);
        b.push_str(&format!("        return boltffi_result;\n    }}\n    boltffi_result.ok=false; BoltFFICWireReader boltffi_error_reader={{boltffi_error.ptr,boltffi_error.len,0,true}}; boltffi_c_{package}_copy_string(&boltffi_error_reader,&boltffi_result.data.error); boltffi_free_buf(boltffi_error); return boltffi_result;\n}}\n"));
        let free_success = free_success_stmt(plan, "value", self.context)?;
        b.push_str(&format!("static inline void {name}_result_free({result_ty} *value) {{ if (value == NULL) return; if (value->ok) {{ {free_success} }} else {{ {package}_string_free(&value->data.error); }} memset(value,0,sizeof(*value)); }}\n"));
        Ok(b)
    }
}

fn return_type(
    plan: &ReturnPlan<Native, boltffi_binding::OutOfRust>,
    context: &RenderContext<Native>,
) -> Result<String> {
    match plan {
        ReturnPlan::Void => Ok("void".into()),
        ReturnPlan::DirectViaReturnSlot { ty } | ReturnPlan::DirectViaOutPointer { ty } => {
            surface::direct_value_type(ty, context)
        }
        ReturnPlan::EncodedViaReturnSlot { ty, .. }
        | ReturnPlan::EncodedViaOutPointer { ty, .. } => {
            surface::value_type(ty, context, surface::ValueUse::Return)
        }
        ReturnPlan::HandleViaReturnSlot {
            target: HandleTarget::Class(id),
            ..
        }
        | ReturnPlan::HandleViaOutPointer {
            target: HandleTarget::Class(id),
            ..
        } => {
            let c = context.class(*id).ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing returned class",
            })?;
            Ok(PackagePrefix::from_context(context).type_name(&Name::new(c.name()).r#type()))
        }
        ReturnPlan::ScalarOptionViaReturnSlot {
            primitive: Primitive::U32,
            ..
        } => Ok(format!("{}OptionU32", package_pascal(context))),
        ReturnPlan::ScalarOptionViaReturnSlot {
            primitive: Primitive::F32,
            ..
        } => Ok(format!("{}OptionF32", package_pascal(context))),
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            surface::direct_vector_type(element, context, surface::ValueUse::Return)
        }
        _ => unsupported("semantic return type"),
    }
}
fn raw_success_decl(plan: &ReturnPlan<Native, boltffi_binding::OutOfRust>) -> Result<&'static str> {
    match plan {
        ReturnPlan::DirectViaOutPointer {
            ty: boltffi_binding::DirectValueType::Primitive(Primitive::U32),
        } => Ok("uint32_t"),
        ReturnPlan::DirectViaOutPointer { .. } => Ok("uint64_t"),
        ReturnPlan::EncodedViaOutPointer { .. } => Ok("FfiBuf_u8"),
        ReturnPlan::HandleViaOutPointer { .. } => Ok("uint64_t"),
        _ => unsupported("fallible success transport"),
    }
}
fn decode_success(
    plan: &ReturnPlan<Native, boltffi_binding::OutOfRust>,
    raw: &str,
    out: &str,
    context: &RenderContext<Native>,
) -> Result<String> {
    match plan {
        ReturnPlan::DirectViaOutPointer { .. } => Ok(format!(
            "        {out}=({}){raw};\n",
            return_type(plan, context)?
        )),
        ReturnPlan::HandleViaOutPointer { .. } => {
            Ok(format!("        {out}._boltffi_handle={raw};\n"))
        }
        ReturnPlan::EncodedViaOutPointer { ty, .. } => decode_owned(ty, raw, out, context, false),
        _ => unsupported("fallible success decode"),
    }
}
fn free_success_stmt(
    plan: &ReturnPlan<Native, boltffi_binding::OutOfRust>,
    field: &str,
    context: &RenderContext<Native>,
) -> Result<String> {
    let p = package_member(context);
    match plan {
        ReturnPlan::EncodedViaOutPointer {
            ty: TypeRef::String,
            ..
        } => Ok(format!("{p}_string_free(&value->data.{field});")),
        ReturnPlan::EncodedViaOutPointer {
            ty: TypeRef::Bytes, ..
        } => Ok(format!("{p}_bytes_free(&value->data.{field});")),
        ReturnPlan::EncodedViaOutPointer {
            ty: TypeRef::Record(id),
            ..
        } => Ok(format!(
            "{p}_{}_free(&value->data.{field});",
            Name::new(context.record(*id).expect("record").name()).member()
        )),
        ReturnPlan::EncodedViaOutPointer {
            ty: TypeRef::Sequence(element),
            ..
        } => match element.as_ref() {
            TypeRef::String => Ok(format!("{p}_string_sequence_free(&value->data.{field});")),
            TypeRef::Record(id) => Ok(format!(
                "{p}_{}_sequence_free(&value->data.{field});",
                Name::new(context.record(*id).expect("record").name()).member()
            )),
            TypeRef::Primitive(v) => Ok(format!(
                "{p}_{}_sequence_free(&value->data.{field});",
                primitive_member(*v)
            )),
            _ => unsupported("result sequence free"),
        },
        ReturnPlan::HandleViaOutPointer {
            target: HandleTarget::Class(id),
            ..
        } => {
            let class = context.class(*id).expect("class");
            Ok(format!(
                "if (value->data.{field}._boltffi_handle) {}(value->data.{field}._boltffi_handle);",
                class.release().name().as_str()
            ))
        }
        ReturnPlan::DirectViaOutPointer { .. } => Ok(String::new()),
        _ => unsupported("result success free"),
    }
}
fn primitive_member(v: Primitive) -> &'static str {
    match v {
        Primitive::Bool => "bool",
        Primitive::I8 => "i8",
        Primitive::U8 => "u8",
        Primitive::I16 => "i16",
        Primitive::U16 => "u16",
        Primitive::I32 => "i32",
        Primitive::U32 => "u32",
        Primitive::I64 => "i64",
        Primitive::U64 => "u64",
        Primitive::ISize => "isize",
        Primitive::USize => "usize",
        Primitive::F32 => "f32",
        Primitive::F64 => "f64",
        _ => "unsupported",
    }
}

fn decode_owned(
    ty: &TypeRef,
    raw: &str,
    out: &str,
    context: &RenderContext<Native>,
    declare: bool,
) -> Result<String> {
    let p = package_member(context);
    let c = surface::value_type(ty, context, surface::ValueUse::Return)?;
    let init = if declare {
        format!("{c} {out}; ")
    } else {
        String::new()
    };
    match ty {
        TypeRef::String => Ok(format!(
            "    {init}memset(&{out},0,sizeof({out})); BoltFFICWireReader boltffi_reader={{{raw}.ptr,{raw}.len,0,true}}; boltffi_c_{p}_copy_string(&boltffi_reader,&{out}); boltffi_free_buf({raw});\n"
        )),
        TypeRef::Bytes => Ok(format!(
            "    {init}memset(&{out},0,sizeof({out})); BoltFFICWireReader boltffi_reader={{{raw}.ptr,{raw}.len,0,true}}; boltffi_c_{p}_copy_bytes(&boltffi_reader,&{out}); boltffi_free_buf({raw});\n"
        )),
        TypeRef::Record(id) => Ok(format!(
            "    {init}BoltFFICWireReader boltffi_reader={{{raw}.ptr,{raw}.len,0,true}}; {}(&boltffi_reader,&{out}); boltffi_free_buf({raw});\n",
            surface::record_helper_name(*id, context, "decode")?
        )),
        TypeRef::Sequence(element) => {
            let elem = surface::value_type(element, context, surface::ValueUse::Field)?;
            let decode = match element.as_ref() {
                TypeRef::String => format!(
                    "if (!boltffi_c_{p}_copy_string(&boltffi_reader,&{out}.ptr[boltffi_i])) {{ {p}_string_sequence_free(&{out}); break; }}"
                ),
                TypeRef::Primitive(v) => format!(
                    "boltffi_c_{p}_read(&boltffi_reader,&{out}.ptr[boltffi_i],{});",
                    match v {
                        Primitive::Bool | Primitive::I8 | Primitive::U8 => 1,
                        Primitive::I16 | Primitive::U16 => 2,
                        Primitive::I32 | Primitive::U32 | Primitive::F32 => 4,
                        _ => 8,
                    }
                ),
                TypeRef::Record(id)
                    if matches!(
                        context.record(*id),
                        Some(boltffi_binding::RecordDecl::Direct(_))
                    ) =>
                {
                    format!(
                        "boltffi_c_{p}_read(&boltffi_reader,&{out}.ptr[boltffi_i],sizeof({elem}));"
                    )
                }
                TypeRef::Record(id) => format!(
                    "if (!{}(&boltffi_reader,&{out}.ptr[boltffi_i])) {{ {p}_{}_sequence_free(&{out}); break; }}",
                    surface::record_helper_name(*id, context, "decode")?,
                    Name::new(context.record(*id).expect("record").name()).member()
                ),
                _ => return unsupported("owned sequence return element"),
            };
            Ok(format!(
                "    {init}memset(&{out},0,sizeof({out})); BoltFFICWireReader boltffi_reader={{{raw}.ptr,{raw}.len,0,true}}; uint32_t boltffi_count=boltffi_c_{p}_read_u32(&boltffi_reader); {out}.ptr=({elem} *)calloc(boltffi_count,sizeof({elem})); {out}.len=boltffi_count; if (boltffi_count && {out}.ptr == NULL) boltffi_reader.ok=false; for (uintptr_t boltffi_i=0; boltffi_reader.ok && boltffi_i<boltffi_count; ++boltffi_i) {{ {decode} }} boltffi_free_buf({raw});\n"
            ))
        }
        _ => unsupported("owned encoded return"),
    }
}
fn package_member(context: &RenderContext<Native>) -> String {
    Name::new(context.bindings().package().name()).member()
}
fn package_pascal(context: &RenderContext<Native>) -> String {
    Name::new(context.bindings().package().name()).r#type()
}
fn unsupported<T>(shape: &'static str) -> Result<T> {
    Err(Error::UnsupportedTarget { target: "c", shape })
}
