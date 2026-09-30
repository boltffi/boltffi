#[repr(u8)]
#[data]
pub enum Mode {
    Fast = 1,
    Slow = 2,
}

#[export]
pub trait Handler: Send + Sync {
    fn process(&self, value: u32) -> u32;
}

#[export]
pub fn greet(
    name: String,
    #[boltffi::default("world")] greeting: String,
    #[boltffi::default(3)] times: u32,
    #[boltffi::default(-1)] offset: i64,
    #[boltffi::default(true)] shout: bool,
    #[boltffi::default(0.5)] ratio: f32,
    #[boltffi::default(Mode::Slow)] mode: Mode,
    #[boltffi::default(None)] suffix: Option<String>,
    #[boltffi::default(7)] limit: Option<u16>,
) -> String {
    let _ = (greeting, times, offset, shout, ratio, mode, suffix, limit);
    name
}

pub struct Server {
    port: u16,
}

#[export]
impl Server {
    pub fn new(#[boltffi::default(8080)] port: u16) -> Self {
        Self { port }
    }

    pub async fn start(
        port: u16,
        #[boltffi::default(None)] first: Option<std::sync::Arc<dyn Handler>>,
        #[boltffi::default(None)] second: Option<std::sync::Arc<dyn Handler>>,
    ) -> Self {
        let _ = (first, second);
        Self { port }
    }

    pub fn port(&self, #[boltffi::default(false)] mapped: bool) -> u16 {
        let _ = mapped;
        self.port
    }
}

#[repr(u8)]
#[derive(Clone, Copy)]
#[data]
pub enum DefaultMode {
    Quiet = 1,
    Loud = 2,
}

#[data(impl)]
impl DefaultMode {
    pub fn with_mode(#[boltffi::default(DefaultMode::Loud)] mode: Self) -> Self {
        mode
    }

    pub fn new(#[boltffi::default(DefaultMode::Quiet)] mode: Self) -> Self {
        mode
    }

    pub async fn load(#[boltffi::default(DefaultMode::Quiet)] mode: Self) -> Self {
        mode
    }

    pub fn matches(&self, #[boltffi::default(DefaultMode::Quiet)] mode: Self) -> bool {
        *self as u8 == mode as u8
    }
}

#[export]
pub trait DefaultHandler: Send + Sync {
    fn apply(&self, value: i32) -> i32;
}

#[export]
pub fn format_defaults(
    name: String,
    #[boltffi::default("hello")] greeting: String,
    #[boltffi::default(true)] enabled: bool,
    #[boltffi::default(0.5)] ratio: f32,
    #[boltffi::default(DefaultMode::Quiet)] mode: DefaultMode,
    #[boltffi::default(1.5)] weight: Option<f64>,
    #[boltffi::default(None)] handler: Option<Box<dyn DefaultHandler>>,
) -> String {
    format!(
        "{greeting} {name} {enabled} {ratio} {} {weight:?} {}",
        mode as u8,
        handler.is_some()
    )
}

#[export]
pub async fn async_default(#[boltffi::default(9)] value: u32) -> u32 {
    value
}

pub struct DefaultCounter {
    start: i32,
}

#[export]
impl DefaultCounter {
    pub fn from_text(#[boltffi::default("40")] value: String) -> Self {
        Self {
            start: value.parse().unwrap(),
        }
    }

    pub fn new(#[boltffi::default(10)] start: i32) -> Self {
        Self { start }
    }

    pub fn with_offset(#[boltffi::default(20)] start: i32, offset: i32) -> Self {
        Self {
            start: start + offset,
        }
    }

    pub fn offset(&self, #[boltffi::default(1)] step: i32) -> i32 {
        self.start + step
    }

    pub fn sum(#[boltffi::default(1)] left: i32, #[boltffi::default(2)] right: i32) -> i32 {
        left + right
    }

    pub async fn async_offset(&self, #[boltffi::default(3)] step: i32) -> i32 {
        self.start + step
    }
}

#[data]
pub struct DefaultAmount {
    #[boltffi::default(3)]
    pub value: i32,
}

#[data(impl)]
impl DefaultAmount {
    pub fn with_scaled_value(#[boltffi::default(2)] value: i32) -> Self {
        Self { value: value * 2 }
    }

    pub fn try_scaled_value(#[boltffi::default(2)] value: i32) -> Option<Self> {
        (value >= 0).then_some(Self { value: value * 2 })
    }

    pub fn offset(&self, #[boltffi::default(2)] step: i32) -> i32 {
        self.value + step
    }
}

#[data]
pub struct NamedAmount {
    pub value: i32,
}

#[data(impl)]
impl NamedAmount {
    pub fn empty() -> Self {
        Self { value: 0 }
    }

    pub async fn load(#[boltffi::default(6)] value: i32) -> Self {
        Self { value }
    }

    pub fn with_value(#[boltffi::default(5)] value: i32) -> Self {
        Self { value }
    }
}

custom_type!(
    pub Amount,
    remote = AmountRust,
    repr = DefaultAmount,
    into_ffi = amount_into_ffi,
    try_from_ffi = amount_from_ffi
);

#[export]
pub fn default_amount(#[boltffi::default(5)] amount: AmountRust) -> AmountRust {
    amount
}

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

custom_type!(
    pub Limit,
    remote = LimitRust,
    repr = Option<u32>,
    into_ffi = limit_into_ffi,
    try_from_ffi = limit_from_ffi
);

#[data]
pub struct Quota {
    #[boltffi::default(None)]
    pub limit: LimitRust,
}

#[export]
pub fn throttle(#[boltffi::default(None)] limit: LimitRust) -> LimitRust {
    limit
}

#[data]
pub struct FloatDefaults {
    #[boltffi::default(-0.0)]
    pub single: f32,
    #[boltffi::default(-0.0)]
    pub double: f64,
}

#[export]
pub fn default_float_bits(#[boltffi::default(-0.0)] value: f32) -> u32 {
    value.to_bits()
}

#[export]
pub fn default_double_bits(#[boltffi::default(-0.0)] value: f64) -> u64 {
    value.to_bits()
}

custom_type!(
    pub Email,
    remote = EmailRust,
    repr = String,
    into_ffi = email_into_ffi,
    try_from_ffi = email_from_ffi
);

custom_type!(
    pub Identifier,
    remote = IdentifierRust,
    repr = String,
    into_ffi = identifier_into_ffi,
    try_from_ffi = identifier_from_ffi
);

#[data]
pub struct ContactDefaults {
    #[boltffi::default("mailto:ada@example.com")]
    pub email: EmailRust,
    #[boltffi::default("01234567-89ab-cdef-0123-456789abcdef")]
    pub identifier: IdentifierRust,
    #[boltffi::default(None)]
    pub optional_email: Option<EmailRust>,
}

#[export]
pub fn default_email(
    #[boltffi::default("mailto:ada@example.com")] email: EmailRust,
) -> EmailRust {
    email
}

#[export]
pub fn default_identifier(
    #[boltffi::default("01234567-89ab-cdef-0123-456789abcdef")] identifier: IdentifierRust,
) -> IdentifierRust {
    identifier
}

#[export]
pub fn default_optional_email(
    #[boltffi::default("mailto:ada@example.com")] email: Option<EmailRust>,
) -> Option<EmailRust> {
    email
}
