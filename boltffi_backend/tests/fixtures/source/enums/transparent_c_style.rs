#[repr(i32)]
#[data]
pub enum Mode {
    Fast = 0,
    Slow = 1,
}

#[data]
pub struct Ping {
    pub sequence: u32,
}

#[data]
pub enum Setting {
    Unset,
    #[boltffi::transparent]
    Mode(Mode),
    #[boltffi::transparent]
    Ping(Ping),
    Raw(Mode),
}

#[data]
pub enum Reply {
    #[boltffi::transparent]
    Mode(Mode),
    Ack,
}

#[export]
pub fn echo_setting(setting: Setting) -> Setting {
    setting
}
