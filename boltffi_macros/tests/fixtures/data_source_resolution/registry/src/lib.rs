boltffi::scaffolding!();

pub mod remote {
    #[derive(Clone, Copy)]
    pub struct Stamp(pub i64);

    #[derive(Clone, Copy)]
    pub struct Window(pub Stamp);

    #[derive(Clone)]
    pub struct Timeline(pub Vec<Stamp>);
}

boltffi::custom_type!(
    pub Stamp,
    remote = remote::Stamp,
    repr = i64,
    into_ffi = |stamp: &remote::Stamp| stamp.0,
    try_from_ffi = |raw: i64| Ok(remote::Stamp(raw)),
);

boltffi::custom_type!(
    pub Window,
    remote = remote::Window,
    repr = Stamp,
    into_ffi = |window: &remote::Window| window.0,
    try_from_ffi = |stamp: Stamp| Ok(remote::Window(stamp)),
);

boltffi::custom_type!(
    pub Timeline,
    remote = remote::Timeline,
    repr = Vec<Stamp>,
    into_ffi = |timeline: &remote::Timeline| timeline.0.clone(),
    try_from_ffi = |stamps: Vec<Stamp>| Ok(remote::Timeline(stamps)),
);
