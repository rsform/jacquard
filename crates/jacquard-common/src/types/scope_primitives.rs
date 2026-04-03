//! Scope action and resource enums for AT Protocol OAuth.
//!
//! These types are used in both OAuth scope parsing and permission set
//! lexicon definitions, allowing consistent validation and serialization.

use serde::{Deserialize, Serialize};

/// Account resource types for AT Protocol OAuth scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccountResource {
    /// Email access.
    Email,
    /// Repository access.
    Repo,
    /// Status access.
    Status,
}

/// Account action permissions for AT Protocol OAuth scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccountAction {
    /// Read-only access.
    Read,
    /// Management access (includes read).
    Manage,
}

/// Repository action permissions for AT Protocol OAuth scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RepoAction {
    /// Create records.
    Create,
    /// Update records.
    Update,
    /// Delete records.
    Delete,
}
