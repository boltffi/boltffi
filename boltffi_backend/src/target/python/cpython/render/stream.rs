use askama::Template as AskamaTemplate;
use boltffi_binding::{
    ByteSize, CanonicalName, ClassId, DirectValueType, Native, NativeSymbol, Primitive, ReadPlan,
    StreamDecl, StreamItemPlan, StreamItemPlanRender, TypeRef, native,
};

use crate::{
    bridge::{
        c::{Identifier, TypeFragment},
        python_cext::{ExtensionMethod, MethodFlags, MethodName, PythonCExtBridgeContract},
    },
    core::{Emitted, Error, RenderContext, Result},
    target::python::{
        cpython::render::{direct, primitive, result},
        name_style::Name,
        syntax::Identifier as PythonIdentifier,
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/stream.c", escape = "none")]
struct Template {
    subscribe: Method,
    pop_batch: Method,
    wait: Method,
    unsubscribe: Method,
    free: Method,
    item: Item,
    stream_handle_type: TypeFragment,
    stream_handle_parser: Identifier,
    stream_handle_boxer: Identifier,
    subscribe_arity: usize,
    receiver: Option<Receiver>,
}

pub struct Stream {
    subscribe: Method,
    pop_batch: Method,
    wait: Method,
    unsubscribe: Method,
    free: Method,
    item: Item,
    handle: primitive::Runtime,
    receiver: Option<Receiver>,
}

impl Stream {
    pub fn from_declaration(
        declaration: &StreamDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let owner = declaration
            .owner()
            .map(|owner| owner_name(owner, context))
            .transpose()?;
        let symbols = Symbols::new(declaration, owner)?;
        Ok(Self {
            subscribe: Method::new(
                symbols.subscribe()?,
                declaration.protocol().subscribe(),
                MethodFlags::FastCall,
                bridge,
            )?,
            pop_batch: Method::new(
                symbols.pop_batch()?,
                declaration.protocol().pop_batch(),
                MethodFlags::FastCall,
                bridge,
            )?,
            wait: Method::new(
                symbols.wait()?,
                declaration.protocol().wait(),
                MethodFlags::FastCall,
                bridge,
            )?,
            unsubscribe: Method::new(
                symbols.unsubscribe()?,
                declaration.protocol().unsubscribe(),
                MethodFlags::FastCall,
                bridge,
            )?,
            free: Method::new(
                symbols.free()?,
                declaration.protocol().free(),
                MethodFlags::FastCall,
                bridge,
            )?,
            item: Item::new(declaration.item(), bridge, context)?,
            handle: primitive::Runtime::native_handle(declaration.handle())?,
            receiver: declaration
                .owner()
                .map(|owner| Receiver::new(owner, context))
                .transpose()?,
        })
    }

    pub fn render(self) -> Result<Emitted> {
        let stream_handle_type = self.handle.c_type()?;
        let stream_handle_parser = self.handle.parser()?;
        let stream_handle_boxer = self.handle.boxer()?;
        let source = Template {
            subscribe: self.subscribe,
            pop_batch: self.pop_batch,
            wait: self.wait,
            unsubscribe: self.unsubscribe,
            free: self.free,
            item: self.item,
            stream_handle_type,
            stream_handle_parser,
            stream_handle_boxer,
            subscribe_arity: self.receiver.iter().count(),
            receiver: self.receiver,
        }
        .render()?;
        Ok(Emitted::primary(source))
    }

    pub fn methods(&self) -> impl Iterator<Item = &ExtensionMethod> {
        [
            &self.subscribe.method,
            &self.pop_batch.method,
            &self.wait.method,
            &self.unsubscribe.method,
            &self.free.method,
        ]
        .into_iter()
    }

    pub fn primitives(&self) -> Vec<primitive::Runtime> {
        [
            Some(self.handle),
            Some(primitive::Runtime::new(Primitive::U32)),
            Some(primitive::Runtime::new(Primitive::USize)),
            self.receiver.as_ref().map(|receiver| receiver.primitive),
            self.item.primitive,
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    pub fn owned_buffer(&self) -> Option<result::OwnedBuffer> {
        self.item.owned_buffer()
    }
}

#[derive(Clone)]
struct Receiver {
    primitive: primitive::Runtime,
    handle_type: TypeFragment,
    parser: Identifier,
}

impl Receiver {
    fn new(owner: ClassId, context: &RenderContext<Native>) -> Result<Self> {
        let handle =
            context
                .class(owner)
                .map(|class| class.handle())
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "stream owner without class declaration",
                })?;
        let handle = primitive::Runtime::native_handle(handle)?;
        Ok(Self {
            primitive: handle,
            handle_type: handle.c_type()?,
            parser: handle.parser()?,
        })
    }
}

struct Method {
    python_name: PythonIdentifier,
    wrapper: Identifier,
    storage: Identifier,
    method: ExtensionMethod,
}

impl Method {
    fn new(
        python_name: PythonIdentifier,
        symbol: &NativeSymbol,
        flags: MethodFlags,
        bridge: &PythonCExtBridgeContract,
    ) -> Result<Self> {
        let loaded = bridge
            .loaded_function(symbol)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "stream method without C bridge symbol",
            })?;
        let wrapper = Identifier::parse(format!(
            "boltffi_python_stream_wrapper_{}",
            symbol.name().as_str()
        ))?;
        let method = ExtensionMethod::new(
            MethodName::parse(python_name.as_str())?,
            wrapper.clone(),
            flags,
        )?;
        Ok(Self {
            python_name,
            wrapper,
            storage: loaded.storage_name().clone(),
            method,
        })
    }
}

struct Item {
    kind: ItemKind,
    primitive: Option<primitive::Runtime>,
}

impl Item {
    fn new(
        plan: &StreamItemPlan<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        plan.render_with(&mut StreamItem { bridge, context })
    }

    fn is_direct(&self) -> bool {
        matches!(self.kind, ItemKind::Direct { .. })
    }

    fn c_type(&self) -> &TypeFragment {
        match &self.kind {
            ItemKind::Direct { c_type, .. } => c_type,
            ItemKind::Encoded => {
                unreachable!("encoded stream items do not have direct C item types")
            }
        }
    }

    fn boxer(&self) -> &Identifier {
        match &self.kind {
            ItemKind::Direct { boxer, .. } => boxer,
            ItemKind::Encoded => unreachable!("encoded stream items do not have direct C boxers"),
        }
    }

    fn owned_buffer(&self) -> Option<result::OwnedBuffer> {
        match self.kind {
            ItemKind::Direct { .. } => None,
            ItemKind::Encoded => Some(result::OwnedBuffer::RawWire),
        }
    }

    fn direct(
        ty: &DirectValueType,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let direct = direct::NativeSlot::from_direct_value(ty, bridge, context)?;
        Ok(Self {
            kind: ItemKind::Direct {
                c_type: direct.c_type().clone(),
                boxer: direct.boxer().clone(),
            },
            primitive: direct.primitive(),
        })
    }
}

struct StreamItem<'render> {
    bridge: &'render PythonCExtBridgeContract,
    context: &'render RenderContext<'render, Native>,
}

impl<'plan, 'render> StreamItemPlanRender<'plan, Native> for StreamItem<'render> {
    type Output = Result<Item>;

    fn direct(&mut self, ty: &'plan DirectValueType, _: ByteSize) -> Self::Output {
        Item::direct(ty, self.bridge, self.context)
    }

    fn encoded(
        &mut self,
        _: &'plan TypeRef,
        _: &'plan ReadPlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        match shape {
            native::BufferShape::Buffer => Ok(Item {
                kind: ItemKind::Encoded,
                primitive: None,
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "encoded stream item shape",
            }),
        }
    }
}

enum ItemKind {
    Direct {
        c_type: TypeFragment,
        boxer: Identifier,
    },
    Encoded,
}

/// The declared name of a stream's owning class, for name qualification.
pub fn owner_name<'bindings>(
    owner: ClassId,
    context: &RenderContext<'bindings, Native>,
) -> Result<&'bindings CanonicalName> {
    context
        .class(owner)
        .map(|class| class.name())
        .ok_or(Error::UnsupportedTarget {
            target: "python",
            shape: "stream owner without class declaration",
        })
}

pub struct Symbols {
    stem: String,
}

impl Symbols {
    /// Extension-module accessor names for one stream, qualified by the owning class.
    ///
    /// Without the prefix two classes declaring a stream of the same name emit the same
    /// accessors, and every subscription decodes through whichever one was registered last.
    pub fn new(declaration: &StreamDecl<Native>, owner: Option<&CanonicalName>) -> Result<Self> {
        let stem = match owner {
            Some(owner) => Name::qualified(owner, declaration.name())?.to_string(),
            None => Name::new(declaration.name()).function_text()?,
        };
        Ok(Self { stem })
    }

    pub fn subscribe(&self) -> Result<PythonIdentifier> {
        PythonIdentifier::parse(self.stem.clone())
    }

    pub fn pop_batch(&self) -> Result<PythonIdentifier> {
        PythonIdentifier::parse(format!("{}_pop_batch", self.stem))
    }

    pub fn wait(&self) -> Result<PythonIdentifier> {
        PythonIdentifier::parse(format!("{}_wait", self.stem))
    }

    pub fn unsubscribe(&self) -> Result<PythonIdentifier> {
        PythonIdentifier::parse(format!("{}_unsubscribe", self.stem))
    }

    pub fn free(&self) -> Result<PythonIdentifier> {
        PythonIdentifier::parse(format!("{}_free", self.stem))
    }
}
