#[repr(u8)]
#[derive(Clone, Copy)]
#[data]
pub enum DefaultMode {
    Quiet = 1,
    Loud = 2,
}

#[data(impl)]
impl DefaultMode {
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
