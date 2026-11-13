use boltffi::{data, export};

#[data]
#[export]
#[derive(Clone, Copy)]
pub struct Both {
    pub x: u32,
}

#[export]
#[data]
#[derive(Clone, Copy)]
pub struct Either {
    pub x: u32,
}

#[data]
#[export]
#[derive(Clone, Copy)]
pub enum Mode {
    Fast,
}

#[export]
#[data]
#[derive(Clone, Copy)]
pub enum Pitch {
    Low,
}

#[boltffi::error]
#[export]
#[derive(Debug)]
pub enum Fault {
    Jammed,
}

#[export]
#[boltffi::error]
#[derive(Debug)]
pub enum Stall {
    Engine,
}
