#[data]
pub enum State {
    Ready,
    Complete,
}

#[data]
pub struct Label {
    pub text: String,
}

#[data]
pub struct CollectionSizes {
    pub strings: Vec<String>,
    pub states: Vec<State>,
    pub nested: Vec<Vec<String>>,
    pub optional: Vec<Option<String>>,
    pub records: Vec<Label>,
    pub numbers: Vec<u32>,
}

#[data]
pub enum CollectionEvent {
    Int(Vec<String>),
}
