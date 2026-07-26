use boltffi_binding::DeclarationRef;

use super::{bindings, bridge, rendered_fixture, source::SourceFixture};

#[test]
fn jni_bridge_indexes_stream_symbols_by_source_id() {
    let source = SourceFixture::one("stream/protocol_functions").read();
    let bindings = bindings(&source);
    let streams = bindings
        .decls()
        .iter()
        .filter_map(|decl| match DeclarationRef::from(decl) {
            DeclarationRef::Stream(stream) => Some(stream),
            _ => None,
        })
        .collect::<Vec<_>>();
    let output = bridge(&source);
    let contract = output.contract();

    streams.iter().for_each(|stream| {
        [
            stream.protocol().subscribe(),
            stream.protocol().pop_batch(),
            stream.protocol().wait(),
            stream.protocol().poll(),
            stream.protocol().unsubscribe(),
            stream.protocol().free(),
        ]
        .into_iter()
        .for_each(|symbol| {
            let native_method = contract.source_method(symbol.id()).is_some();
            let direct_batch = contract.source_direct_batch(symbol.id()).is_some();
            assert_ne!(
                native_method, direct_batch,
                "each stream symbol must select exactly one JNI method shape"
            );
        });
    });
}

#[test]
fn jni_bridge_renders_stream_protocol_functions() {
    insta::assert_snapshot!(rendered_fixture("stream/protocol_functions"));
}
