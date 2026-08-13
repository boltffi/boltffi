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
    /// Semantic parameter identifiers already used in the wrapper signature;
    /// generated locals must never shadow them.
    param_names: Vec<String>,
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
            param_names: vec![],
            args: vec![],
            setup: vec![],
            cleanup: vec![],
            next: 0,
        }
    }
    fn local(&mut self, stem: &str) -> String {
        loop {
            let n = format!("boltffi_{stem}_{}", self.next);
            self.next += 1;
            if !self.param_names.iter().any(|name| name == &n) {
                return n;
            }
        }
    }
    /// Allocates a wrapper local preferring the unsuffixed spelling, falling
    /// back to the numbered counter when a parameter claims it.
    fn reserved_local(&mut self, stem: &str) -> String {
        let base = format!("boltffi_{stem}");
        if !self.param_names.iter().any(|name| name == &base) {
            return base;
        }
        self.local(stem)
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
                self.param_names.push("receiver".to_owned());
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
                self.param_names.push("receiver".to_owned());
                self.pack_record("receiver", id, true)?;
            }
            Receiver::DirectRecord { c_type, receive } => match receive {
                Receive::ByRef => {
                    self.params.push(format!("const {c_type} *receiver"));
                    self.param_names.push("receiver".to_owned());
                    self.args.push("*receiver".into())
                }
                Receive::ByValue => {
                    self.params.push(format!("{c_type} receiver"));
                    self.param_names.push("receiver".to_owned());
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
        self.param_names.push(name.clone());
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
            ParamPlan::DirectVec { element, receive } => {
                let use_ = if *receive == Receive::ByMutRef {
                    surface::ValueUse::ParamMut
                } else {
                    surface::ValueUse::Param
                };
                let c = surface::direct_vector_type(element, self.context, use_)?;
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
            TypeRef::String | TypeRef::Bytes => format!("4 + {name}.ptr[{i}].len"),
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
            TypeRef::String | TypeRef::Bytes => self.setup.push(format!("    for (uintptr_t {i}=0; {i}<{name}.len; ++{i}) {{ boltffi_c_{p}_write_u32(&{writer},(uint32_t){name}.ptr[{i}].len); boltffi_c_{p}_write(&{writer},{name}.ptr[{i}].ptr,{name}.ptr[{i}].len); }}")),
            TypeRef::Primitive(v) => {
                let write = surface::wire_write_stmt(*v, &format!("{name}.ptr[{i}]"), &p);
                self.setup
                    .push(format!("    for (uintptr_t {i}=0; {i}<{name}.len; ++{i}) {write}"));
            }
            TypeRef::Record(id) if matches!(self.context.record(*id),Some(boltffi_binding::RecordDecl::Encoded(_)))=>self.setup.push(format!("    for (uintptr_t {i}=0; {i}<{name}.len; ++{i}) {}(&{writer},&{name}.ptr[{i}]);",surface::record_helper_name(*id,self.context,"encode")?)),
            _=>self.setup.push(format!("    boltffi_c_{p}_write(&{writer},{name}.ptr,{name}.len*({element_size}));"))
        }
        self.args.push(format!("{buf}.ptr"));
        self.args.push(format!("{buf}.len"));
        self.cleanup.push(format!("    boltffi_free_buf({buf});"));
        Ok(())
    }
    fn pack_scalar_option(&mut self, name: &str, primitive: Primitive) {
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
        let value_write = match primitive {
            Primitive::U32 => format!("boltffi_c_{p}_write_u32(&{writer},{name}.value);"),
            Primitive::F32 => format!("boltffi_c_{p}_write_f32(&{writer},{name}.value);"),
            _ => return,
        };
        self.setup
            .push(format!("    if ({name}.has_value) {value_write}"));
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
                let result = self.reserved_local("result");
                b.push_str(&format!(
                    "    {semantic} {result} = ({semantic}){}({args});\n",
                    self.abi.name()
                ));
                for l in &self.cleanup {
                    b.push_str(l);
                    b.push('\n')
                }
                b.push_str(&format!("    return {result};\n"))
            }
            ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Class(_),
                ..
            } => {
                let result = self.reserved_local("result");
                b.push_str(&format!(
                    "    {semantic} {result}; {result}._boltffi_handle = {}({args});\n",
                    self.abi.name()
                ));
                for l in &self.cleanup {
                    b.push_str(l);
                    b.push('\n')
                }
                b.push_str(&format!("    return {result};\n"))
            }
            ReturnPlan::EncodedViaReturnSlot { ty, .. } => {
                let raw = self.reserved_local("raw");
                let result = self.reserved_local("result");
                b.push_str(&format!(
                    "    FfiBuf_u8 {raw} = {}({args});\n",
                    self.abi.name()
                ));
                for l in &self.cleanup {
                    b.push_str(l);
                    b.push('\n')
                }
                let context = self.context;
                b.push_str(&decode_owned(
                    ty,
                    &raw,
                    &result,
                    context,
                    true,
                    &mut |stem| self.reserved_local(stem),
                )?);
                b.push_str(&format!("    return {result};\n"))
            }
            ReturnPlan::ScalarOptionViaReturnSlot {
                primitive: scalar @ (Primitive::U32 | Primitive::F32),
                ..
            } => {
                let p = package_member(self.context);
                let raw = self.reserved_local("raw");
                let result = self.reserved_local("result");
                let reader = self.reserved_local("reader");
                let tag = self.reserved_local("tag");
                b.push_str(&format!(
                    "    FfiBuf_u8 {raw} = {}({args});\n",
                    self.abi.name()
                ));
                for l in &self.cleanup {
                    b.push_str(l);
                    b.push('\n');
                }
                let value_read = match scalar {
                    Primitive::U32 => format!("boltffi_c_{p}_read_u32(&{reader})"),
                    _ => format!("boltffi_c_{p}_read_f32(&{reader})"),
                };
                b.push_str(&format!("    {semantic} {result}; memset(&{result},0,sizeof({result})); BoltFFICWireReader {reader}={{{raw}.ptr,{raw}.len,0,true}}; uint8_t {tag}=boltffi_c_{p}_read_u8(&{reader}); if ({tag}==1) {{ {result}.has_value=true; {result}.value={value_read}; }} boltffi_free_buf({raw}); return {result};\n"));
            }
            ReturnPlan::DirectVecViaReturnSlot { element } => {
                let raw = self.reserved_local("raw");
                let result = self.reserved_local("result");
                b.push_str(&format!(
                    "    FfiBuf_u8 {raw} = {}({args});\n",
                    self.abi.name()
                ));
                for l in &self.cleanup {
                    b.push_str(l);
                    b.push('\n')
                }
                let elem = surface::direct_vector_element_type(element, self.context)?;
                b.push_str(&format!("    {semantic} {result}; {result}.len={raw}.len/sizeof({elem}); {result}.ptr=({elem} *)malloc({raw}.len); if ({raw}.len) memcpy({result}.ptr,{raw}.ptr,{raw}.len); boltffi_free_buf({raw});\n    return {result};\n"));
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
        let raw_decl = raw_success_decl(plan, self.context)?;
        let success = self.reserved_local("success");
        let error = self.reserved_local("error");
        let result = self.reserved_local("result");
        let error_reader = self.reserved_local("error_reader");
        let mut call_args = args.to_owned();
        if !call_args.is_empty() {
            call_args.push_str(", ")
        }
        call_args.push_str(&format!("&{success}"));
        let mut b = format!(
            "typedef struct {{ bool ok; union {{ {ok} value; {package_type_prefix}String error; }} data; }} {result_ty};\nstatic inline {result_ty} {name}({params}) {{\n"
        );
        for l in &self.setup {
            b.push_str(l);
            b.push('\n')
        }
        b.push_str(&format!(
            "    {raw_decl} {success};\n    FfiBuf_u8 {error} = {}({call_args});\n",
            self.abi.name()
        ));
        for l in &self.cleanup {
            b.push_str(l);
            b.push('\n')
        }
        b.push_str(&format!("    {result_ty} {result}; memset(&{result},0,sizeof({result}));\n    if ({error}.len == 0) {{ {result}.ok=true;\n"));
        let context = self.context;
        b.push_str(&decode_success(
            plan,
            &success,
            &format!("{result}.data.value"),
            context,
            &mut |stem| self.reserved_local(stem),
        )?);
        let package = package_member(self.context);
        b.push_str(&format!("        return {result};\n    }}\n    {result}.ok=false; BoltFFICWireReader {error_reader}={{{error}.ptr,{error}.len,0,true}}; boltffi_c_{package}_copy_string(&{error_reader},&{result}.data.error); boltffi_free_buf({error}); return {result};\n}}\n"));
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
fn raw_success_decl(
    plan: &ReturnPlan<Native, boltffi_binding::OutOfRust>,
    context: &RenderContext<Native>,
) -> Result<String> {
    match plan {
        ReturnPlan::DirectViaOutPointer { ty } => surface::direct_value_type(ty, context),
        ReturnPlan::EncodedViaOutPointer { .. } => Ok("FfiBuf_u8".to_owned()),
        ReturnPlan::HandleViaOutPointer { .. } => Ok("uint64_t".to_owned()),
        _ => unsupported("fallible success transport"),
    }
}
fn decode_success(
    plan: &ReturnPlan<Native, boltffi_binding::OutOfRust>,
    raw: &str,
    out: &str,
    context: &RenderContext<Native>,
    namer: &mut dyn FnMut(&str) -> String,
) -> Result<String> {
    match plan {
        ReturnPlan::DirectViaOutPointer { .. } => Ok(format!(
            "        {out}=({}){raw};\n",
            return_type(plan, context)?
        )),
        ReturnPlan::HandleViaOutPointer { .. } => {
            Ok(format!("        {out}._boltffi_handle={raw};\n"))
        }
        ReturnPlan::EncodedViaOutPointer { ty, .. } => {
            decode_owned(ty, raw, out, context, false, namer)
        }
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
            "{}(&value->data.{field});",
            surface::record_helper_name(*id, context, "free")?
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
        Primitive::USize => "usize",
        Primitive::F32 => "f32",
        Primitive::F64 => "f64",
        _ => "unsupported",
    }
}

pub(super) fn decode_owned(
    ty: &TypeRef,
    raw: &str,
    out: &str,
    context: &RenderContext<Native>,
    declare: bool,
    namer: &mut dyn FnMut(&str) -> String,
) -> Result<String> {
    let p = package_member(context);
    let c = surface::value_type(ty, context, surface::ValueUse::Return)?;
    let init = if declare {
        format!("{c} {out}; ")
    } else {
        String::new()
    };
    let reader = namer("reader");
    match ty {
        TypeRef::String => Ok(format!(
            "    {init}memset(&{out},0,sizeof({out})); BoltFFICWireReader {reader}={{{raw}.ptr,{raw}.len,0,true}}; boltffi_c_{p}_copy_string(&{reader},&{out}); boltffi_free_buf({raw});\n"
        )),
        TypeRef::Bytes => Ok(format!(
            "    {init}memset(&{out},0,sizeof({out})); BoltFFICWireReader {reader}={{{raw}.ptr,{raw}.len,0,true}}; boltffi_c_{p}_copy_bytes(&{reader},&{out}); boltffi_free_buf({raw});\n"
        )),
        TypeRef::Record(id) => Ok(format!(
            "    {init}BoltFFICWireReader {reader}={{{raw}.ptr,{raw}.len,0,true}}; {}(&{reader},&{out}); boltffi_free_buf({raw});\n",
            surface::record_helper_name(*id, context, "decode")?
        )),
        TypeRef::Sequence(element) => {
            let elem = surface::value_type(element, context, surface::ValueUse::Field)?;
            let count = namer("count");
            let index = namer("i");
            let decode = match element.as_ref() {
                TypeRef::String => format!(
                    "if (!boltffi_c_{p}_copy_string(&{reader},&{out}.ptr[{index}])) {{ {p}_string_sequence_free(&{out}); break; }}"
                ),
                TypeRef::Primitive(v) => format!(
                    "{out}.ptr[{index}] = {};",
                    surface::wire_read_expr(*v, &p).replace("boltffi_reader", &reader)
                ),
                TypeRef::Record(id)
                    if matches!(
                        context.record(*id),
                        Some(boltffi_binding::RecordDecl::Direct(_))
                    ) =>
                {
                    format!("boltffi_c_{p}_read(&{reader},&{out}.ptr[{index}],sizeof({elem}));")
                }
                TypeRef::Record(id) => format!(
                    "if (!{}(&{reader},&{out}.ptr[{index}])) {{ {p}_{}_sequence_free(&{out}); break; }}",
                    surface::record_helper_name(*id, context, "decode")?,
                    Name::new(context.record(*id).expect("record").name()).member()
                ),
                _ => return unsupported("owned sequence return element"),
            };
            Ok(format!(
                "    {init}memset(&{out},0,sizeof({out})); BoltFFICWireReader {reader}={{{raw}.ptr,{raw}.len,0,true}}; uint32_t {count}=boltffi_c_{p}_read_u32(&{reader}); {out}.ptr=({elem} *)calloc({count},sizeof({elem})); {out}.len={count}; if ({count} && {out}.ptr == NULL) {reader}.ok=false; for (uintptr_t {index}=0; {reader}.ok && {index}<{count}; ++{index}) {{ {decode} }} boltffi_free_buf({raw});\n"
            ))
        }
        TypeRef::Optional(inner) if **inner == TypeRef::String => {
            let tag = namer("tag");
            Ok(format!(
                "    {init}memset(&{out},0,sizeof({out})); BoltFFICWireReader {reader}={{{raw}.ptr,{raw}.len,0,true}}; uint8_t {tag}=boltffi_c_{p}_read_u8(&{reader}); if ({tag} == 1) {{ if (!boltffi_c_{p}_copy_string(&{reader},&{out}.value)) {{ {p}_string_free(&{out}.value); }} else {{ {out}.has_value=true; }} }} else if ({tag} != 0) {{ {reader}.ok=false; }} boltffi_free_buf({raw});\n"
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
