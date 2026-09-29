pub struct Ledger {
    balance: i64,
}

#[export]
impl Ledger {
    pub fn new(balance: i64) -> Self {
        Self { balance }
    }

    pub fn balance(&self) -> i64 {
        self.balance
    }
}

pub struct Tally {
    count: u64,
}

#[export]
impl Tally {
    pub fn new(count: u64) -> Self {
        Self { count }
    }

    pub fn count(&self) -> u64 {
        self.count
    }
}
