use boltffi_binding::{
    ByteSize, DirectValueType, Native, ReadPlan, StreamDecl, StreamItemPlan, StreamItemPlanRender,
    TypeRef, native,
};

use crate::core::Result;

use super::{Function, Parameter, Type, function::Signature, names::Names};

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
/// C ABI functions that drive one lowered stream.
pub struct Stream {
    subscribe: Function,
    pop_batch: StreamBatch,
    wait: Function,
    poll: Function,
    unsubscribe: Function,
    free: Function,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
/// C ABI shape of the stream batch function.
pub enum StreamBatch {
    /// A batch copied into a caller-provided direct item buffer.
    Direct(DirectStreamBatch),
    /// A batch returned as an owned encoded buffer.
    Encoded(Function),
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
/// C ABI batch function for direct stream items.
pub struct DirectStreamBatch {
    function: Function,
    item: Type,
    item_size: ByteSize,
}

struct StreamBatchBuilder<'stream> {
    symbol: &'stream str,
    subscription: Type,
    names: &'stream Names,
}

impl Stream {
    /// Creates the C stream protocol from a lowered stream declaration.
    pub fn from_decl(stream: &StreamDecl<Native>, names: &Names) -> Result<Self> {
        let protocol = stream.protocol();
        let subscription = Type::handle_carrier(stream.handle())?;
        let subscribe_params = stream
            .owner()
            .map(|owner| {
                names
                    .class_handle(owner)
                    .and_then(Type::handle_carrier)
                    .and_then(|ty| Parameter::new("receiver", ty))
            })
            .transpose()?
            .into_iter()
            .collect();

        Ok(Self {
            subscribe: Function::new(
                protocol.subscribe().name().as_str(),
                subscribe_params,
                subscription.clone(),
            )?,
            pop_batch: StreamBatch::from_plan(
                protocol.pop_batch().name().as_str(),
                stream.item(),
                subscription.clone(),
                names,
            )?,
            wait: Function::new(
                protocol.wait().name().as_str(),
                vec![
                    Parameter::new("subscription", subscription.clone())?,
                    Parameter::new("timeout_milliseconds", Type::Uint32)?,
                ],
                Type::WaitResult,
            )?,
            poll: Function::new(
                protocol.poll().name().as_str(),
                vec![
                    Parameter::new("subscription", subscription.clone())?,
                    Parameter::continuation_data("callback")?,
                    Parameter::continuation_callback("callback", Type::StreamPollResult)?,
                ],
                Type::Void,
            )?,
            unsubscribe: Function::new(
                protocol.unsubscribe().name().as_str(),
                vec![Parameter::new("subscription", subscription.clone())?],
                Type::Void,
            )?,
            free: Function::new(
                protocol.free().name().as_str(),
                vec![Parameter::new("subscription", subscription)?],
                Type::Void,
            )?,
        })
    }

    /// Returns every C function exposed by this stream protocol.
    pub fn functions(&self) -> [&Function; 6] {
        [
            &self.subscribe,
            self.pop_batch.function(),
            &self.wait,
            &self.poll,
            &self.unsubscribe,
            &self.free,
        ]
    }

    /// Returns the C stream subscription function.
    pub fn subscribe(&self) -> &Function {
        &self.subscribe
    }

    /// Returns the C stream batch function.
    pub fn pop_batch(&self) -> &Function {
        self.pop_batch.function()
    }

    /// Returns the C stream wait function.
    pub fn wait(&self) -> &Function {
        &self.wait
    }

    /// Returns the C stream poll function.
    pub fn poll(&self) -> &Function {
        &self.poll
    }

    /// Returns the C stream unsubscribe function.
    pub fn unsubscribe(&self) -> &Function {
        &self.unsubscribe
    }

    /// Returns the C stream free function.
    pub fn free(&self) -> &Function {
        &self.free
    }

    /// Returns the direct batch function when this stream copies direct items.
    pub fn direct_batch(&self) -> Option<&DirectStreamBatch> {
        match &self.pop_batch {
            StreamBatch::Direct(batch) => Some(batch),
            StreamBatch::Encoded(_) => None,
        }
    }
}

impl StreamBatch {
    /// Returns the C batch function.
    pub fn function(&self) -> &Function {
        match self {
            Self::Direct(batch) => batch.function(),
            Self::Encoded(function) => function,
        }
    }

    fn from_plan(
        symbol: &str,
        item: &StreamItemPlan<Native>,
        subscription: Type,
        names: &Names,
    ) -> Result<Self> {
        item.render_with(&mut StreamBatchBuilder {
            symbol,
            subscription,
            names,
        })
    }
}

impl DirectStreamBatch {
    /// Returns the C batch function.
    pub fn function(&self) -> &Function {
        &self.function
    }

    /// Returns the C type copied into the output buffer.
    pub fn item(&self) -> &Type {
        &self.item
    }

    /// Returns the byte size of one stream item.
    pub const fn item_size(&self) -> ByteSize {
        self.item_size
    }
}

impl<'plan> StreamItemPlanRender<'plan, Native> for StreamBatchBuilder<'_> {
    type Output = Result<StreamBatch>;

    fn direct(&mut self, ty: &'plan DirectValueType, size: ByteSize) -> Self::Output {
        let item = self.names.direct_value(ty)?;
        Function::new(
            self.symbol,
            vec![
                Parameter::new("subscription", self.subscription.clone())?,
                Parameter::new("output_ptr", Type::MutPointer(Box::new(item.clone())))?,
                Parameter::new("output_capacity", Type::PointerWidth)?,
            ],
            Type::PointerWidth,
        )
        .map(|function| {
            StreamBatch::Direct(DirectStreamBatch {
                function,
                item,
                item_size: size,
            })
        })
    }

    fn encoded(
        &mut self,
        _: &'plan TypeRef,
        _: &'plan ReadPlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        Function::new(
            self.symbol,
            vec![
                Parameter::new("subscription", self.subscription.clone())?,
                Parameter::new("max_count", Type::PointerWidth)?,
            ],
            Signature::new(self.names, Vec::new()).encoded_return(shape)?,
        )
        .map(StreamBatch::Encoded)
    }
}
