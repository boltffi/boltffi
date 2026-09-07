#[data]
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

#[export]
pub fn echo_envelope(envelope: Envelope) -> Envelope {
    envelope
}

#[export]
pub fn echo_envelopes(envelopes: Vec<Envelope>) -> Vec<Envelope> {
    envelopes
}
