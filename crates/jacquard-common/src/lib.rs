#[macro_use]
pub mod cowstr;
#[macro_use]
pub mod into_static;

pub mod types;

pub use cowstr::CowStr;
pub use into_static::IntoStatic;

pub use smol_str;
pub use url;
