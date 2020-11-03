#![recursion_limit = "256"]

#[macro_use]
extern crate derive_more;

mod apt_cache;
mod apt_get;
mod apt_mark;
mod dpkg;
mod upgrade;
mod utils;

pub mod apt;
pub mod fetch;
pub mod hash;
pub mod lock;
pub mod request;

pub use self::apt_cache::{AptCache, Policies, Policy};
pub use self::apt_get::AptGet;
pub use self::apt_mark::AptMark;
pub use self::dpkg::{Dpkg, DpkgQuery};
pub use self::upgrade::AptUpgradeEvent;
