boltffi::scaffolding!();

use std::net::IpAddr;

#[boltffi::data]
#[derive(Clone, Copy)]
pub struct Peer {
    pub address: IpAddr,
}

fn main() {}
