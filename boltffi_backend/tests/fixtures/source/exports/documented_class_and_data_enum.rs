/// A counter owned by Rust.
#[export]
pub struct Counter {
    value: i64,
}

#[export]
impl Counter {
    /// Creates a counter starting at `start`.
    pub fn new(start: i64) -> Self {
        Self { value: start }
    }

    /// Returns a counter starting at zero.
    pub fn zeroed() -> Self {
        Self { value: 0 }
    }

    /// Returns the current value.
    pub fn value(&self) -> i64 {
        self.value
    }
}

/// How a shape is labelled.
#[data]
pub enum Outline {
    /// A shape the caller named.
    Named {
        /// The name shown to the user.
        label: String,
    },
    /// A shape with no name.
    Anonymous,
}

/// A profile carried by value.
#[data]
pub struct Profile {
    /// The display name.
    pub name: String,
    /// Whether the profile is active.
    pub active: bool,
}

/// Returns the profile unchanged.
#[export]
pub fn echo_profile(profile: Profile) -> Profile {
    profile
}

/// Returns the outline unchanged.
#[export]
pub fn echo_outline(outline: Outline) -> Outline {
    outline
}

/// The wire protocol revision.
#[export]
pub const PROTOCOL_REVISION: u32 = 3;
