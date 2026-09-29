pub mod remote {
    #[derive(Clone, Copy)]
    pub struct Stamp(pub i64);

    #[derive(Clone, Copy)]
    pub struct Tick(pub i64);

    #[derive(Clone, Copy)]
    pub struct Deadline(pub registry::remote::Stamp);
}

use remote::Tick;

boltffi::custom_type!(
    pub Stamp,
    remote = remote::Stamp,
    repr = i64,
    into_ffi = |stamp: &remote::Stamp| stamp.0,
    try_from_ffi = |raw: i64| Ok(remote::Stamp(raw)),
);

boltffi::custom_type!(
    pub Tick,
    remote = Tick,
    repr = i64,
    into_ffi = |tick: &Tick| tick.0,
    try_from_ffi = |raw: i64| Ok(Tick(raw)),
);

#[boltffi::export]
pub fn later(stamp: Stamp, tick: Tick) -> Stamp {
    remote::Stamp(stamp.0 + tick.0)
}

#[boltffi::data]
#[derive(Clone)]
pub struct Stamped {
    pub at: registry::Stamp,
    pub history: Vec<registry::Stamp>,
}

boltffi::custom_type!(
    pub Deadline,
    remote = remote::Deadline,
    repr = registry::Stamp,
    into_ffi = |deadline: &remote::Deadline| deadline.0,
    try_from_ffi = |stamp: registry::Stamp| Ok(remote::Deadline(stamp)),
);

#[boltffi::export]
pub fn restamp(stamped: Stamped, window: registry::Window) -> Option<registry::Stamp> {
    stamped.history.last().copied().or(Some(window.0))
}

#[boltffi::export]
pub fn postpone(deadline: Deadline, window: registry::Window) -> Deadline {
    remote::Deadline(registry::remote::Stamp(deadline.0.0 + window.0.0))
}
