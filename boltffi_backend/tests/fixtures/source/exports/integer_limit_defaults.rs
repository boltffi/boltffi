#[export]
pub const FLOOR: i64 = -9223372036854775808;

#[export]
pub const CEILING: u64 = 18446744073709551615;

#[data]
pub struct Range {
    #[boltffi::default(-9223372036854775808)]
    pub start: i64,
    #[boltffi::default(18446744073709551615)]
    pub end: u64,
}

#[export]
pub fn span(
    #[boltffi::default(-9223372036854775808)] start: i64,
    #[boltffi::default(18446744073709551615)] end: u64,
) -> u64 {
    end.wrapping_add_signed(start)
}
