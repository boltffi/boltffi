use crate::wire::{
    decode::{DecodeError, WireDecode},
    encode::WireEncode,
};

pub struct WireBuffer {
    data: Vec<u8>,
}

impl WireBuffer {
    pub fn new<T: WireEncode>(value: &T) -> Self {
        let size = value.wire_size();
        let mut data = vec![0u8; size];
        value.encode_to(&mut data);
        Self { data }
    }

    pub fn from_bytes(data: Vec<u8>) -> Self {
        Self { data }
    }

    pub fn decode<T: WireDecode>(&self) -> Result<T, DecodeError> {
        let (value, _) = T::decode_from(&self.data)?;
        Ok(value)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl AsRef<[u8]> for WireBuffer {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl From<WireBuffer> for Vec<u8> {
    fn from(buffer: WireBuffer) -> Self {
        buffer.data
    }
}

pub fn encode<T: WireEncode>(value: &T) -> Vec<u8> {
    WireBuffer::new(value).into_bytes()
}

pub fn decode<T: WireDecode>(data: &[u8]) -> Result<T, DecodeError> {
    let (value, _) = T::decode_from(data)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_primitive() {
        let buffer = WireBuffer::new(&42i32);
        assert_eq!(buffer.len(), 4);
        let decoded: i32 = buffer.decode().unwrap();
        assert_eq!(decoded, 42);
    }

    #[test]
    fn buffer_string() {
        let original = "hello world".to_string();
        let buffer = WireBuffer::new(&original);
        let decoded: String = buffer.decode().unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn buffer_vec() {
        let original = vec![1i32, 2, 3, 4, 5];
        let buffer = WireBuffer::new(&original);
        let decoded: Vec<i32> = buffer.decode().unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn encode_decode_helpers() {
        let original = vec!["hello".to_string(), "world".to_string()];
        let bytes = encode(&original);
        let decoded: Vec<String> = decode(&bytes).unwrap();
        assert_eq!(decoded, original);
    }
}
