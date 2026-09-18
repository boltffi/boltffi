use pim_macros::{data, define_record};

macro_rules! make_data {
    ($name:ident, $field:ident) => {
        #[data]
        pub struct $name {
            pub $field: crate::geometry::Point,
        }
    };
}

make_data!(MacroEmitted, anchor);

define_record!(ProcEmitted {
    pub id: u64,
    pub anchor: crate::physics::Point,
});
