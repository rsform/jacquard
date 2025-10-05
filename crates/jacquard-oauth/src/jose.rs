pub mod jws;
pub mod jwt;
pub mod signing;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Header<'a> {
    #[serde(borrow)]
    Jws(jws::Header<'a>),
}

pub use self::signing::create_signed_jwt;
