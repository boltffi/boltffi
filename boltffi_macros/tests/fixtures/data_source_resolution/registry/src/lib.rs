boltffi::scaffolding!();

pub mod remote {
    #[derive(Clone, Copy)]
    pub struct Stamp(pub i64);

    #[derive(Clone, Copy)]
    pub struct Window(pub Stamp);
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
