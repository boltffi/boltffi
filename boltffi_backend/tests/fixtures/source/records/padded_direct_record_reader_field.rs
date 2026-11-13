#[data]
pub struct Shadowing {
    pub reader: u8,
    pub value: u32,
}

#[export]
pub fn shadowing() -> Option<Shadowing> {
    None
}
