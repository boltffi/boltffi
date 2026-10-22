use boltffi::export;

pub struct Email(pub String);

boltffi::custom_type!(
    pub Email,
    remote = Email,
    repr = String,
    into_ffi = |email: &Email| email.0.clone(),
    try_from_ffi = |text: String| Ok::<_, boltffi::CustomTypeConversionError>(Email(text)),
);

#[export]
impl Email {
    pub fn length(&self) -> u32 {
        self.0.len() as u32
    }
}
