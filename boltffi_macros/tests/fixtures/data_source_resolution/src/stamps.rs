pub mod remote {
    #[derive(Clone, Copy)]
    pub struct Stamp(pub i64);

    #[derive(Clone, Copy)]
    pub struct Tick(pub i64);

    #[derive(Clone, Copy)]
    pub struct Deadline(pub registry::remote::Stamp);

    #[derive(Clone)]
    pub struct Streak(pub Vec<Stamp>);

    #[derive(Clone, Copy)]
    pub struct Lapse(pub Option<registry::remote::Stamp>);

    #[derive(Clone)]
    pub struct Ledger(pub std::collections::HashMap<String, registry::remote::Window>);
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

boltffi::custom_type!(
    pub Streak,
    remote = remote::Streak,
    repr = Vec<Stamp>,
    into_ffi = |streak: &remote::Streak| streak.0.clone(),
    try_from_ffi = |stamps: Vec<Stamp>| Ok(remote::Streak(stamps)),
);

boltffi::custom_type!(
    pub Lapse,
    remote = remote::Lapse,
    repr = Option<registry::Stamp>,
    into_ffi = |lapse: &remote::Lapse| lapse.0,
    try_from_ffi = |stamp: Option<registry::Stamp>| Ok(remote::Lapse(stamp)),
);

boltffi::custom_type!(
    pub Ledger,
    remote = remote::Ledger,
    repr = std::collections::HashMap<String, registry::Window>,
    into_ffi = |ledger: &remote::Ledger| ledger.0.clone(),
    try_from_ffi = |windows: std::collections::HashMap<String, registry::Window>| {
        Ok(remote::Ledger(windows))
    },
);

#[boltffi::export]
pub fn retrace(streak: Streak, lapse: Lapse, ledger: Ledger, timeline: registry::Timeline) -> Streak {
    let mut stamps = streak.0;
    stamps.extend(lapse.0.map(|stamp| remote::Stamp(stamp.0)));
    stamps.extend(ledger.0.values().map(|window| remote::Stamp(window.0.0)));
    stamps.extend(timeline.0.iter().map(|stamp| remote::Stamp(stamp.0)));
    remote::Streak(stamps)
}

pub mod labels {
    #[derive(Clone)]
    pub struct Label(pub String);

    #[boltffi::custom_ffi]
    impl boltffi::CustomFfiConvertible for Label {
        type FfiRepr = String;
        type Error = boltffi::CustomTypeConversionError;

        fn into_ffi(&self) -> String {
            self.0.clone()
        }

        fn try_from_ffi(repr: String) -> Result<Self, boltffi::CustomTypeConversionError> {
            Ok(Self(repr))
        }
    }
}

#[derive(Clone)]
pub struct Shelf(pub Vec<labels::Label>);

boltffi::custom_type!(
    pub Labels,
    remote = Shelf,
    repr = Vec<labels::Label>,
    into_ffi = |shelf: &Shelf| shelf.0.clone(),
    try_from_ffi = |labels: Vec<labels::Label>| Ok(Shelf(labels)),
);

#[boltffi::export]
pub fn relabel(labels: Labels) -> Labels {
    labels
}
