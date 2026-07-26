use std::collections::BTreeMap;

use boltffi_binding::{CallbackId, SymbolId};

use crate::core::{Error, Result};

use super::super::{
    CallbackRegistration, DirectStreamBatchMethod, NativeMethod, StreamProtocolMethods,
};

const JNI_BRIDGE: &str = "jni";
const DUPLICATE_CALLBACK_ID: &str = "duplicate source callback id";
const DUPLICATE_SYMBOL_ID: &str = "duplicate source symbol id";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CallbackIndex(usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MethodIndex(usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StreamIndex(usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DirectBatchIndex(usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MethodLocation {
    Root(MethodIndex),
    Stream {
        stream: StreamIndex,
        method: MethodIndex,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DirectBatchLocation {
    stream: StreamIndex,
    batch: DirectBatchIndex,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SymbolLocation {
    Method(MethodLocation),
    DirectBatch(DirectBatchLocation),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LocationIndex<Id, Location> {
    locations: BTreeMap<Id, Location>,
}

impl<Id: Ord, Location> LocationIndex<Id, Location> {
    fn new(
        entries: impl IntoIterator<Item = (Id, Location)>,
        duplicate_invariant: &'static str,
    ) -> Result<Self> {
        entries
            .into_iter()
            .try_fold(
                BTreeMap::new(),
                |mut locations, (id, location)| match locations.insert(id, location) {
                    Some(_) => Err(Error::BrokenBridgeContract {
                        bridge: JNI_BRIDGE,
                        invariant: duplicate_invariant,
                    }),
                    None => Ok(locations),
                },
            )
            .map(|locations| Self { locations })
    }

    fn get(&self, id: &Id) -> Option<&Location> {
        self.locations.get(id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceIndex {
    callbacks: LocationIndex<CallbackId, CallbackIndex>,
    symbols: LocationIndex<SymbolId, SymbolLocation>,
}

impl SourceIndex {
    pub fn new(
        callbacks: &[CallbackRegistration],
        methods: &[NativeMethod],
        streams: &[StreamProtocolMethods],
    ) -> Result<Self> {
        let callbacks = LocationIndex::new(
            callbacks
                .iter()
                .enumerate()
                .map(|(index, registration)| (registration.id(), CallbackIndex(index))),
            DUPLICATE_CALLBACK_ID,
        )?;
        let symbols = LocationIndex::new(
            methods
                .iter()
                .enumerate()
                .map(|(index, method)| {
                    (
                        method.source_symbol(),
                        SymbolLocation::Method(MethodLocation::Root(MethodIndex(index))),
                    )
                })
                .chain(
                    streams
                        .iter()
                        .enumerate()
                        .flat_map(|(stream_index, stream)| {
                            stream.methods().iter().enumerate().map(
                                move |(method_index, method)| {
                                    (
                                        method.source_symbol(),
                                        SymbolLocation::Method(MethodLocation::Stream {
                                            stream: StreamIndex(stream_index),
                                            method: MethodIndex(method_index),
                                        }),
                                    )
                                },
                            )
                        }),
                )
                .chain(
                    streams
                        .iter()
                        .enumerate()
                        .flat_map(|(stream_index, stream)| {
                            stream.direct_batches().iter().enumerate().map(
                                move |(batch_index, batch)| {
                                    (
                                        batch.source_symbol(),
                                        SymbolLocation::DirectBatch(DirectBatchLocation {
                                            stream: StreamIndex(stream_index),
                                            batch: DirectBatchIndex(batch_index),
                                        }),
                                    )
                                },
                            )
                        }),
                ),
            DUPLICATE_SYMBOL_ID,
        )?;
        Ok(Self { callbacks, symbols })
    }

    pub fn callback<'contract>(
        &self,
        id: CallbackId,
        callbacks: &'contract [CallbackRegistration],
    ) -> Option<&'contract CallbackRegistration> {
        self.callbacks
            .get(&id)
            .and_then(|location| callbacks.get(location.0))
    }

    pub fn method<'contract>(
        &self,
        id: SymbolId,
        methods: &'contract [NativeMethod],
        streams: &'contract [StreamProtocolMethods],
    ) -> Option<&'contract NativeMethod> {
        match self.symbols.get(&id)? {
            SymbolLocation::Method(MethodLocation::Root(method)) => methods.get(method.0),
            SymbolLocation::Method(MethodLocation::Stream { stream, method }) => streams
                .get(stream.0)
                .and_then(|protocol| protocol.methods().get(method.0)),
            SymbolLocation::DirectBatch(_) => None,
        }
    }

    pub fn direct_batch<'contract>(
        &self,
        id: SymbolId,
        streams: &'contract [StreamProtocolMethods],
    ) -> Option<&'contract DirectStreamBatchMethod> {
        match self.symbols.get(&id)? {
            SymbolLocation::DirectBatch(location) => streams
                .get(location.stream.0)
                .and_then(|protocol| protocol.direct_batches().get(location.batch.0)),
            SymbolLocation::Method(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use boltffi_binding::{CallbackId, SymbolId};

    use crate::core::Error;

    use super::{
        CallbackIndex, DUPLICATE_CALLBACK_ID, DUPLICATE_SYMBOL_ID, DirectBatchIndex,
        DirectBatchLocation, JNI_BRIDGE, LocationIndex, MethodIndex, MethodLocation, StreamIndex,
        SymbolLocation,
    };

    #[test]
    fn rejects_duplicate_callback_ids() {
        let id = CallbackId::from_raw(7);
        let result = LocationIndex::new(
            [(id, CallbackIndex(0)), (id, CallbackIndex(1))],
            DUPLICATE_CALLBACK_ID,
        );

        assert_eq!(
            result,
            Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: DUPLICATE_CALLBACK_ID,
            })
        );
    }

    #[test]
    fn rejects_duplicate_symbol_ids_across_method_shapes() {
        let id = SymbolId::from_raw(11);
        let result = LocationIndex::new(
            [
                (
                    id,
                    SymbolLocation::Method(MethodLocation::Root(MethodIndex(0))),
                ),
                (
                    id,
                    SymbolLocation::DirectBatch(DirectBatchLocation {
                        stream: StreamIndex(0),
                        batch: DirectBatchIndex(0),
                    }),
                ),
            ],
            DUPLICATE_SYMBOL_ID,
        );

        assert_eq!(
            result,
            Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: DUPLICATE_SYMBOL_ID,
            })
        );
    }
}
