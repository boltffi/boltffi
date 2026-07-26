#[repr(u8)]
#[data]
pub enum Mode {
    Default = 1,
    Fast = 2,
}

#[data(impl)]
impl Mode {
    pub const DEFAULT: Self = Self::Default;
}
