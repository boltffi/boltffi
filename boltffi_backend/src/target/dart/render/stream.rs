use askama::Template;
use boltffi_binding::{
    ByteSize, DirectValueType, Native, Primitive, ReadPlan, StreamDecl, StreamItemPlanRender,
    StreamMode, TypeRef, native,
};

use crate::{
    bridge::c::{CBridgeContract, Stream as CStream},
    core::{Emitted, Error, RenderContext, Result},
    target::dart::syntax::{Identifier, TypeFragment},
};

use super::super::{codec::Reader, name_style::Name, native::NativeType, type_name};
use super::{Documentation, declaration_name, indent};

#[derive(Template)]
#[template(path = "target/dart/stream.dart", escape = "none")]
struct StreamTemplate<'a> {
    stream: &'a Stream,
}

#[derive(Template)]
#[template(path = "target/dart/stream_method.dart", escape = "none")]
struct StreamMethodTemplate<'a> {
    method: &'a StreamMethod,
}

pub struct Stream {
    owner: Option<Identifier>,
    name: Identifier,
    method: String,
}

struct StreamMethod {
    documentation: Documentation,
    name: Identifier,
    item_type: TypeFragment,
    mode: DartStreamMode,
    context: StreamContext,
    delivery: Delivery,
}

enum DartStreamMode {
    Async,
    Callback,
    Batch,
}

struct StreamContext {
    receiver: Option<&'static str>,
    subscribe: String,
    poll: String,
    wait: String,
    unsubscribe: String,
    free: String,
    item_size: Option<u64>,
}

enum StreamItem {
    Direct {
        public_type: TypeFragment,
        pop_native_type: String,
        byte_size: u64,
        decode: String,
        // Numeric primitives can be read out of the batch buffer with a
        // single bulk `.asTypedList()` view instead of a `List.generate`
        // that pointer-dereferences one element at a time.
        supports_typed_list: bool,
    },
    Encoded {
        public_type: TypeFragment,
        decode: String,
    },
}

struct Delivery {
    setup: String,
    has_items: String,
    prepare: Option<String>,
    read: String,
    cleanup: Option<String>,
}

struct ItemRenderer<'context, 'bindings, 'bridge> {
    bridge: &'bridge CBridgeContract,
    protocol: &'bridge CStream,
    context: &'context RenderContext<'bindings, Native>,
}

impl Stream {
    pub fn from_declaration(
        declaration: &StreamDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let protocol =
            bridge
                .source_stream(declaration.id())
                .ok_or(Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "Dart stream protocol is missing from the C bridge",
                })?;
        let item = declaration.item().render_with(&mut ItemRenderer {
            bridge,
            protocol,
            context,
        })?;
        let owner = declaration
            .owner()
            .map(|owner| {
                context
                    .class(owner)
                    .ok_or(Error::UnexpectedBindingShape {
                        layer: "dart stream",
                        shape: "missing stream owner",
                    })
                    .and_then(|owner| declaration_name(owner.name()))
            })
            .transpose()?;
        let method = StreamMethod {
            documentation: Documentation::new(declaration.meta().doc(), 0),
            name: Name::new(declaration.name()).lower_camel()?,
            item_type: item.public_type().clone(),
            mode: DartStreamMode::from_binding(declaration.mode())?,
            context: StreamContext::new(protocol, owner.is_some(), item.byte_size()),
            delivery: item.delivery(protocol, bridge)?,
        };
        Ok(Self {
            owner,
            name: method.name.clone(),
            method: StreamMethodTemplate { method: &method }
                .render()
                .expect("rendering an in-memory Dart stream method template cannot fail"),
        })
    }

    pub fn render(self) -> Emitted {
        Emitted::primary(
            StreamTemplate { stream: &self }
                .render()
                .expect("rendering an in-memory Dart stream template cannot fail"),
        )
    }

    fn owner(&self) -> Option<&Identifier> {
        self.owner.as_ref()
    }

    fn method(&self) -> &str {
        &self.method
    }

    fn method_name(&self) -> &Identifier {
        &self.name
    }

    fn associated_method(&self) -> String {
        indent(&self.method, 2)
    }
}

impl StreamMethod {
    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn item_type(&self) -> &TypeFragment {
        &self.item_type
    }

    fn mode(&self) -> &DartStreamMode {
        &self.mode
    }

    fn context(&self) -> &StreamContext {
        &self.context
    }

    fn delivery(&self) -> &Delivery {
        &self.delivery
    }
}

impl DartStreamMode {
    fn from_binding(mode: StreamMode) -> Result<Self> {
        match mode {
            StreamMode::Async => Ok(Self::Async),
            StreamMode::Callback => Ok(Self::Callback),
            StreamMode::Batch => Ok(Self::Batch),
            _ => super::super::unsupported("unknown stream mode"),
        }
    }

    fn asynchronous(&self) -> bool {
        matches!(self, Self::Async)
    }

    fn callback(&self) -> bool {
        matches!(self, Self::Callback)
    }
}

impl StreamContext {
    fn new(protocol: &CStream, owned: bool, item_size: Option<u64>) -> Self {
        Self {
            receiver: owned.then_some("_handle"),
            subscribe: protocol.subscribe().name().to_owned(),
            poll: protocol.poll().name().to_owned(),
            wait: protocol.wait().name().to_owned(),
            unsubscribe: protocol.unsubscribe().name().to_owned(),
            free: protocol.free().name().to_owned(),
            item_size,
        }
    }

    fn owned(&self) -> bool {
        self.receiver.is_some()
    }

    fn receiver(&self) -> &str {
        self.receiver.unwrap_or("")
    }

    fn subscribe(&self) -> &str {
        &self.subscribe
    }

    fn poll(&self) -> &str {
        &self.poll
    }

    fn wait(&self) -> &str {
        &self.wait
    }

    fn unsubscribe(&self) -> &str {
        &self.unsubscribe
    }

    fn free(&self) -> &str {
        &self.free
    }

    fn item_size(&self) -> Option<u64> {
        self.item_size
    }
}

impl StreamItem {
    fn public_type(&self) -> &TypeFragment {
        match self {
            Self::Direct { public_type, .. } | Self::Encoded { public_type, .. } => public_type,
        }
    }

    fn byte_size(&self) -> Option<u64> {
        match self {
            Self::Direct { byte_size, .. } => Some(*byte_size),
            Self::Encoded { .. } => None,
        }
    }

    fn delivery(&self, protocol: &CStream, bridge: &CBridgeContract) -> Result<Delivery> {
        Ok(match self {
            Self::Direct {
                pop_native_type,
                byte_size,
                decode,
                supports_typed_list,
                ..
            } => Delivery {
                // Every batch read pays for this allocation (the drain loop
                // in `_$$BoltStreamCtx.stream` can call it dozens of times
                // per poll wake), so it skips the finalizer the same way the
                // async completion-status allocation does, and disposes
                // synchronously via `cleanup` below.
                setup: format!(
                    "final _l$storage = _$$BoltCallocPtr<$$ffi.Uint8>.allocUnmanaged(batchSize * {byte_size});\nfinal _l$count = _f${}(handle, _l$storage.ptr.cast<{pop_native_type}>(), batchSize);",
                    protocol.pop_batch().name(),
                ),
                has_items: "_l$count != 0".to_owned(),
                prepare: None,
                // Numeric primitives: one bulk typed-data view instead of a
                // `List.generate` that pointer-dereferences per element.
                // `.sublist(0)` copies it, since `cleanup` below disposes
                // `_l$storage` before batch mode returns this value.
                read: if *supports_typed_list {
                    format!(
                        "_l$storage.ptr.cast<{pop_native_type}>().asTypedList(_l$count).sublist(0)"
                    )
                } else {
                    format!("List.generate(_l$count, (_l$index) => {decode})")
                },
                cleanup: Some("_l$storage.dispose();".to_owned()),
            },
            Self::Encoded { decode, .. } => Delivery {
                setup: format!(
                    "final _l$buffer = _f${}(handle, batchSize);",
                    protocol.pop_batch().name(),
                ),
                has_items: "_l$buffer.len != 0".to_owned(),
                prepare: Some(
                    "final _l$reader = _$$BoltWireDecoder(_$$BoltBufReader.fromSpan(_l$buffer.ptr, _l$buffer.len));"
                        .to_owned(),
                ),
                read: format!("_l$reader.readList((_l$reader) => {decode})"),
                cleanup: Some(format!(
                    "_f${}(_l$buffer);",
                    bridge.support().buffer_free()?.name(),
                )),
            },
        })
    }
}

impl Delivery {
    fn setup(&self) -> &str {
        &self.setup
    }

    fn has_items(&self) -> &str {
        &self.has_items
    }

    fn prepare(&self) -> Option<&str> {
        self.prepare.as_deref()
    }

    fn read(&self) -> &str {
        &self.read
    }

    fn cleanup(&self) -> Option<&str> {
        self.cleanup.as_deref()
    }
}

impl<'plan> StreamItemPlanRender<'plan, Native> for ItemRenderer<'_, '_, '_> {
    type Output = Result<StreamItem>;

    fn direct(&mut self, ty: &'plan DirectValueType, size: ByteSize) -> Self::Output {
        let batch = self
            .protocol
            .direct_batch()
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "Dart direct stream item disagrees with the C bridge",
            })?;
        let pop_native_type = NativeType::from_c(batch.item())?.native().to_owned();
        let decode_native_type = match ty {
            DirectValueType::Record(id) => self
                .bridge
                .source_direct_record(*id)
                .map(|record| format!("_$${}", record.name()))
                .ok_or(Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "Dart direct stream record is missing from the C bridge",
                })?,
            _ => pop_native_type.clone(),
        };
        let native_value =
            format!("_l$storage.ptr.cast<{decode_native_type}>().elementAt(_l$index).value");
        let decode = match ty {
            DirectValueType::Primitive(_) => native_value,
            DirectValueType::Enum(_) => format!(
                "{}._m$fromDiscriminant({native_value})",
                type_name::direct_value(ty, self.context)?,
            ),
            DirectValueType::Record(_) => format!(
                "{}._m$fromStruct(_l$storage.ptr.cast<{decode_native_type}>().elementAt(_l$index).ref)",
                type_name::direct_value(ty, self.context)?,
            ),
            _ => return super::super::unsupported("unknown direct stream item"),
        };
        // Every fixed-width numeric primitive maps onto a Dart typed-data
        // class, so the batch reads with one `.asTypedList()` call. Excluded:
        // `bool` (no typed-list view), `isize`/`usize` (platform-dependent
        // width, no `Pointer<IntPtr>.asTypedList`).
        let supports_typed_list = matches!(
            ty,
            DirectValueType::Primitive(
                Primitive::I8
                    | Primitive::U8
                    | Primitive::I16
                    | Primitive::U16
                    | Primitive::I32
                    | Primitive::U32
                    | Primitive::I64
                    | Primitive::U64
                    | Primitive::F32
                    | Primitive::F64
            )
        );
        Ok(StreamItem::Direct {
            public_type: type_name::direct_value(ty, self.context)?,
            pop_native_type,
            byte_size: size.get(),
            decode,
            supports_typed_list,
        })
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        read: &'plan ReadPlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        if shape != native::BufferShape::Buffer {
            return super::super::unsupported("Dart encoded stream buffer shape");
        }
        Ok(StreamItem::Encoded {
            public_type: type_name::type_ref(ty, self.context)?,
            decode: read
                .render_with(&mut Reader::new("_l$reader", self.context))?
                .into_source(),
        })
    }
}
