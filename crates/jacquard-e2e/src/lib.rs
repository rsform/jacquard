//! Support crate for Jacquard's opt-in full-stack e2e harness.
//!
//! Heavy scenario targets live under `tests/` behind the `e2e` feature and
//! `required-features`, so ordinary `cargo nextest run` never executes them.
//! This module holds the provider-neutral fixture model shared by those
//! targets: deterministic fixture identities and run coordinates handed over
//! by the lifecycle controller through
//! environment variables.

#![forbid(unsafe_code)]

pub mod provider;

pub use provider::{Provider, ProviderContext};

#[cfg(feature = "e2e")]
pub mod bootstrap;

#[cfg(feature = "e2e")]
pub mod reference_bootstrap;

#[cfg(feature = "e2e")]
pub mod scenarios;

#[cfg(feature = "e2e")]
pub mod oauth;

#[cfg(feature = "e2e")]
pub mod spaces;

#[cfg(feature = "e2e")]
pub mod transport;

#[cfg(feature = "e2e")]
pub use transport::{AllowedHost, FixtureTransport, TransportAllowlist};
