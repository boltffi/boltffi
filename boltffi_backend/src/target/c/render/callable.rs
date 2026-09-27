//! Semantic sync callable rendering for the package-prefixed C facade.

use super::{prefix::PackagePrefix, surface};
use crate::{
    bridge::c,
    core::{Emitted, Error, RenderContext, Result},
    target::c::name_style::Name,
};
use boltffi_binding::{
    ErrorChannel, ErrorPlacement, ExportedCallable, HandlePresence, HandleTarget, Native,
    ParamPlan, Primitive, Receive, ReturnPlan, TypeRef,
};

pub enum Receiver<'a> {
    None,
    Class {
        c_type: &'a str,
        receive: Receive,
    },
    Encoded {
        ty: TypeRef,
        codec: &'a boltffi_binding::ReadPlan,
        receive: Receive,
    },
    DirectValue {
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
    let parameters = callable
        .params()
        .iter()
        .map(|parameter| {
            let name = c::Identifier::escape(Name::new(parameter.name()).member())?.to_string();
            let plan = parameter
                .payload()
                .as_value()
                .ok_or(Error::UnsupportedTarget {
                    target: "c",
                    shape: "closure callable parameter",
                })?;
            Ok((name, plan))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut state = State::new(
        abi,
        context,
        parameters.iter().map(|(name, _)| name.clone()).collect(),
    );
    state.receiver(receiver)?;
    parameters
        .into_iter()
        .try_for_each(|(name, plan)| state.param(name, plan))?;
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
            ty,
            codec,
            ..
        } => state.render_result(
            callable.returns().plan(),
            wrapper_name,
            result_stem,
            ty,
            codec,
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
    fn new(
        abi: &'a c::Function,
        context: &'a RenderContext<'a, Native>,
        param_names: Vec<String>,
    ) -> Self {
        Self {
            abi,
            context,
            params: vec![],
            param_names,
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
                if receive == Receive::ByValue {
                    self.cleanup
                        .push("receiver->_boltffi_handle = 0;".to_owned());
                }
            }
            Receiver::Encoded { ty, codec, receive } => {
                if receive == Receive::ByMutRef {
                    return self.mutable_encoded("receiver", &ty, &codec.write_self_value());
                }
                let c_type = surface::value_type(&ty, self.context, surface::ValueUse::Param)?;
                self.params.push(format!("const {c_type} *receiver"));
                self.param_names.push("receiver".to_owned());
                self.pack_encoded(
                    "(*receiver)",
                    &codec.write_self_value(),
                    surface::ValueUse::Param,
                )?;
            }
            Receiver::DirectValue { c_type, receive } => match receive {
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
                Receive::ByMutRef => {
                    self.params.push(format!("{c_type} *receiver"));
                    self.param_names.push("receiver".to_owned());
                    self.args.push("*receiver".to_owned());
                    self.args.push("receiver".to_owned());
                }
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
                        self.params.push(format!("{c} *{name}"));
                        self.args.push(name);
                    }
                    _ => {
                        self.params.push(format!("{c} {name}"));
                        self.args.push(name)
                    }
                }
            }
            ParamPlan::Encoded {
                ty, codec, receive, ..
            } => {
                if *receive == Receive::ByMutRef {
                    return self.mutable_encoded(&name, ty, codec);
                }
                let c_type = surface::value_type(ty, self.context, surface::ValueUse::Param)?;
                let borrowed_record =
                    *receive == Receive::ByRef && matches!(ty, TypeRef::Record(_));
                let value = if borrowed_record {
                    self.params.push(format!("const {c_type} *{name}"));
                    format!("(*{name})")
                } else {
                    self.params.push(format!("{c_type} {name}"));
                    name
                };
                self.pack_encoded(&value, codec, surface::ValueUse::Param)?;
            }
            ParamPlan::ScalarOption { primitive } => {
                let c = surface::value_type(
                    &TypeRef::Optional(Box::new(TypeRef::Primitive(*primitive))),
                    self.context,
                    surface::ValueUse::Param,
                )?;
                self.params.push(format!("{c} {name}"));
                self.pack_scalar_option(&name, *primitive);
            }
            ParamPlan::DirectVec { element, receive } => {
                let mutable = *receive == Receive::ByMutRef;
                let use_ = if mutable {
                    surface::ValueUse::ParamMut
                } else {
                    surface::ValueUse::Param
                };
                let c = surface::direct_vector_type(element, self.context, use_)?;
                self.params.push(format!("{c} {name}"));
                match element {
                    // Packed direct-record vectors cross the C ABI as opaque bytes with a
                    // byte count, so the typed slice pointer needs a byte-pointer cast: the
                    // bridge parameter is a const/non-const uint8_t pointer, never the
                    // element pointer this facade slice type carries.
                    boltffi_binding::DirectVectorElementType::Record(_) => {
                        self.args.push(format!(
                            "({}uint8_t *){name}.ptr",
                            if mutable { "" } else { "const " }
                        ));
                        self.args.push(format!(
                            "{name}.len * sizeof({})",
                            surface::direct_vector_element_type(element, self.context)?
                        ));
                    }
                    // Typed primitive vectors carry an element pointer and an element count,
                    // matching the typed bridge parameter.
                    _ => {
                        self.args.push(format!("{name}.ptr"));
                        self.args.push(format!("{name}.len"));
                    }
                }
            }
            ParamPlan::Handle {
                target: HandleTarget::Callback(id),
                receive,
                presence,
                ..
            } => {
                let c_type = super::callback::handle_type_name(*id, self.context)?;
                let qualifier = if *receive == Receive::ByValue {
                    ""
                } else {
                    "const "
                };
                self.params.push(format!("{qualifier}{c_type} *{name}"));
                self.args.push(if *presence == HandlePresence::Nullable {
                    format!("{name} == NULL ? (BoltFFICallbackHandle){{0}} : {name}->raw")
                } else {
                    format!("{name}->raw")
                });
                if *receive == Receive::ByValue {
                    self.cleanup.push(format!(
                        "if ({name} != NULL) memset(&{name}->raw, 0, sizeof({name}->raw));"
                    ));
                }
            }
            ParamPlan::Handle {
                target: HandleTarget::Class(id),
                receive,
                presence,
                ..
            } => {
                let class = self.context.class(*id).ok_or(Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "missing class parameter",
                })?;
                let c = PackagePrefix::from_context(self.context).type_name(class.name());
                let nullable = *presence == HandlePresence::Nullable;
                if nullable {
                    self.args
                        .push(format!("{name} == NULL ? 0 : {name}->_boltffi_handle"));
                } else {
                    self.args.push(format!("{name}->_boltffi_handle"));
                }
                if *receive == Receive::ByValue {
                    self.params.push(format!("{c} *{name}"));
                    self.cleanup.push(if nullable {
                        format!("    if ({name} != NULL) {name}->_boltffi_handle = 0;")
                    } else {
                        format!("    {name}->_boltffi_handle = 0;")
                    });
                } else {
                    self.params.push(format!("const {c} *{name}"));
                }
            }
            _ => return unsupported("callable parameter plan"),
        }
        Ok(())
    }
    fn mutable_encoded(
        &mut self,
        name: &str,
        ty: &TypeRef,
        codec: &boltffi_binding::WritePlan,
    ) -> Result<()> {
        if self
            .abi
            .params()
            .get(self.args.len())
            .is_some_and(|parameter| matches!(parameter.ty(), c::Type::MutPointer(_)))
        {
            let slice = surface::direct_vector_type(
                &boltffi_binding::DirectVectorElementType::Primitive(
                    boltffi_binding::DirectVectorPrimitive::writable_bytes(),
                ),
                self.context,
                surface::ValueUse::ParamMut,
            )?;
            self.params.push(format!("{slice} {name}"));
            self.args.push(format!("{name}.ptr"));
            self.args.push(format!("{name}.len"));
            return Ok(());
        }
        let owned_type = surface::value_type(ty, self.context, surface::ValueUse::Return)?;
        self.params.push(format!("{owned_type} *{name}"));
        self.pack_encoded(&format!("(*{name})"), codec, surface::ValueUse::Return)?;
        let raw = self.local("writeback");
        let updated = self.local("updated");
        let reader_name = self.local("reader");
        self.setup.push(format!("FfiBuf_u8 {raw} = {{0}};"));
        self.args.push(format!("&{raw}"));
        let mut reader =
            crate::target::c::codec::read::Reader::new(self.context, format!("&{reader_name}"));
        let decode = reader.decode(&codec.read_plan(), &updated)?;
        let free_original = surface::free_owned(ty, &format!("(*{name})"), self.context)?;
        let free_updated = surface::free_owned(ty, &updated, self.context)?;
        self.cleanup.push(format!(
            "{owned_type} {updated} = {{0}};\n\
            BoltFFICWireReader {reader_name} = {{{raw}.ptr, {raw}.len, 0, true}};\n\
            {{\n{decode}\n}}\n\
            if ({reader_name}.ok && {reader_name}.offset == {reader_name}.len) {{\n\
                {free_original}\n*{name} = {updated};\n\
            }} else {{\n{free_updated}\n}}\nboltffi_free_buf({raw});"
        ));
        Ok(())
    }
    fn pack_encoded(
        &mut self,
        value: &str,
        codec: &boltffi_binding::WritePlan,
        usage: surface::ValueUse,
    ) -> Result<()> {
        if matches!(
            codec.root().owned_wire_encoding(),
            boltffi_binding::OwnedWireEncoding::Utf8String
                | boltffi_binding::OwnedWireEncoding::RawBytes
        ) {
            self.args.push(format!("(const uint8_t *)({value}).ptr"));
            self.args.push(format!("({value}).len"));
            return Ok(());
        }
        let size = self.local("size");
        let buffer = self.local("buffer");
        let writer_name = self.local("writer");
        let mut sizer = crate::target::c::codec::size::Sizer::new(self.context, value, usage);
        let size_statements = sizer.measure(codec, &size)?;
        let mut writer = crate::target::c::codec::write::Writer::new(
            self.context,
            format!("&{writer_name}"),
            value,
            usage,
        );
        let encode = writer.encode(codec)?;
        self.setup.push(format!(
            "uintptr_t {size} = 0;\n{{\n{size_statements}\n}}\n\
             FfiBuf_u8 {buffer} = boltffi_buf_with_len({size});\n\
             BoltFFICWireWriter {writer_name} = {{{buffer}.ptr, {buffer}.len, 0, true}};\n\
             {{\n{encode}\n}}"
        ));
        self.args.push(format!("{buffer}.ptr"));
        self.args.push(format!("{buffer}.len"));
        self.cleanup.push(format!("boltffi_free_buf({buffer});"));
        Ok(())
    }
    fn pack_scalar_option(&mut self, name: &str, primitive: Primitive) {
        let buf = self.local("buf");
        let writer = self.local("writer");
        let p = package_member(self.context);
        let present_size = 1 + primitive.wire_size().get();
        self.setup
            .push(format!("    uint8_t {buf}[{present_size}];"));
        self.setup.push(format!(
            "    BoltFFICWireWriter {writer}={{{buf},sizeof({buf}),0,true}};"
        ));
        self.setup.push(format!(
            "    boltffi_c_{p}_write_u8(&{writer},{name}.has_value ? 1 : 0);"
        ));
        let value_write = surface::wire_write_stmt(primitive, &format!("{name}.value"), &p)
            .replace("boltffi_writer", &format!("&{writer}"));
        self.setup
            .push(format!("    if ({name}.has_value) {value_write}"));
        self.args.push(buf);
        self.args.push(format!("{writer}.offset"));
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
                    "    {semantic} {result} = {}({args});\n",
                    self.abi.name()
                ));
                for l in &self.cleanup {
                    b.push_str(l);
                    b.push('\n')
                }
                b.push_str(&format!("    return {result};\n"))
            }
            ReturnPlan::HandleViaReturnSlot { carrier, .. } => {
                let result = self.reserved_local("result");
                let field = match carrier {
                    boltffi_binding::native::HandleCarrier::U64
                    | boltffi_binding::native::HandleCarrier::USize => "_boltffi_handle",
                    boltffi_binding::native::HandleCarrier::CallbackHandle => "raw",
                    _ => return unsupported("returned handle carrier"),
                };
                b.push_str(&format!(
                    "    {semantic} {result}; {result}.{field} = {}({args});\n",
                    self.abi.name()
                ));
                for l in &self.cleanup {
                    b.push_str(l);
                    b.push('\n')
                }
                b.push_str(&format!("    return {result};\n"))
            }
            ReturnPlan::EncodedViaReturnSlot { ty, codec, .. } => {
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
                b.push_str(&self.decode_owned(ty, codec, &raw, &result, true)?);
                b.push_str(&format!("    return {result};\n"))
            }
            ReturnPlan::ScalarOptionViaReturnSlot {
                primitive: scalar,
                enum_target,
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
                let mut value_read = surface::wire_read_expr(*scalar, &p)
                    .replace("boltffi_reader", &format!("&{reader}"));
                if let Some(target) = enum_target {
                    let enum_type =
                        surface::value_type(target, self.context, surface::ValueUse::Return)?;
                    value_read = format!("({enum_type}){value_read}");
                }
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
                b.push_str(&format!(
                    "    {semantic} {result} = {{0}};\n\
                    if ({raw}.len != 0 && {raw}.len % sizeof({elem}) == 0) {{\n\
                        {result}.ptr = ({elem} *)malloc({raw}.len);\n\
                        if ({result}.ptr != NULL) {{\n\
                            memcpy({result}.ptr, {raw}.ptr, {raw}.len);\n\
                            {result}.len = {raw}.len / sizeof({elem});\n\
                        }}\n}}\nboltffi_free_buf({raw});\nreturn {result};\n"
                ));
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
        error_type: &TypeRef,
        error_codec: &boltffi_binding::ReadPlan,
    ) -> Result<String> {
        let params = if self.params.is_empty() {
            "void".to_owned()
        } else {
            self.params.join(", ")
        };
        let args = self.args.join(", ");
        let ok = return_type(plan, self.context)?;
        let error_c_type =
            surface::value_type(error_type, self.context, surface::ValueUse::Return)?;
        let result_ty = format!("{stem}Result");
        let raw_decl = raw_success_decl(plan, self.context)?;
        let success = self.reserved_local("success");
        let error = self.reserved_local("error");
        let result = self.reserved_local("result");
        let mut call_args = args.to_owned();
        let success_field = if raw_decl.is_some() {
            format!("{ok} value;")
        } else {
            String::new()
        };
        let success_declaration = match raw_decl {
            Some(raw_type) => {
                if !call_args.is_empty() {
                    call_args.push_str(", ");
                }
                call_args.push_str(&format!("&{success}"));
                format!("{raw_type} {success};")
            }
            None => String::new(),
        };
        let mut b = format!(
            "typedef struct {{ bool ok; union {{ {success_field} {error_c_type} error; }} data; }} {result_ty};\nstatic inline {result_ty} {name}({params}) {{\n"
        );
        for l in &self.setup {
            b.push_str(l);
            b.push('\n')
        }
        b.push_str(&format!(
            "    {success_declaration}\n    FfiBuf_u8 {error} = {}({call_args});\n",
            self.abi.name()
        ));
        for l in &self.cleanup {
            b.push_str(l);
            b.push('\n')
        }
        b.push_str(&format!("    {result_ty} {result}; memset(&{result},0,sizeof({result}));\n    if ({error}.len == 0) {{ {result}.ok=true;\n"));
        b.push_str(&self.decode_success(plan, &success, &format!("{result}.data.value"))?);
        b.push_str(&format!(
            "        return {result};\n    }}\n    {result}.ok=false;\n"
        ));
        b.push_str(&self.decode_owned(
            error_type,
            error_codec,
            &error,
            &format!("{result}.data.error"),
            false,
        )?);
        b.push_str(&format!("    return {result};\n}}\n"));
        let free_success = free_success_stmt(plan, "value", self.context)?;
        let free_error = surface::free_owned(error_type, "value->data.error", self.context)?;
        b.push_str(&format!("static inline void {name}_result_free({result_ty} *value) {{ if (value == NULL) return; if (value->ok) {{ {free_success} }} else {{ {free_error} }} memset(value,0,sizeof(*value)); }}\n"));
        Ok(b)
    }
    fn decode_success(
        &mut self,
        plan: &ReturnPlan<Native, boltffi_binding::OutOfRust>,
        raw: &str,
        out: &str,
    ) -> Result<String> {
        match plan {
            ReturnPlan::Void => Ok(String::new()),
            ReturnPlan::DirectViaOutPointer { .. } => Ok(format!("        {out}={raw};\n")),
            ReturnPlan::HandleViaOutPointer { carrier, .. } => {
                let field = match carrier {
                    boltffi_binding::native::HandleCarrier::U64
                    | boltffi_binding::native::HandleCarrier::USize => "_boltffi_handle",
                    boltffi_binding::native::HandleCarrier::CallbackHandle => "raw",
                    _ => return unsupported("returned handle carrier"),
                };
                Ok(format!("        {out}.{field}={raw};\n"))
            }
            ReturnPlan::EncodedViaOutPointer { ty, codec, .. } => {
                self.decode_owned(ty, codec, raw, out, false)
            }
            _ => unsupported("fallible success decode"),
        }
    }
    fn decode_owned(
        &mut self,
        ty: &TypeRef,
        codec: &boltffi_binding::ReadPlan,
        raw: &str,
        destination: &str,
        declare: bool,
    ) -> Result<String> {
        let value_type = surface::value_type(ty, self.context, surface::ValueUse::Return)?;
        let declaration = if declare {
            format!("{value_type} {destination};")
        } else {
            String::new()
        };
        let reader_name = self.local("reader");
        let mut reader =
            crate::target::c::codec::read::Reader::new(self.context, format!("&{reader_name}"));
        let decode = reader.decode(codec, destination)?;
        let free = surface::free_owned(ty, destination, self.context)?;
        Ok(format!(
            "{declaration}\nmemset(&{destination}, 0, sizeof({destination}));\n\
         BoltFFICWireReader {reader_name} = {{{raw}.ptr, {raw}.len, 0, true}};\n\
         {{\n{decode}\n}}\n\
         if (!{reader_name}.ok || {reader_name}.offset != {reader_name}.len) {{\n\
         {free}\nmemset(&{destination}, 0, sizeof({destination}));\n}}\n\
         boltffi_free_buf({raw});\n"
        ))
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
            Ok(PackagePrefix::from_context(context).type_name(c.name()))
        }
        ReturnPlan::HandleViaReturnSlot {
            target: HandleTarget::Callback(id),
            ..
        }
        | ReturnPlan::HandleViaOutPointer {
            target: HandleTarget::Callback(id),
            ..
        } => super::callback::handle_type_name(*id, context),
        ReturnPlan::ScalarOptionViaReturnSlot {
            primitive,
            enum_target,
        } => surface::value_type(
            &TypeRef::Optional(Box::new(
                enum_target
                    .clone()
                    .unwrap_or(TypeRef::Primitive(*primitive)),
            )),
            context,
            surface::ValueUse::Return,
        ),
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            surface::direct_vector_type(element, context, surface::ValueUse::Return)
        }
        _ => unsupported("semantic return type"),
    }
}
fn raw_success_decl(
    plan: &ReturnPlan<Native, boltffi_binding::OutOfRust>,
    context: &RenderContext<Native>,
) -> Result<Option<String>> {
    match plan {
        ReturnPlan::Void => Ok(None),
        ReturnPlan::DirectViaOutPointer { ty } => surface::direct_value_type(ty, context).map(Some),
        ReturnPlan::EncodedViaOutPointer { .. } => Ok(Some("FfiBuf_u8".to_owned())),
        ReturnPlan::HandleViaOutPointer {
            target, carrier, ..
        } => c::TypeFragment::anonymous(&c::Type::handle_target(target, *carrier)?)
            .map(|ty| Some(ty.to_string())),
        _ => unsupported("fallible success transport"),
    }
}

fn free_success_stmt(
    plan: &ReturnPlan<Native, boltffi_binding::OutOfRust>,
    field: &str,
    context: &RenderContext<Native>,
) -> Result<String> {
    match plan {
        ReturnPlan::EncodedViaOutPointer { ty, .. } => {
            surface::free_owned(ty, &format!("value->data.{field}"), context)
        }
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
        ReturnPlan::HandleViaOutPointer {
            target: HandleTarget::Callback(id),
            ..
        } => {
            let callback = context.callback(*id).ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing returned callback",
            })?;
            let free = PackagePrefix::from_context(context)
                .member(&format!("{}_free", Name::new(callback.name()).member()));
            Ok(format!("{free}(&value->data.{field});"))
        }
        ReturnPlan::Void | ReturnPlan::DirectViaOutPointer { .. } => Ok(String::new()),
        _ => unsupported("result success free"),
    }
}
fn package_member(context: &RenderContext<Native>) -> String {
    Name::new(context.bindings().package().name()).member()
}
fn unsupported<T>(shape: &'static str) -> Result<T> {
    Err(Error::UnsupportedTarget { target: "c", shape })
}
