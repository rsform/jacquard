use serde::{Deserialize, Serialize};
use std::str::

pub use cid::Cid as IpldCid;

/// raw
pub const ATP_CID_CODEC: u64 = 0x55;

/// SHA-256
pub const ATP_CID_HASH: u64 = 0x12;

/// base 32
pub const ATP_CID_BASE: multibase::Base = multibase::Base::Base32;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Cid {
    Cid(IpldCid),
    CidStr(CowStr<'static>),
}
