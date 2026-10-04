#[data]
pub struct UnsignedDefaults {
    #[default(255)]
    pub byte: u8,
    #[default(65535)]
    pub short: u16,
    #[default(4294967295)]
    pub word: u32,
    #[default(18446744073709551615)]
    pub wide: u64,
    #[default(1)]
    pub size: usize,
}
