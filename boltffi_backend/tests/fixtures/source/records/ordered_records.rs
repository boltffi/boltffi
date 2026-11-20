#[data(ord)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rank {
    pub tier: u32,
    pub score: i32,
}

#[data(ord)]
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub pre: Option<String>,
}
