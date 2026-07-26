#[repr(u8)]
#[data]
pub enum SearchMode {
    Exact = 1,
    Prefix = 2,
}

#[data(impl)]
impl SearchMode {
    pub fn from_label(boltffi_result_value: &str) -> Option<Self> {
        match boltffi_result_value {
            "exact" => Some(Self::Exact),
            "prefix" => Some(Self::Prefix),
            _ => None,
        }
    }

    pub fn from_labels(primary: &str, fallback: &str) -> Option<Self> {
        match (primary, fallback) {
            ("exact", _) | (_, "exact") => Some(Self::Exact),
            ("prefix", _) | (_, "prefix") => Some(Self::Prefix),
            _ => None,
        }
    }
}
