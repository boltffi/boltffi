use boltffi_binding::Primitive;

use crate::core::Result;

use super::super::native::NativeType;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrimitiveVector {
    primitive: Primitive,
    native: NativeType,
}

impl PrimitiveVector {
    pub fn new(primitive: Primitive) -> Result<Self> {
        Ok(Self {
            primitive,
            native: NativeType::primitive(primitive)?,
        })
    }

    pub fn native(&self) -> &NativeType {
        &self.native
    }

    pub fn populate(&self, storage: &str, source: &str) -> Result<String> {
        Ok(match self.primitive {
            Primitive::Bool => format!(
                "for (var _l$index = 0; _l$index < {source}.length; _l$index++) {{ {storage}.ptr.cast<$$ffi.Uint8>().elementAt(_l$index).value = {source}[_l$index] ? 1 : 0; }}"
            ),
            Primitive::ISize | Primitive::USize => format!(
                "for (var _l$index = 0; _l$index < {source}.length; _l$index++) {{ {storage}.ptr.elementAt(_l$index).value = {source}[_l$index]; }}"
            ),
            Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32
            | Primitive::I64
            | Primitive::U64
            | Primitive::F32
            | Primitive::F64 => {
                format!("{storage}.ptr.asTypedList({source}.length).setAll(0, {source});")
            }
            _ => return super::super::unsupported("unknown direct-vector primitive"),
        })
    }

    /// Copies a mutable direct-vector's native storage back into the host
    /// list after the call (`&mut [T]` parameters). `Bool`, `ISize`, and
    /// `USize` have no `Pointer<T>.asTypedList()` view in `dart:ffi` -- same
    /// reason `populate` and `copied_from` special-case them above -- so
    /// they're copied back element by element instead.
    pub fn writeback(&self, storage: &str, source: &str) -> Result<String> {
        Ok(match self.primitive {
            Primitive::Bool => format!(
                "{source}.setAll(0, List<bool>.generate({source}.length, (_l$index) => {storage}.ptr.cast<$$ffi.Uint8>().elementAt(_l$index).value != 0));"
            ),
            Primitive::ISize | Primitive::USize => format!(
                "{source}.setAll(0, List<int>.generate({source}.length, (_l$index) => {storage}.ptr.elementAt(_l$index).value));"
            ),
            Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32
            | Primitive::I64
            | Primitive::U64
            | Primitive::F32
            | Primitive::F64 => {
                format!("{source}.setAll(0, {storage}.ptr.asTypedList({source}.length));")
            }
            _ => return super::super::unsupported("unknown direct-vector primitive"),
        })
    }

    pub fn copied_from(&self, pointer: &str, length: &str) -> Result<String> {
        let copied = match self.primitive {
            Primitive::Bool => format!(
                "$$BoltBoolList._m$fromUint8List($$typed_data.Uint8List.fromList({pointer}.cast<$$ffi.Uint8>().asTypedList({length})))"
            ),
            Primitive::I8 => format!(
                "$$typed_data.Int8List.fromList({pointer}.cast<$$ffi.Int8>().asTypedList({length}))"
            ),
            Primitive::U8 => format!(
                "$$typed_data.Uint8List.fromList({pointer}.cast<$$ffi.Uint8>().asTypedList({length}))"
            ),
            Primitive::I16 => format!(
                "$$typed_data.Int16List.fromList({pointer}.cast<$$ffi.Int16>().asTypedList({length}))"
            ),
            Primitive::U16 => format!(
                "$$typed_data.Uint16List.fromList({pointer}.cast<$$ffi.Uint16>().asTypedList({length}))"
            ),
            Primitive::I32 => format!(
                "$$typed_data.Int32List.fromList({pointer}.cast<$$ffi.Int32>().asTypedList({length}))"
            ),
            Primitive::U32 => format!(
                "$$typed_data.Uint32List.fromList({pointer}.cast<$$ffi.Uint32>().asTypedList({length}))"
            ),
            Primitive::I64 => format!(
                "$$typed_data.Int64List.fromList({pointer}.cast<$$ffi.Int64>().asTypedList({length}))"
            ),
            Primitive::U64 => format!(
                "$$typed_data.Uint64List.fromList({pointer}.cast<$$ffi.Uint64>().asTypedList({length}))"
            ),
            Primitive::ISize => format!(
                "$$typed_data.Int64List.fromList(List<int>.generate({length}, (_l$index) => {pointer}.cast<$$ffi.IntPtr>().elementAt(_l$index).value))"
            ),
            Primitive::USize => format!(
                "$$typed_data.Uint64List.fromList(List<int>.generate({length}, (_l$index) => {pointer}.cast<$$ffi.UintPtr>().elementAt(_l$index).value))"
            ),
            Primitive::F32 => format!(
                "$$typed_data.Float32List.fromList({pointer}.cast<$$ffi.Float>().asTypedList({length}))"
            ),
            Primitive::F64 => format!(
                "$$typed_data.Float64List.fromList({pointer}.cast<$$ffi.Double>().asTypedList({length}))"
            ),
            _ => return super::super::unsupported("unknown direct-vector primitive"),
        };
        Ok(format!("({length}) == 0 ? {} : {copied}", self.empty()?))
    }

    fn empty(&self) -> Result<&'static str> {
        Ok(match self.primitive {
            Primitive::Bool => "$$BoltBoolList._m$fromUint8List($$typed_data.Uint8List(0))",
            Primitive::I8 => "$$typed_data.Int8List(0)",
            Primitive::U8 => "$$typed_data.Uint8List(0)",
            Primitive::I16 => "$$typed_data.Int16List(0)",
            Primitive::U16 => "$$typed_data.Uint16List(0)",
            Primitive::I32 => "$$typed_data.Int32List(0)",
            Primitive::U32 => "$$typed_data.Uint32List(0)",
            Primitive::I64 | Primitive::ISize => "$$typed_data.Int64List(0)",
            Primitive::U64 | Primitive::USize => "$$typed_data.Uint64List(0)",
            Primitive::F32 => "$$typed_data.Float32List(0)",
            Primitive::F64 => "$$typed_data.Float64List(0)",
            _ => return super::super::unsupported("unknown direct-vector primitive"),
        })
    }
}
