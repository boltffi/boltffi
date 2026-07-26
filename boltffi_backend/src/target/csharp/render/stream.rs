use boltffi_binding::{
    ByteSize, CanonicalName, DirectValueType, Native, ReadPlan, StreamDecl, StreamItemPlanRender,
    StreamMode, TypeRef, native,
};

use crate::{
    bridge::c::CBridgeContract,
    core::{AuxChunk, Emitted, Error, HelperId, RenderContext, Result},
};

use super::super::{
    codec::{ReadExpression, Reader},
    name_style::Name,
    syntax::{Identifier, Literal, TypeFragment},
    type_name,
};
use super::{Documentation, FreeBufferTemplate, direct_type};
use askama::Template;

pub(in crate::target::csharp) struct Stream {
    documentation: Documentation,
    name: Identifier,
    qualified: Identifier,
    owner: Option<TypeFragment>,
    item: StreamItem,
    mode: StreamMode,
    runtime: Identifier,
    subscription: Identifier,
    cancellable: Identifier,
    subscribe: String,
    pop_batch: String,
    wait: String,
    unsubscribe: String,
    free: String,
    free_buffer: Literal,
}

struct StreamItem {
    ty: TypeFragment,
    read_batch: String,
    encoded: bool,
    bool_item: bool,
}

struct StreamItemRenderer<'a> {
    context: &'a RenderContext<'a, Native>,
}

impl Stream {
    pub(in crate::target::csharp) fn from_declaration(
        declaration: &StreamDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let protocol =
            bridge
                .source_stream(declaration.id())
                .ok_or(Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "stream protocol is missing from the C bridge",
                })?;
        let name = Name::new(declaration.name()).pascal()?;
        let owner = declaration
            .owner()
            .map(|owner| type_name::class(owner, context))
            .transpose()?;
        let qualified = match &owner {
            Some(owner) => Identifier::parse(format!("{owner}{name}"))?,
            None => name.clone(),
        };
        let mut item = declaration
            .item()
            .render_with(&mut StreamItemRenderer { context })?;
        item.read_batch = item.read_batch.replace(
            "NativeStreamPopBatch",
            &format!("Native{qualified}PopBatch"),
        );
        Ok(Self {
            documentation: Documentation::summary(declaration.meta().doc(), "        "),
            runtime: Identifier::parse(format!("{qualified}StreamRuntime"))?,
            subscription: Identifier::parse(format!("{qualified}Subscription"))?,
            cancellable: Identifier::parse(format!("{qualified}Cancellable"))?,
            name,
            qualified,
            owner,
            item,
            mode: declaration.mode(),
            subscribe: protocol.subscribe().name().to_owned(),
            pop_batch: protocol.pop_batch().name().to_owned(),
            wait: protocol.wait().name().to_owned(),
            unsubscribe: protocol.unsubscribe().name().to_owned(),
            free: protocol.free().name().to_owned(),
            free_buffer: Literal::string(bridge.support().buffer_free()?.name()),
        })
    }

    pub(in crate::target::csharp) fn render(&self) -> Result<Emitted> {
        let primary = self.primary()?;
        let mut emitted = Emitted::primary(primary)
            .with_aux(AuxChunk::ForwardDecl(self.runtime_source()?.into()))
            .with_aux(AuxChunk::Helper {
                id: HelperId::new(CanonicalName::single(format!(
                    "csharp_stream_{}",
                    self.qualified
                ))),
                text: self.native_source().into(),
            });
        if self.item.encoded {
            emitted = emitted
                .with_aux(AuxChunk::ForwardDecl(super::WireTemplate.render()?.into()))
                .with_aux(AuxChunk::Helper {
                    id: HelperId::new(CanonicalName::single("csharp_free_buffer")),
                    text: FreeBufferTemplate {
                        entry_point: &self.free_buffer,
                    }
                    .render()?
                    .into(),
                });
        }
        Ok(emitted)
    }

    fn receiver_parameter(&self) -> String {
        self.owner
            .as_ref()
            .map(|owner| format!("this {owner} self"))
            .unwrap_or_default()
    }

    fn receiver_argument(&self) -> &'static str {
        if self.owner.is_some() {
            "self.Handle"
        } else {
            ""
        }
    }

    fn primary(&self) -> Result<String> {
        let receiver = self.receiver_parameter();
        let separator = if receiver.is_empty() { "" } else { ", " };
        let item = &self.item.ty;
        let source = match self.mode {
            StreamMode::Async => format!(
                "        public static global::System.Collections.Generic.IAsyncEnumerable<{item}> {}({receiver}{separator}global::System.Threading.CancellationToken cancellationToken = default)\n            => {}.ReadAll({}, cancellationToken);\n",
                self.name,
                self.runtime,
                self.receiver_argument(),
            ),
            StreamMode::Batch => format!(
                "        public static {} {}({receiver})\n            => {}.Create({});\n",
                self.subscription,
                self.name,
                self.runtime,
                self.receiver_argument(),
            ),
            StreamMode::Callback => format!(
                "        public static {} {}({receiver}{separator}global::System.Action<{item}> callback)\n            => {}.Subscribe({}, callback);\n",
                self.cancellable,
                self.name,
                self.runtime,
                self.receiver_argument(),
            ),
            _ => return super::super::unsupported("unknown stream mode"),
        };
        Ok(format!("{}{source}", self.documentation))
    }

    fn subscribe_call(&self) -> String {
        match self.owner {
            Some(_) => format!("NativeMethods.{}(receiver)", self.subscribe_method()),
            None => format!("NativeMethods.{}()", self.subscribe_method()),
        }
    }

    fn runtime_source(&self) -> Result<String> {
        let item = &self.item.ty;
        let receiver = if self.owner.is_some() {
            "ulong receiver, "
        } else {
            ""
        };
        let receiver_only = if self.owner.is_some() {
            "ulong receiver"
        } else {
            ""
        };
        let read_all_receiver = if matches!(self.mode, StreamMode::Callback) {
            "ulong subscription, "
        } else {
            receiver
        };
        let subscription_setup = if matches!(self.mode, StreamMode::Callback) {
            String::new()
        } else {
            format!(
                "            ulong subscription = {};\n",
                self.subscribe_call()
            )
        };
        let async_runtime = format!(
            "    internal static class {}\n    {{\n        internal static async global::System.Collections.Generic.IAsyncEnumerable<{item}> ReadAll({read_all_receiver}[global::System.Runtime.CompilerServices.EnumeratorCancellation] global::System.Threading.CancellationToken cancellationToken = default)\n        {{\n{subscription_setup}            if (subscription == 0) yield break;\n            try\n            {{\n                while (true)\n                {{\n                    {item}[] items = ReadBatch(subscription, 16);\n                    foreach ({item} item in items) yield return item;\n                    if (items.Length != 0) continue;\n                    int wait = await global::System.Threading.Tasks.Task.Run(() => NativeMethods.{}(subscription, 100), cancellationToken).ConfigureAwait(false);\n                    if (wait < 0) yield break;\n                }}\n            }}\n            finally\n            {{\n                NativeMethods.{}(subscription);\n                NativeMethods.{}(subscription);\n            }}\n        }}\n\n        internal static {item}[] ReadBatch(ulong subscription, nuint maxCount)\n        {{\n{}\n        }}",
            self.runtime,
            self.wait_method(),
            self.unsubscribe_method(),
            self.free_method(),
            indent(&self.item.read_batch, 12),
        );
        let delivery = match self.mode {
            StreamMode::Async => String::new(),
            StreamMode::Batch => format!(
                "\n\n        internal static {} Create({receiver_only}) => new {}({});\n    }}\n\n    public sealed class {} : global::System.IDisposable\n    {{\n        private ulong handle;\n        internal {}(ulong handle) => this.handle = handle;\n\n        public {item}[] PopBatch(nuint maxCount = 16) => handle == 0 ? global::System.Array.Empty<{item}>() : {}.ReadBatch(handle, maxCount);\n        public int Wait(uint timeoutMilliseconds) => handle == 0 ? -1 : NativeMethods.{}(handle, timeoutMilliseconds);\n        public void Unsubscribe() {{ if (handle != 0) NativeMethods.{}(handle); }}\n        public void Dispose()\n        {{\n            ulong released = global::System.Threading.Interlocked.Exchange(ref handle, 0);\n            if (released == 0) return;\n            NativeMethods.{}(released);\n            NativeMethods.{}(released);\n            global::System.GC.SuppressFinalize(this);\n        }}\n        ~{}() {{ if (handle != 0) NativeMethods.{}(handle); }}\n",
                self.subscription,
                self.subscription,
                self.subscribe_call(),
                self.subscription,
                self.subscription,
                self.runtime,
                self.wait_method(),
                self.unsubscribe_method(),
                self.unsubscribe_method(),
                self.free_method(),
                self.subscription,
                self.free_method(),
            ),
            StreamMode::Callback => format!(
                "\n\n        internal static {} Subscribe({receiver_only}{})\n        {{\n            ulong subscription = {};\n            var cancellation = new global::System.Threading.CancellationTokenSource();\n            _ = global::System.Threading.Tasks.Task.Run(async () =>\n            {{\n                try\n                {{\n                    await foreach (var item in ReadAll(subscription, cancellation.Token)) callback(item);\n                }}\n                catch (global::System.OperationCanceledException) {{ }}\n            }});\n            return new {}(cancellation);\n        }}\n    }}\n\n    public sealed class {} : global::System.IDisposable\n    {{\n        private global::System.Threading.CancellationTokenSource? cancellation;\n        internal {}(global::System.Threading.CancellationTokenSource cancellation) => this.cancellation = cancellation;\n        public void Cancel() => global::System.Threading.Interlocked.Exchange(ref cancellation, null)?.Cancel();\n        public void Dispose() {{ Cancel(); global::System.GC.SuppressFinalize(this); }}\n        ~{}() => Cancel();\n",
                self.cancellable,
                if receiver_only.is_empty() { "global::System.Action<" } else { ", global::System.Action<" },
                self.subscribe_call(),
                self.cancellable,
                self.cancellable,
                self.cancellable,
                self.cancellable,
            )
            .replace("global::System.Action<)", &format!("global::System.Action<{item}> callback)"))
            .replace("global::System.Action<\n", &format!("global::System.Action<{item}> callback\n")),
            _ => return super::super::unsupported("unknown stream mode"),
        };
        Ok(match self.mode {
            StreamMode::Async => format!("{async_runtime}\n    }}"),
            _ => format!("{async_runtime}{delivery}\n    }}"),
        })
    }

    fn native_source(&self) -> String {
        let subscribe_parameters = if self.owner.is_some() {
            "ulong receiver"
        } else {
            ""
        };
        format!(
            "        [global::System.Runtime.InteropServices.DllImport(LibName, EntryPoint = \"{}\")]\n        internal static extern ulong {}({subscribe_parameters});\n\n{}\n\n        [global::System.Runtime.InteropServices.DllImport(LibName, EntryPoint = \"{}\")]\n        internal static extern int {}(ulong subscription, uint timeoutMilliseconds);\n\n        [global::System.Runtime.InteropServices.DllImport(LibName, EntryPoint = \"{}\")]\n        internal static extern void {}(ulong subscription);\n\n        [global::System.Runtime.InteropServices.DllImport(LibName, EntryPoint = \"{}\")]\n        internal static extern void {}(ulong subscription);",
            self.subscribe,
            self.subscribe_method(),
            self.pop_native_source(),
            self.wait,
            self.wait_method(),
            self.unsubscribe,
            self.unsubscribe_method(),
            self.free,
            self.free_method(),
        )
    }

    fn subscribe_method(&self) -> String {
        format!("Native{}Subscribe", self.qualified)
    }
    fn pop_method(&self) -> String {
        format!("Native{}PopBatch", self.qualified)
    }
    fn wait_method(&self) -> String {
        format!("Native{}Wait", self.qualified)
    }
    fn unsubscribe_method(&self) -> String {
        format!("Native{}Unsubscribe", self.qualified)
    }
    fn free_method(&self) -> String {
        format!("Native{}Free", self.qualified)
    }

    fn pop_native_source(&self) -> String {
        let method = self.pop_method();
        if self.item.encoded {
            format!(
                "        [global::System.Runtime.InteropServices.DllImport(LibName, EntryPoint = \"{}\")]\n        internal static extern FfiBuf {method}(ulong subscription, nuint maxCount);",
                self.pop_batch,
            )
        } else {
            let marshal = if self.item.bool_item {
                "[global::System.Runtime.InteropServices.MarshalAs(global::System.Runtime.InteropServices.UnmanagedType.LPArray, ArraySubType = global::System.Runtime.InteropServices.UnmanagedType.U1)] "
            } else {
                ""
            };
            format!(
                "        [global::System.Runtime.InteropServices.DllImport(LibName, EntryPoint = \"{}\")]\n        internal static extern nuint {method}(ulong subscription, {marshal}[global::System.Runtime.InteropServices.Out] {}[] output, nuint outputCapacity);",
                self.pop_batch, self.item.ty,
            )
        }
    }
}

impl<'plan> StreamItemPlanRender<'plan, Native> for StreamItemRenderer<'_> {
    type Output = Result<StreamItem>;

    fn direct(&mut self, ty: &'plan DirectValueType, _: ByteSize) -> Self::Output {
        let ty = direct_type(ty, self.context)?;
        let method = "NativeStreamPopBatch";
        Ok(StreamItem {
            ty: ty.clone(),
            read_batch: format!(
                "if (maxCount == 0) return global::System.Array.Empty<{ty}>();\n{ty}[] items = new {ty}[checked((int)maxCount)];\nnuint count = NativeMethods.{method}(subscription, items, maxCount);\nif (count > maxCount) throw new global::System.InvalidOperationException(\"stream batch exceeded capacity\");\nif (count != maxCount) global::System.Array.Resize(ref items, checked((int)count));\nreturn items;"
            ),
            encoded: false,
            bool_item: matches!(ty.to_string().as_str(), "bool"),
        })
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        read: &'plan ReadPlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        if shape != native::BufferShape::Buffer {
            return super::super::unsupported("encoded stream item shape");
        }
        let ty = type_name::type_ref(ty, self.context)?;
        let reader = Identifier::parse("boltffiReader")?;
        let decode = read
            .render_with(&mut Reader::new(reader.clone(), self.context))
            .map(ReadExpression::into_expression)?;
        Ok(StreamItem {
            ty: ty.clone(),
            read_batch: format!(
                "FfiBuf buffer = NativeMethods.NativeStreamPopBatch(subscription, maxCount);\ntry\n{{\n    if (buffer.ptr == 0 || buffer.len == 0) return global::System.Array.Empty<{ty}>();\n    WireReader {reader} = new WireReader(buffer);\n    int count = checked((int){reader}.ReadU32());\n    {ty}[] items = new {ty}[count];\n    for (int index = 0; index < count; index++) items[index] = {decode};\n    return items;\n}}\nfinally\n{{\n    NativeMethods.FreeBuf(buffer);\n}}"
            ),
            encoded: true,
            bool_item: false,
        })
    }
}

fn indent(source: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    source
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
