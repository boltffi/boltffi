#[data]
pub enum State {
    Idle,
    Busy { jobs: u32 },
}

#[data(impl)]
impl State {
    pub const INITIAL_BUSY: Self = State::Busy { jobs: 0 };
}
