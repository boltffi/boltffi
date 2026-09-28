pub mod remote {
    #[derive(Clone, Copy)]
    pub struct Stamp(pub i64);

    #[derive(Clone, Copy)]
    pub struct Tick(pub i64);
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
