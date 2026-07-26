#[export]
pub const PAIR: (u32, u32) = (3, 5);

#[export]
pub const TRIPLE: (u32, u32, u32) = (3, 5, 8);

#[export]
pub fn echo_pair(value: (u32, String)) -> (u32, String) {
    value
}

#[export]
pub fn echo_triple(value: (u32, String, bool)) -> (u32, String, bool) {
    value
}
