#[data]
pub enum State {
    Idle,
    Busy { jobs: u32 },
}

#[export]
pub const IDLE: State = State::Idle;
