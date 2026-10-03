#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UuidLiteral(u128);

impl UuidLiteral {
    pub fn parse(value: &str) -> Option<Self> {
        match value.len() {
            32 => {}
            36 if [8, 13, 18, 23]
                .into_iter()
                .all(|position| value.as_bytes()[position] == b'-') => {}
            _ => return None,
        }
        value
            .bytes()
            .enumerate()
            .filter(|(position, _)| value.len() != 36 || !matches!(position, 8 | 13 | 18 | 23))
            .try_fold(0u128, |bits, (_, digit)| {
                let digit = match digit {
                    b'0'..=b'9' => digit - b'0',
                    b'a'..=b'f' => digit - b'a' + 10,
                    b'A'..=b'F' => digit - b'A' + 10,
                    _ => return None,
                };
                Some((bits << 4) | u128::from(digit))
            })
            .map(Self)
    }

    pub fn high_bits(self) -> u64 {
        (self.0 >> 64) as u64
    }

    pub fn low_bits(self) -> u64 {
        self.0 as u64
    }
}
