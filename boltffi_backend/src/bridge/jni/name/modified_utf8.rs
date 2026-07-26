use crate::bridge::c::Literal;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModifiedUtf8 {
    bytes: Vec<u8>,
}

impl ModifiedUtf8 {
    pub fn new(value: &str) -> Self {
        let bytes =
            value
                .encode_utf16()
                .fold(Vec::with_capacity(value.len()), |mut bytes, code_unit| {
                    match code_unit {
                        0 => bytes.extend([0xc0, 0x80]),
                        0x0001..=0x007f => bytes.push(code_unit as u8),
                        0x0080..=0x07ff => bytes.extend([
                            0xc0 | ((code_unit >> 6) as u8),
                            0x80 | ((code_unit & 0x3f) as u8),
                        ]),
                        _ => bytes.extend([
                            0xe0 | ((code_unit >> 12) as u8),
                            0x80 | (((code_unit >> 6) & 0x3f) as u8),
                            0x80 | ((code_unit & 0x3f) as u8),
                        ]),
                    }
                    bytes
                });
        Self { bytes }
    }

    pub fn literal(&self) -> Literal {
        Literal::byte_string(&self.bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::ModifiedUtf8;

    #[test]
    fn renders_jni_modified_utf8_as_unambiguous_c_bytes() {
        assert_eq!(
            ModifiedUtf8::new("A\0😀??/").literal().to_string(),
            "\"A\\300\\200\\355\\240\\275\\355\\270\\200\\077\\077/\""
        );
    }
}
