#[repr(C)]
#[data]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[data(impl)]
impl Color {
    pub const BLACK: Self = Self::rgba(0, 0, 0, 255);
    pub const CHANNEL_COUNT: u8 = 4;

    const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

#[repr(u8)]
#[data]
pub enum Mode {
    Fast = 1,
    Slow = 2,
}

#[data(impl)]
impl Mode {
    pub const DEFAULT: Self = Self::Fast;
}

#[data]
pub enum State {
    Idle,
    Busy { jobs: u32 },
}

#[data(impl)]
impl State {
    pub const INITIAL: Self = Self::Idle;
}

pub struct Palette;

#[export]
impl Palette {
    pub const MAX_COLORS: u8 = 16;

    pub fn new() -> Self {
        Self
    }
}

impl Palette {
    pub const UNEXPORTED_ASSOCIATED: u8 = 99;
}
