use boltffi_binding::CodecNode;

/// Which side of the extension boundary finishes an encoded value.
///
/// The extension owns the conversion for shapes it can complete in one C
/// call; every other shape crosses as wire bytes for the package-side
/// codec. The package renderer and the extension renderer both match this
/// classification, so the two halves of one function cannot disagree
/// about who decodes.
///
/// # Example
///
/// A `-> String` return classifies as `Utf8Text`: the extension returns
/// the finished `str` and the package body is a bare native call. A
/// `-> Vec<String>` return classifies as `WireBytes`: the extension
/// returns the payload and the package decodes it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodedCrossing {
    Utf8Text,
    WireBytes,
}

impl EncodedCrossing {
    pub fn of(root: &CodecNode) -> Self {
        match root {
            CodecNode::String => Self::Utf8Text,
            _ => Self::WireBytes,
        }
    }
}
