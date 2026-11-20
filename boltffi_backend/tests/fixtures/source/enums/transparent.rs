#[data]
#[derive(Clone, Copy)]
pub struct Ping {
    pub sequence: u32,
}

#[data]
pub struct Note {
    pub body: String,
}

#[data]
pub enum Envelope {
    Unset,
    #[boltffi::transparent]
    Ping(Ping),
    #[boltffi::transparent]
    Note(Note),
    Raw(String),
}

#[data]
pub enum Reply {
    #[boltffi::transparent]
    Ping(Ping),
    Ack,
}

#[data(impl)]
impl Envelope {
    pub fn new(#[boltffi::default(7)] sequence: u32) -> Self {
        Self::Ping(Ping { sequence })
    }

    pub async fn load(#[boltffi::default(9)] sequence: u32) -> Self {
        Self::Ping(Ping { sequence })
    }

    pub fn default_step(#[boltffi::default(3)] step: u32) -> u32 {
        step
    }

    pub fn offset(&self, #[boltffi::default(3)] step: u32) -> u32 {
        match self {
            Self::Ping(ping) => ping.sequence + step,
            _ => step,
        }
    }

    pub async fn async_offset(&self, #[boltffi::default(5)] step: u32) -> u32 {
        self.offset(step)
    }
}

#[data(impl)]
impl Reply {
    pub fn reply_offset(&self, #[boltffi::default(11)] step: u32) -> u32 {
        match self {
            Self::Ping(ping) => ping.sequence + step,
            Self::Ack => step,
        }
    }
}

#[export]
pub fn echo_envelope(envelope: Envelope) -> Envelope {
    envelope
}

#[export]
pub fn echo_envelopes(envelopes: Vec<Envelope>) -> Vec<Envelope> {
    envelopes
}
