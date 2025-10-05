//! AT Protocol OAuth scopes module
//! Derived from https://tangled.org/@smokesignal.events/atproto-identity-rs/raw/main/crates/atproto-oauth/src/scopes.rs
//!
//! This module provides comprehensive support for AT Protocol OAuth scopes,
//! including parsing, serialization, normalization, and permission checking.
//!
//! Scopes in AT Protocol follow a prefix-based format with optional query parameters:
//! - `account`: Access to account information (email, repo, status)
//! - `identity`: Access to identity information (handle)
//! - `blob`: Access to blob operations with mime type constraints
//! - `repo`: Repository operations with collection and action constraints
//! - `rpc`: RPC method access with lexicon and audience constraints
//! - `atproto`: Required scope to indicate that other AT Protocol scopes will be used
//! - `transition`: Migration operations (generic or email)
//!
//! Standard OpenID Connect scopes (no suffixes or query parameters):
//! - `openid`: Required for OpenID Connect authentication
//! - `profile`: Access to user profile information
//! - `email`: Access to user email address

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use jacquard_common::types::did::Did;
use jacquard_common::types::nsid::Nsid;
use jacquard_common::types::string::AtStrError;
use jacquard_common::{CowStr, IntoStatic};
use smol_str::{SmolStr, ToSmolStr};

/// Represents an AT Protocol OAuth scope
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Scope<'s> {
    /// Account scope for accessing account information
    Account(AccountScope),
    /// Identity scope for accessing identity information
    Identity(IdentityScope),
    /// Blob scope for blob operations with mime type constraints
    Blob(BlobScope<'s>),
    /// Repository scope for collection operations
    Repo(RepoScope<'s>),
    /// RPC scope for method access
    Rpc(RpcScope<'s>),
    /// AT Protocol scope - required to indicate that other AT Protocol scopes will be used
    Atproto,
    /// Transition scope for migration operations
    Transition(TransitionScope),
    /// OpenID Connect scope - required for OpenID Connect authentication
    OpenId,
    /// Profile scope - access to user profile information
    Profile,
    /// Email scope - access to user email address
    Email,
}

impl IntoStatic for Scope<'_> {
    type Output = Scope<'static>;

    fn into_static(self) -> Self::Output {
        match self {
            Scope::Account(scope) => Scope::Account(scope),
            Scope::Identity(scope) => Scope::Identity(scope),
            Scope::Blob(scope) => Scope::Blob(scope.into_static()),
            Scope::Repo(scope) => Scope::Repo(scope.into_static()),
            Scope::Rpc(scope) => Scope::Rpc(scope.into_static()),
            Scope::Atproto => Scope::Atproto,
            Scope::Transition(scope) => Scope::Transition(scope),
            Scope::OpenId => Scope::OpenId,
            Scope::Profile => Scope::Profile,
            Scope::Email => Scope::Email,
        }
    }
}

/// Account scope attributes
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountScope {
    /// The account resource type
    pub resource: AccountResource,
    /// The action permission level
    pub action: AccountAction,
}

/// Account resource types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccountResource {
    /// Email access
    Email,
    /// Repository access
    Repo,
    /// Status access
    Status,
}

/// Account action permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccountAction {
    /// Read-only access
    Read,
    /// Management access (includes read)
    Manage,
}

/// Identity scope attributes
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IdentityScope {
    /// Handle access
    Handle,
    /// All identity access (wildcard)
    All,
}

/// Transition scope types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TransitionScope {
    /// Generic transition operations
    Generic,
    /// Email transition operations
    Email,
}

/// Blob scope with mime type constraints
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlobScope<'s> {
    /// Accepted mime types
    pub accept: BTreeSet<MimePattern<'s>>,
}

impl IntoStatic for BlobScope<'_> {
    type Output = BlobScope<'static>;

    fn into_static(self) -> Self::Output {
        BlobScope {
            accept: self.accept.into_iter().map(|p| p.into_static()).collect(),
        }
    }
}

/// MIME type pattern for blob scope
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MimePattern<'s> {
    /// Match all types
    All,
    /// Match all subtypes of a type (e.g., "image/*")
    TypeWildcard(CowStr<'s>),
    /// Exact mime type match
    Exact(CowStr<'s>),
}

impl IntoStatic for MimePattern<'_> {
    type Output = MimePattern<'static>;

    fn into_static(self) -> Self::Output {
        match self {
            MimePattern::All => MimePattern::All,
            MimePattern::TypeWildcard(s) => MimePattern::TypeWildcard(s.into_static()),
            MimePattern::Exact(s) => MimePattern::Exact(s.into_static()),
        }
    }
}

/// Repository scope with collection and action constraints
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepoScope<'s> {
    /// Collection NSID or wildcard
    pub collection: RepoCollection<'s>,
    /// Allowed actions
    pub actions: BTreeSet<RepoAction>,
}

impl IntoStatic for RepoScope<'_> {
    type Output = RepoScope<'static>;

    fn into_static(self) -> Self::Output {
        RepoScope {
            collection: self.collection.into_static(),
            actions: self.actions,
        }
    }
}

/// Repository collection identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RepoCollection<'s> {
    /// All collections (wildcard)
    All,
    /// Specific collection NSID
    Nsid(Nsid<'s>),
}

impl IntoStatic for RepoCollection<'_> {
    type Output = RepoCollection<'static>;

    fn into_static(self) -> Self::Output {
        match self {
            RepoCollection::All => RepoCollection::All,
            RepoCollection::Nsid(nsid) => RepoCollection::Nsid(nsid.into_static()),
        }
    }
}

/// Repository actions
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RepoAction {
    /// Create records
    Create,
    /// Update records
    Update,
    /// Delete records
    Delete,
}

/// RPC scope with lexicon method and audience constraints
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RpcScope<'s> {
    /// Lexicon methods (NSIDs or wildcard)
    pub lxm: BTreeSet<RpcLexicon<'s>>,
    /// Audiences (DIDs or wildcard)
    pub aud: BTreeSet<RpcAudience<'s>>,
}

impl IntoStatic for RpcScope<'_> {
    type Output = RpcScope<'static>;

    fn into_static(self) -> Self::Output {
        RpcScope {
            lxm: self.lxm.into_iter().map(|s| s.into_static()).collect(),
            aud: self.aud.into_iter().map(|s| s.into_static()).collect(),
        }
    }
}

/// RPC lexicon identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RpcLexicon<'s> {
    /// All lexicons (wildcard)
    All,
    /// Specific lexicon NSID
    Nsid(Nsid<'s>),
}

impl IntoStatic for RpcLexicon<'_> {
    type Output = RpcLexicon<'static>;

    fn into_static(self) -> Self::Output {
        match self {
            RpcLexicon::All => RpcLexicon::All,
            RpcLexicon::Nsid(nsid) => RpcLexicon::Nsid(nsid.into_static()),
        }
    }
}

/// RPC audience identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RpcAudience<'s> {
    /// All audiences (wildcard)
    All,
    /// Specific DID
    Did(Did<'s>),
}

impl IntoStatic for RpcAudience<'_> {
    type Output = RpcAudience<'static>;

    fn into_static(self) -> Self::Output {
        match self {
            RpcAudience::All => RpcAudience::All,
            RpcAudience::Did(did) => RpcAudience::Did(did.into_static()),
        }
    }
}

impl<'s> Scope<'s> {
    /// Parse multiple space-separated scopes from a string
    ///
    /// # Examples
    /// ```
    /// # use jacquard_oauth::scopes::Scope;
    /// let scopes = Scope::parse_multiple("atproto repo:*").unwrap();
    /// assert_eq!(scopes.len(), 2);
    /// ```
    pub fn parse_multiple(s: &'s str) -> Result<Vec<Self>, ParseError> {
        if s.trim().is_empty() {
            return Ok(Vec::new());
        }

        let mut scopes = Vec::new();
        for scope_str in s.split_whitespace() {
            scopes.push(Self::parse(scope_str)?);
        }

        Ok(scopes)
    }

    /// Parse multiple space-separated scopes and return the minimal set needed
    ///
    /// This method removes duplicate scopes and scopes that are already granted
    /// by other scopes in the list, returning only the minimal set of scopes needed.
    ///
    /// # Examples
    /// ```
    /// # use jacquard_oauth::scopes::Scope;
    /// // repo:* grants repo:foo.bar, so only repo:* is kept
    /// let scopes = Scope::parse_multiple_reduced("atproto repo:app.bsky.feed.post repo:*").unwrap();
    /// assert_eq!(scopes.len(), 2); // atproto and repo:*
    /// ```
    pub fn parse_multiple_reduced(s: &'s str) -> Result<Vec<Self>, ParseError> {
        let all_scopes = Self::parse_multiple(s)?;

        if all_scopes.is_empty() {
            return Ok(Vec::new());
        }

        let mut result: Vec<Self> = Vec::new();

        for scope in all_scopes {
            // Check if this scope is already granted by something in the result
            let mut is_granted = false;
            for existing in &result {
                if existing.grants(&scope) && existing != &scope {
                    is_granted = true;
                    break;
                }
            }

            if is_granted {
                continue; // Skip this scope, it's already covered
            }

            // Check if this scope grants any existing scopes in the result
            let mut indices_to_remove = Vec::new();
            for (i, existing) in result.iter().enumerate() {
                if scope.grants(existing) && &scope != existing {
                    indices_to_remove.push(i);
                }
            }

            // Remove scopes that are granted by the new scope (in reverse order to maintain indices)
            for i in indices_to_remove.into_iter().rev() {
                result.remove(i);
            }

            // Add the new scope if it's not a duplicate
            if !result.contains(&scope) {
                result.push(scope);
            }
        }

        Ok(result)
    }

    /// Serialize a list of scopes into a space-separated OAuth scopes string
    ///
    /// The scopes are sorted alphabetically by their string representation to ensure
    /// consistent output regardless of input order.
    ///
    /// # Examples
    /// ```
    /// # use jacquard_oauth::scopes::Scope;
    /// let scopes = vec![
    ///     Scope::parse("repo:*").unwrap(),
    ///     Scope::parse("atproto").unwrap(),
    ///     Scope::parse("account:email").unwrap(),
    /// ];
    /// let result = Scope::serialize_multiple(&scopes);
    /// assert_eq!(result, "account:email atproto repo:*");
    /// ```
    pub fn serialize_multiple(scopes: &[Self]) -> CowStr<'static> {
        if scopes.is_empty() {
            return CowStr::default();
        }

        let mut serialized: Vec<String> = scopes.iter().map(|scope| scope.to_string()).collect();

        serialized.sort();
        serialized.join(" ").into()
    }

    /// Remove a scope from a list of scopes
    ///
    /// Returns a new vector with all instances of the specified scope removed.
    /// If the scope doesn't exist in the list, returns a copy of the original list.
    ///
    /// # Examples
    /// ```
    /// # use jacquard_oauth::scopes::Scope;
    /// let scopes = vec![
    ///     Scope::parse("repo:*").unwrap(),
    ///     Scope::parse("atproto").unwrap(),
    ///     Scope::parse("account:email").unwrap(),
    /// ];
    /// let to_remove = Scope::parse("atproto").unwrap();
    /// let result = Scope::remove_scope(&scopes, &to_remove);
    /// assert_eq!(result.len(), 2);
    /// assert!(!result.contains(&to_remove));
    /// ```
    pub fn remove_scope(scopes: &[Self], scope_to_remove: &Self) -> Vec<Self> {
        scopes
            .iter()
            .filter(|s| *s != scope_to_remove)
            .cloned()
            .collect()
    }

    /// Parse a scope from a string
    pub fn parse(s: &'s str) -> Result<Self, ParseError> {
        // Determine the prefix first by checking for known prefixes
        let prefixes = [
            "account",
            "identity",
            "blob",
            "repo",
            "rpc",
            "atproto",
            "transition",
            "openid",
            "profile",
            "email",
        ];
        let mut found_prefix = None;
        let mut suffix = None;

        for prefix in &prefixes {
            if let Some(remainder) = s.strip_prefix(prefix)
                && (remainder.is_empty()
                    || remainder.starts_with(':')
                    || remainder.starts_with('?'))
            {
                found_prefix = Some(*prefix);
                if let Some(stripped) = remainder.strip_prefix(':') {
                    suffix = Some(stripped);
                } else if remainder.starts_with('?') {
                    suffix = Some(remainder);
                } else {
                    suffix = None;
                }
                break;
            }
        }

        let prefix = found_prefix.ok_or_else(|| {
            // If no known prefix found, extract what looks like a prefix for error reporting
            let end = s.find(':').or_else(|| s.find('?')).unwrap_or(s.len());
            ParseError::UnknownPrefix(s[..end].to_string())
        })?;

        match prefix {
            "account" => Self::parse_account(suffix),
            "identity" => Self::parse_identity(suffix),
            "blob" => Self::parse_blob(suffix),
            "repo" => Self::parse_repo(suffix),
            "rpc" => Self::parse_rpc(suffix),
            "atproto" => Self::parse_atproto(suffix),
            "transition" => Self::parse_transition(suffix),
            "openid" => Self::parse_openid(suffix),
            "profile" => Self::parse_profile(suffix),
            "email" => Self::parse_email(suffix),
            _ => Err(ParseError::UnknownPrefix(prefix.to_string())),
        }
    }

    fn parse_account(suffix: Option<&'s str>) -> Result<Self, ParseError> {
        let (resource_str, params) = match suffix {
            Some(s) => {
                if let Some(pos) = s.find('?') {
                    (&s[..pos], Some(&s[pos + 1..]))
                } else {
                    (s, None)
                }
            }
            None => return Err(ParseError::MissingResource),
        };

        let resource = match resource_str {
            "email" => AccountResource::Email,
            "repo" => AccountResource::Repo,
            "status" => AccountResource::Status,
            _ => return Err(ParseError::InvalidResource(resource_str.to_string())),
        };

        let action = if let Some(params) = params {
            let parsed_params = parse_query_string(params);
            match parsed_params
                .get("action")
                .and_then(|v| v.first())
                .map(|s| s.as_ref())
            {
                Some("read") => AccountAction::Read,
                Some("manage") => AccountAction::Manage,
                Some(other) => return Err(ParseError::InvalidAction(other.to_string())),
                None => AccountAction::Read,
            }
        } else {
            AccountAction::Read
        };

        Ok(Scope::Account(AccountScope { resource, action }))
    }

    fn parse_identity(suffix: Option<&'s str>) -> Result<Self, ParseError> {
        let scope = match suffix {
            Some("handle") => IdentityScope::Handle,
            Some("*") => IdentityScope::All,
            Some(other) => return Err(ParseError::InvalidResource(other.to_string())),
            None => return Err(ParseError::MissingResource),
        };

        Ok(Scope::Identity(scope))
    }

    fn parse_blob(suffix: Option<&'s str>) -> Result<Self, ParseError> {
        let mut accept = BTreeSet::new();

        match suffix {
            Some(s) if s.starts_with('?') => {
                let params = parse_query_string(&s[1..]);
                if let Some(values) = params.get("accept") {
                    for value in values {
                        accept.insert(MimePattern::from_str(value)?);
                    }
                }
            }
            Some(s) => {
                accept.insert(MimePattern::from_str(s)?);
            }
            None => {
                accept.insert(MimePattern::All);
            }
        }

        if accept.is_empty() {
            accept.insert(MimePattern::All);
        }

        Ok(Scope::Blob(BlobScope { accept }))
    }

    fn parse_repo(suffix: Option<&'s str>) -> Result<Self, ParseError> {
        let (collection_str, params) = match suffix {
            Some(s) => {
                if let Some(pos) = s.find('?') {
                    (Some(&s[..pos]), Some(&s[pos + 1..]))
                } else {
                    (Some(s), None)
                }
            }
            None => (None, None),
        };

        let collection = match collection_str {
            Some("*") | None => RepoCollection::All,
            Some(nsid) => RepoCollection::Nsid(Nsid::new(nsid)?),
        };

        let mut actions = BTreeSet::new();
        if let Some(params) = params {
            let parsed_params = parse_query_string(params);
            if let Some(values) = parsed_params.get("action") {
                for value in values {
                    match value.as_ref() {
                        "create" => {
                            actions.insert(RepoAction::Create);
                        }
                        "update" => {
                            actions.insert(RepoAction::Update);
                        }
                        "delete" => {
                            actions.insert(RepoAction::Delete);
                        }
                        "*" => {
                            actions.insert(RepoAction::Create);
                            actions.insert(RepoAction::Update);
                            actions.insert(RepoAction::Delete);
                        }
                        other => return Err(ParseError::InvalidAction(other.to_string())),
                    }
                }
            }
        }

        if actions.is_empty() {
            actions.insert(RepoAction::Create);
            actions.insert(RepoAction::Update);
            actions.insert(RepoAction::Delete);
        }

        Ok(Scope::Repo(RepoScope {
            collection,
            actions,
        }))
    }

    fn parse_rpc(suffix: Option<&'s str>) -> Result<Self, ParseError> {
        let mut lxm = BTreeSet::new();
        let mut aud = BTreeSet::new();

        match suffix {
            Some("*") => {
                lxm.insert(RpcLexicon::All);
                aud.insert(RpcAudience::All);
            }
            Some(s) if s.starts_with('?') => {
                let params = parse_query_string(&s[1..]);

                if let Some(values) = params.get("lxm") {
                    for value in values {
                        if value.as_ref() == "*" {
                            lxm.insert(RpcLexicon::All);
                        } else {
                            lxm.insert(RpcLexicon::Nsid(Nsid::new(value)?.into_static()));
                        }
                    }
                }

                if let Some(values) = params.get("aud") {
                    for value in values {
                        if value.as_ref() == "*" {
                            aud.insert(RpcAudience::All);
                        } else {
                            aud.insert(RpcAudience::Did(Did::new(value)?.into_static()));
                        }
                    }
                }
            }
            Some(s) => {
                // Check if there's a query string in the suffix
                if let Some(pos) = s.find('?') {
                    let nsid = &s[..pos];
                    let params = parse_query_string(&s[pos + 1..]);

                    lxm.insert(RpcLexicon::Nsid(Nsid::new(nsid)?.into_static()));

                    if let Some(values) = params.get("aud") {
                        for value in values {
                            if value.as_ref() == "*" {
                                aud.insert(RpcAudience::All);
                            } else {
                                aud.insert(RpcAudience::Did(Did::new(value)?.into_static()));
                            }
                        }
                    }
                } else {
                    lxm.insert(RpcLexicon::Nsid(Nsid::new(s)?.into_static()));
                }
            }
            None => {}
        }

        if lxm.is_empty() {
            lxm.insert(RpcLexicon::All);
        }
        if aud.is_empty() {
            aud.insert(RpcAudience::All);
        }

        Ok(Scope::Rpc(RpcScope { lxm, aud }))
    }

    fn parse_atproto(suffix: Option<&str>) -> Result<Self, ParseError> {
        if suffix.is_some() {
            return Err(ParseError::InvalidResource(
                "atproto scope does not accept suffixes".to_string(),
            ));
        }
        Ok(Scope::Atproto)
    }

    fn parse_transition(suffix: Option<&str>) -> Result<Self, ParseError> {
        let scope = match suffix {
            Some("generic") => TransitionScope::Generic,
            Some("email") => TransitionScope::Email,
            Some(other) => return Err(ParseError::InvalidResource(other.to_string())),
            None => return Err(ParseError::MissingResource),
        };

        Ok(Scope::Transition(scope))
    }

    fn parse_openid(suffix: Option<&str>) -> Result<Self, ParseError> {
        if suffix.is_some() {
            return Err(ParseError::InvalidResource(
                "openid scope does not accept suffixes".to_string(),
            ));
        }
        Ok(Scope::OpenId)
    }

    fn parse_profile(suffix: Option<&str>) -> Result<Self, ParseError> {
        if suffix.is_some() {
            return Err(ParseError::InvalidResource(
                "profile scope does not accept suffixes".to_string(),
            ));
        }
        Ok(Scope::Profile)
    }

    fn parse_email(suffix: Option<&str>) -> Result<Self, ParseError> {
        if suffix.is_some() {
            return Err(ParseError::InvalidResource(
                "email scope does not accept suffixes".to_string(),
            ));
        }
        Ok(Scope::Email)
    }

    /// Convert the scope to its normalized string representation
    pub fn to_string_normalized(&self) -> String {
        match self {
            Scope::Account(scope) => {
                let resource = match scope.resource {
                    AccountResource::Email => "email",
                    AccountResource::Repo => "repo",
                    AccountResource::Status => "status",
                };

                match scope.action {
                    AccountAction::Read => format!("account:{}", resource),
                    AccountAction::Manage => format!("account:{}?action=manage", resource),
                }
            }
            Scope::Identity(scope) => match scope {
                IdentityScope::Handle => "identity:handle".to_string(),
                IdentityScope::All => "identity:*".to_string(),
            },
            Scope::Blob(scope) => {
                if scope.accept.len() == 1 {
                    if let Some(pattern) = scope.accept.iter().next() {
                        match pattern {
                            MimePattern::All => "blob:*/*".to_string(),
                            MimePattern::TypeWildcard(t) => format!("blob:{}/*", t),
                            MimePattern::Exact(mime) => format!("blob:{}", mime),
                        }
                    } else {
                        "blob:*/*".to_string()
                    }
                } else {
                    let mut params = Vec::new();
                    for pattern in &scope.accept {
                        match pattern {
                            MimePattern::All => params.push("accept=*/*".to_string()),
                            MimePattern::TypeWildcard(t) => params.push(format!("accept={}/*", t)),
                            MimePattern::Exact(mime) => params.push(format!("accept={}", mime)),
                        }
                    }
                    params.sort();
                    format!("blob?{}", params.join("&"))
                }
            }
            Scope::Repo(scope) => {
                let collection = match &scope.collection {
                    RepoCollection::All => "*",
                    RepoCollection::Nsid(nsid) => nsid,
                };

                if scope.actions.len() == 3 {
                    format!("repo:{}", collection)
                } else {
                    let mut params = Vec::new();
                    for action in &scope.actions {
                        match action {
                            RepoAction::Create => params.push("action=create"),
                            RepoAction::Update => params.push("action=update"),
                            RepoAction::Delete => params.push("action=delete"),
                        }
                    }
                    format!("repo:{}?{}", collection, params.join("&"))
                }
            }
            Scope::Rpc(scope) => {
                if scope.lxm.len() == 1
                    && scope.lxm.contains(&RpcLexicon::All)
                    && scope.aud.len() == 1
                    && scope.aud.contains(&RpcAudience::All)
                {
                    "rpc:*".to_string()
                } else if scope.lxm.len() == 1
                    && scope.aud.len() == 1
                    && scope.aud.contains(&RpcAudience::All)
                {
                    if let Some(lxm) = scope.lxm.iter().next() {
                        match lxm {
                            RpcLexicon::All => "rpc:*".to_string(),
                            RpcLexicon::Nsid(nsid) => format!("rpc:{}", nsid),
                        }
                    } else {
                        "rpc:*".to_string()
                    }
                } else {
                    let mut params = Vec::new();

                    for lxm in &scope.lxm {
                        match lxm {
                            RpcLexicon::All => params.push("lxm=*".to_string()),
                            RpcLexicon::Nsid(nsid) => params.push(format!("lxm={}", nsid)),
                        }
                    }

                    for aud in &scope.aud {
                        match aud {
                            RpcAudience::All => params.push("aud=*".to_string()),
                            RpcAudience::Did(did) => params.push(format!("aud={}", did)),
                        }
                    }

                    params.sort();

                    if params.is_empty() {
                        "rpc:*".to_string()
                    } else {
                        format!("rpc?{}", params.join("&"))
                    }
                }
            }
            Scope::Atproto => "atproto".to_string(),
            Scope::Transition(scope) => match scope {
                TransitionScope::Generic => "transition:generic".to_string(),
                TransitionScope::Email => "transition:email".to_string(),
            },
            Scope::OpenId => "openid".to_string(),
            Scope::Profile => "profile".to_string(),
            Scope::Email => "email".to_string(),
        }
    }

    /// Check if this scope grants the permissions of another scope
    pub fn grants(&self, other: &Scope) -> bool {
        match (self, other) {
            // Atproto only grants itself (it's a required scope, not a permission grant)
            (Scope::Atproto, Scope::Atproto) => true,
            (Scope::Atproto, _) => false,
            // Nothing else grants atproto
            (_, Scope::Atproto) => false,
            // Transition scopes only grant themselves
            (Scope::Transition(a), Scope::Transition(b)) => a == b,
            // Other scopes don't grant transition scopes
            (_, Scope::Transition(_)) => false,
            (Scope::Transition(_), _) => false,
            // OpenID Connect scopes only grant themselves
            (Scope::OpenId, Scope::OpenId) => true,
            (Scope::OpenId, _) => false,
            (_, Scope::OpenId) => false,
            (Scope::Profile, Scope::Profile) => true,
            (Scope::Profile, _) => false,
            (_, Scope::Profile) => false,
            (Scope::Email, Scope::Email) => true,
            (Scope::Email, _) => false,
            (_, Scope::Email) => false,
            (Scope::Account(a), Scope::Account(b)) => {
                a.resource == b.resource
                    && matches!(
                        (a.action, b.action),
                        (AccountAction::Manage, _) | (AccountAction::Read, AccountAction::Read)
                    )
            }
            (Scope::Identity(a), Scope::Identity(b)) => matches!(
                (a, b),
                (IdentityScope::All, _) | (IdentityScope::Handle, IdentityScope::Handle)
            ),
            (Scope::Blob(a), Scope::Blob(b)) => {
                for b_pattern in &b.accept {
                    let mut granted = false;
                    for a_pattern in &a.accept {
                        if a_pattern.grants(b_pattern) {
                            granted = true;
                            break;
                        }
                    }
                    if !granted {
                        return false;
                    }
                }
                true
            }
            (Scope::Repo(a), Scope::Repo(b)) => {
                let collection_match = match (&a.collection, &b.collection) {
                    (RepoCollection::All, _) => true,
                    (RepoCollection::Nsid(a_nsid), RepoCollection::Nsid(b_nsid)) => {
                        a_nsid == b_nsid
                    }
                    _ => false,
                };

                if !collection_match {
                    return false;
                }

                b.actions.is_subset(&a.actions) || a.actions.len() == 3
            }
            (Scope::Rpc(a), Scope::Rpc(b)) => {
                let lxm_match = if a.lxm.contains(&RpcLexicon::All) {
                    true
                } else {
                    b.lxm.iter().all(|b_lxm| match b_lxm {
                        RpcLexicon::All => false,
                        RpcLexicon::Nsid(_) => a.lxm.contains(b_lxm),
                    })
                };

                let aud_match = if a.aud.contains(&RpcAudience::All) {
                    true
                } else {
                    b.aud.iter().all(|b_aud| match b_aud {
                        RpcAudience::All => false,
                        RpcAudience::Did(_) => a.aud.contains(b_aud),
                    })
                };

                lxm_match && aud_match
            }
            _ => false,
        }
    }
}

impl MimePattern<'_> {
    fn grants(&self, other: &MimePattern) -> bool {
        match (self, other) {
            (MimePattern::All, _) => true,
            (MimePattern::TypeWildcard(a_type), MimePattern::TypeWildcard(b_type)) => {
                a_type == b_type
            }
            (MimePattern::TypeWildcard(a_type), MimePattern::Exact(b_mime)) => {
                b_mime.starts_with(&format!("{}/", a_type))
            }
            (MimePattern::Exact(a), MimePattern::Exact(b)) => a == b,
            _ => false,
        }
    }
}

impl FromStr for MimePattern<'_> {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "*/*" {
            Ok(MimePattern::All)
        } else if let Some(stripped) = s.strip_suffix("/*") {
            Ok(MimePattern::TypeWildcard(CowStr::Owned(
                stripped.to_smolstr(),
            )))
        } else if s.contains('/') {
            Ok(MimePattern::Exact(CowStr::Owned(s.to_smolstr())))
        } else {
            Err(ParseError::InvalidMimeType(s.to_string()))
        }
    }
}

impl FromStr for Scope<'_> {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Scope<'static>, Self::Err> {
        match Scope::parse(s) {
            Ok(parsed) => Ok(parsed.into_static()),
            Err(e) => Err(e),
        }
    }
}

impl fmt::Display for Scope<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_normalized())
    }
}

/// Parse a query string into a map of keys to lists of values
fn parse_query_string(query: &str) -> BTreeMap<SmolStr, Vec<CowStr<'static>>> {
    let mut params = BTreeMap::new();

    for pair in query.split('&') {
        if let Some(pos) = pair.find('=') {
            let key = &pair[..pos];
            let value = &pair[pos + 1..];
            params
                .entry(key.to_smolstr())
                .or_insert_with(Vec::new)
                .push(CowStr::Owned(value.to_smolstr()));
        }
    }

    params
}

/// Error type for scope parsing
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, miette::Diagnostic)]
pub enum ParseError {
    /// Unknown scope prefix
    UnknownPrefix(String),
    /// Missing required resource
    MissingResource,
    /// Invalid resource type
    InvalidResource(String),
    /// Invalid action type
    InvalidAction(String),
    /// Invalid MIME type
    InvalidMimeType(String),
    ParseError(#[from] AtStrError),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::UnknownPrefix(prefix) => write!(f, "Unknown scope prefix: {}", prefix),
            ParseError::MissingResource => write!(f, "Missing required resource"),
            ParseError::InvalidResource(resource) => write!(f, "Invalid resource: {}", resource),
            ParseError::InvalidAction(action) => write!(f, "Invalid action: {}", action),
            ParseError::InvalidMimeType(mime) => write!(f, "Invalid MIME type: {}", mime),
            ParseError::ParseError(err) => write!(f, "Parse error: {}", err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_scope_parsing() {
        let scope = Scope::parse("account:email").unwrap();
        assert_eq!(
            scope,
            Scope::Account(AccountScope {
                resource: AccountResource::Email,
                action: AccountAction::Read,
            })
        );

        let scope = Scope::parse("account:repo?action=manage").unwrap();
        assert_eq!(
            scope,
            Scope::Account(AccountScope {
                resource: AccountResource::Repo,
                action: AccountAction::Manage,
            })
        );

        let scope = Scope::parse("account:status?action=read").unwrap();
        assert_eq!(
            scope,
            Scope::Account(AccountScope {
                resource: AccountResource::Status,
                action: AccountAction::Read,
            })
        );
    }

    #[test]
    fn test_identity_scope_parsing() {
        let scope = Scope::parse("identity:handle").unwrap();
        assert_eq!(scope, Scope::Identity(IdentityScope::Handle));

        let scope = Scope::parse("identity:*").unwrap();
        assert_eq!(scope, Scope::Identity(IdentityScope::All));
    }

    #[test]
    fn test_blob_scope_parsing() {
        let scope = Scope::parse("blob:*/*").unwrap();
        let mut accept = BTreeSet::new();
        accept.insert(MimePattern::All);
        assert_eq!(scope, Scope::Blob(BlobScope { accept }));

        let scope = Scope::parse("blob:image/png").unwrap();
        let mut accept = BTreeSet::new();
        accept.insert(MimePattern::Exact(CowStr::new_static("image/png")));
        assert_eq!(scope, Scope::Blob(BlobScope { accept }));

        let scope = Scope::parse("blob?accept=image/png&accept=image/jpeg").unwrap();
        let mut accept = BTreeSet::new();
        accept.insert(MimePattern::Exact(CowStr::new_static("image/png")));
        accept.insert(MimePattern::Exact(CowStr::new_static("image/jpeg")));
        assert_eq!(scope, Scope::Blob(BlobScope { accept }));

        let scope = Scope::parse("blob:image/*").unwrap();
        let mut accept = BTreeSet::new();
        accept.insert(MimePattern::TypeWildcard(CowStr::new_static("image")));
        assert_eq!(scope, Scope::Blob(BlobScope { accept }));
    }

    #[test]
    fn test_repo_scope_parsing() {
        let scope = Scope::parse("repo:*?action=create").unwrap();
        let mut actions = BTreeSet::new();
        actions.insert(RepoAction::Create);
        assert_eq!(
            scope,
            Scope::Repo(RepoScope {
                collection: RepoCollection::All,
                actions,
            })
        );

        let scope = Scope::parse("repo:app.bsky.feed.post?action=create&action=update").unwrap();
        let mut actions = BTreeSet::new();
        actions.insert(RepoAction::Create);
        actions.insert(RepoAction::Update);
        assert_eq!(
            scope,
            Scope::Repo(RepoScope {
                collection: RepoCollection::Nsid(Nsid::new_static("app.bsky.feed.post").unwrap()),
                actions,
            })
        );

        let scope = Scope::parse("repo:app.bsky.feed.post").unwrap();
        let mut actions = BTreeSet::new();
        actions.insert(RepoAction::Create);
        actions.insert(RepoAction::Update);
        actions.insert(RepoAction::Delete);
        assert_eq!(
            scope,
            Scope::Repo(RepoScope {
                collection: RepoCollection::Nsid(Nsid::new_static("app.bsky.feed.post").unwrap()),
                actions,
            })
        );
    }

    #[test]
    fn test_rpc_scope_parsing() {
        let scope = Scope::parse("rpc:*").unwrap();
        let mut lxm = BTreeSet::new();
        let mut aud = BTreeSet::new();
        lxm.insert(RpcLexicon::All);
        aud.insert(RpcAudience::All);
        assert_eq!(scope, Scope::Rpc(RpcScope { lxm, aud }));

        let scope = Scope::parse("rpc:com.example.service").unwrap();
        let mut lxm = BTreeSet::new();
        let mut aud = BTreeSet::new();
        lxm.insert(RpcLexicon::Nsid(
            Nsid::new_static("com.example.service").unwrap(),
        ));
        aud.insert(RpcAudience::All);
        assert_eq!(scope, Scope::Rpc(RpcScope { lxm, aud }));

        let scope =
            Scope::parse("rpc:com.example.service?aud=did:plc:yfvwmnlztr4dwkb7hwz55r2g").unwrap();
        let mut lxm = BTreeSet::new();
        let mut aud = BTreeSet::new();
        lxm.insert(RpcLexicon::Nsid(
            Nsid::new_static("com.example.service").unwrap(),
        ));
        aud.insert(RpcAudience::Did(
            Did::new_static("did:plc:yfvwmnlztr4dwkb7hwz55r2g").unwrap(),
        ));
        assert_eq!(scope, Scope::Rpc(RpcScope { lxm, aud }));

        let scope =
            Scope::parse("rpc?lxm=com.example.method1&lxm=com.example.method2&aud=did:plc:yfvwmnlztr4dwkb7hwz55r2g")
                .unwrap();
        let mut lxm = BTreeSet::new();
        let mut aud = BTreeSet::new();
        lxm.insert(RpcLexicon::Nsid(
            Nsid::new_static("com.example.method1").unwrap(),
        ));
        lxm.insert(RpcLexicon::Nsid(
            Nsid::new_static("com.example.method2").unwrap(),
        ));
        aud.insert(RpcAudience::Did(
            Did::new_static("did:plc:yfvwmnlztr4dwkb7hwz55r2g").unwrap(),
        ));
        assert_eq!(scope, Scope::Rpc(RpcScope { lxm, aud }));
    }

    #[test]
    fn test_scope_normalization() {
        let tests = vec![
            ("account:email", "account:email"),
            ("account:email?action=read", "account:email"),
            ("account:email?action=manage", "account:email?action=manage"),
            ("blob:image/png", "blob:image/png"),
            (
                "blob?accept=image/jpeg&accept=image/png",
                "blob?accept=image/jpeg&accept=image/png",
            ),
            ("repo:app.bsky.feed.post", "repo:app.bsky.feed.post"),
            (
                "repo:app.bsky.feed.post?action=create",
                "repo:app.bsky.feed.post?action=create",
            ),
            ("rpc:*", "rpc:*"),
        ];

        for (input, expected) in tests {
            let scope = Scope::parse(input).unwrap();
            assert_eq!(scope.to_string_normalized(), expected);
        }
    }

    #[test]
    fn test_account_scope_grants() {
        let manage = Scope::parse("account:email?action=manage").unwrap();
        let read = Scope::parse("account:email?action=read").unwrap();
        let other_read = Scope::parse("account:repo?action=read").unwrap();

        assert!(manage.grants(&read));
        assert!(manage.grants(&manage));
        assert!(!read.grants(&manage));
        assert!(read.grants(&read));
        assert!(!read.grants(&other_read));
    }

    #[test]
    fn test_identity_scope_grants() {
        let all = Scope::parse("identity:*").unwrap();
        let handle = Scope::parse("identity:handle").unwrap();

        assert!(all.grants(&handle));
        assert!(all.grants(&all));
        assert!(!handle.grants(&all));
        assert!(handle.grants(&handle));
    }

    #[test]
    fn test_blob_scope_grants() {
        let all = Scope::parse("blob:*/*").unwrap();
        let image_all = Scope::parse("blob:image/*").unwrap();
        let image_png = Scope::parse("blob:image/png").unwrap();
        let text_plain = Scope::parse("blob:text/plain").unwrap();

        assert!(all.grants(&image_all));
        assert!(all.grants(&image_png));
        assert!(all.grants(&text_plain));
        assert!(image_all.grants(&image_png));
        assert!(!image_all.grants(&text_plain));
        assert!(!image_png.grants(&image_all));
    }

    #[test]
    fn test_repo_scope_grants() {
        let all_all = Scope::parse("repo:*").unwrap();
        let all_create = Scope::parse("repo:*?action=create").unwrap();
        let specific_all = Scope::parse("repo:app.bsky.feed.post").unwrap();
        let specific_create = Scope::parse("repo:app.bsky.feed.post?action=create").unwrap();
        let other_create = Scope::parse("repo:pub.leaflet.publication?action=create").unwrap();

        assert!(all_all.grants(&all_create));
        assert!(all_all.grants(&specific_all));
        assert!(all_all.grants(&specific_create));
        assert!(all_create.grants(&all_create));
        assert!(!all_create.grants(&specific_all));
        assert!(specific_all.grants(&specific_create));
        assert!(!specific_create.grants(&specific_all));
        assert!(!specific_create.grants(&other_create));
    }

    #[test]
    fn test_rpc_scope_grants() {
        let all = Scope::parse("rpc:*").unwrap();
        let specific_lxm = Scope::parse("rpc:com.example.service").unwrap();
        let specific_both = Scope::parse("rpc:com.example.service?aud=did:example:123").unwrap();

        assert!(all.grants(&specific_lxm));
        assert!(all.grants(&specific_both));
        assert!(specific_lxm.grants(&specific_both));
        assert!(!specific_both.grants(&specific_lxm));
        assert!(!specific_both.grants(&all));
    }

    #[test]
    fn test_cross_scope_grants() {
        let account = Scope::parse("account:email").unwrap();
        let identity = Scope::parse("identity:handle").unwrap();

        assert!(!account.grants(&identity));
        assert!(!identity.grants(&account));
    }

    #[test]
    fn test_parse_errors() {
        assert!(matches!(
            Scope::parse("unknown:test"),
            Err(ParseError::UnknownPrefix(_))
        ));

        assert!(matches!(
            Scope::parse("account"),
            Err(ParseError::MissingResource)
        ));

        assert!(matches!(
            Scope::parse("account:invalid"),
            Err(ParseError::InvalidResource(_))
        ));

        assert!(matches!(
            Scope::parse("account:email?action=invalid"),
            Err(ParseError::InvalidAction(_))
        ));
    }

    #[test]
    fn test_query_parameter_sorting() {
        let scope =
            Scope::parse("blob?accept=image/png&accept=application/pdf&accept=image/jpeg").unwrap();
        let normalized = scope.to_string_normalized();
        assert!(normalized.contains("accept=application/pdf"));
        assert!(normalized.contains("accept=image/jpeg"));
        assert!(normalized.contains("accept=image/png"));
        let pdf_pos = normalized.find("accept=application/pdf").unwrap();
        let jpeg_pos = normalized.find("accept=image/jpeg").unwrap();
        let png_pos = normalized.find("accept=image/png").unwrap();
        assert!(pdf_pos < jpeg_pos);
        assert!(jpeg_pos < png_pos);
    }

    #[test]
    fn test_repo_action_wildcard() {
        let scope = Scope::parse("repo:app.bsky.feed.post?action=*").unwrap();
        let mut actions = BTreeSet::new();
        actions.insert(RepoAction::Create);
        actions.insert(RepoAction::Update);
        actions.insert(RepoAction::Delete);
        assert_eq!(
            scope,
            Scope::Repo(RepoScope {
                collection: RepoCollection::Nsid(Nsid::new_static("app.bsky.feed.post").unwrap()),
                actions,
            })
        );
    }

    #[test]
    fn test_multiple_blob_accepts() {
        let scope = Scope::parse("blob?accept=image/*&accept=text/plain").unwrap();
        assert!(scope.grants(&Scope::parse("blob:image/png").unwrap()));
        assert!(scope.grants(&Scope::parse("blob:text/plain").unwrap()));
        assert!(!scope.grants(&Scope::parse("blob:application/json").unwrap()));
    }

    #[test]
    fn test_rpc_default_wildcards() {
        let scope = Scope::parse("rpc").unwrap();
        let mut lxm = BTreeSet::new();
        let mut aud = BTreeSet::new();
        lxm.insert(RpcLexicon::All);
        aud.insert(RpcAudience::All);
        assert_eq!(scope, Scope::Rpc(RpcScope { lxm, aud }));
    }

    #[test]
    fn test_atproto_scope_parsing() {
        let scope = Scope::parse("atproto").unwrap();
        assert_eq!(scope, Scope::Atproto);

        // Atproto should not accept suffixes
        assert!(Scope::parse("atproto:something").is_err());
        assert!(Scope::parse("atproto?param=value").is_err());
    }

    #[test]
    fn test_transition_scope_parsing() {
        let scope = Scope::parse("transition:generic").unwrap();
        assert_eq!(scope, Scope::Transition(TransitionScope::Generic));

        let scope = Scope::parse("transition:email").unwrap();
        assert_eq!(scope, Scope::Transition(TransitionScope::Email));

        // Test invalid transition types
        assert!(matches!(
            Scope::parse("transition:invalid"),
            Err(ParseError::InvalidResource(_))
        ));

        // Test missing suffix
        assert!(matches!(
            Scope::parse("transition"),
            Err(ParseError::MissingResource)
        ));

        // Test transition doesn't accept query parameters
        assert!(matches!(
            Scope::parse("transition:generic?param=value"),
            Err(ParseError::InvalidResource(_))
        ));
    }

    #[test]
    fn test_atproto_scope_normalization() {
        let scope = Scope::parse("atproto").unwrap();
        assert_eq!(scope.to_string_normalized(), "atproto");
    }

    #[test]
    fn test_transition_scope_normalization() {
        let tests = vec![
            ("transition:generic", "transition:generic"),
            ("transition:email", "transition:email"),
        ];

        for (input, expected) in tests {
            let scope = Scope::parse(input).unwrap();
            assert_eq!(scope.to_string_normalized(), expected);
        }
    }

    #[test]
    fn test_atproto_scope_grants() {
        let atproto = Scope::parse("atproto").unwrap();
        let account = Scope::parse("account:email").unwrap();
        let identity = Scope::parse("identity:handle").unwrap();
        let blob = Scope::parse("blob:image/png").unwrap();
        let repo = Scope::parse("repo:app.bsky.feed.post").unwrap();
        let rpc = Scope::parse("rpc:com.example.service").unwrap();
        let transition_generic = Scope::parse("transition:generic").unwrap();
        let transition_email = Scope::parse("transition:email").unwrap();

        // Atproto only grants itself (it's a required scope, not a permission grant)
        assert!(atproto.grants(&atproto));
        assert!(!atproto.grants(&account));
        assert!(!atproto.grants(&identity));
        assert!(!atproto.grants(&blob));
        assert!(!atproto.grants(&repo));
        assert!(!atproto.grants(&rpc));
        assert!(!atproto.grants(&transition_generic));
        assert!(!atproto.grants(&transition_email));

        // Nothing else grants atproto
        assert!(!account.grants(&atproto));
        assert!(!identity.grants(&atproto));
        assert!(!blob.grants(&atproto));
        assert!(!repo.grants(&atproto));
        assert!(!rpc.grants(&atproto));
        assert!(!transition_generic.grants(&atproto));
        assert!(!transition_email.grants(&atproto));
    }

    #[test]
    fn test_transition_scope_grants() {
        let transition_generic = Scope::parse("transition:generic").unwrap();
        let transition_email = Scope::parse("transition:email").unwrap();
        let account = Scope::parse("account:email").unwrap();

        // Transition scopes only grant themselves
        assert!(transition_generic.grants(&transition_generic));
        assert!(transition_email.grants(&transition_email));
        assert!(!transition_generic.grants(&transition_email));
        assert!(!transition_email.grants(&transition_generic));

        // Transition scopes don't grant other scope types
        assert!(!transition_generic.grants(&account));
        assert!(!transition_email.grants(&account));

        // Other scopes don't grant transition scopes
        assert!(!account.grants(&transition_generic));
        assert!(!account.grants(&transition_email));
    }

    #[test]
    fn test_parse_multiple() {
        // Test parsing multiple scopes
        let scopes = Scope::parse_multiple("atproto repo:*").unwrap();
        assert_eq!(scopes.len(), 2);
        assert_eq!(scopes[0], Scope::Atproto);
        assert_eq!(
            scopes[1],
            Scope::Repo(RepoScope {
                collection: RepoCollection::All,
                actions: {
                    let mut actions = BTreeSet::new();
                    actions.insert(RepoAction::Create);
                    actions.insert(RepoAction::Update);
                    actions.insert(RepoAction::Delete);
                    actions
                }
            })
        );

        // Test with more scopes
        let scopes = Scope::parse_multiple("account:email identity:handle blob:image/png").unwrap();
        assert_eq!(scopes.len(), 3);
        assert!(matches!(scopes[0], Scope::Account(_)));
        assert!(matches!(scopes[1], Scope::Identity(_)));
        assert!(matches!(scopes[2], Scope::Blob(_)));

        // Test with complex scopes
        let scopes = Scope::parse_multiple(
            "account:email?action=manage repo:app.bsky.feed.post?action=create transition:email",
        )
        .unwrap();
        assert_eq!(scopes.len(), 3);

        // Test empty string
        let scopes = Scope::parse_multiple("").unwrap();
        assert_eq!(scopes.len(), 0);

        // Test whitespace only
        let scopes = Scope::parse_multiple("   ").unwrap();
        assert_eq!(scopes.len(), 0);

        // Test with extra whitespace
        let scopes = Scope::parse_multiple("  atproto   repo:*  ").unwrap();
        assert_eq!(scopes.len(), 2);

        // Test single scope
        let scopes = Scope::parse_multiple("atproto").unwrap();
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0], Scope::Atproto);

        // Test error propagation
        assert!(Scope::parse_multiple("atproto invalid:scope").is_err());
        assert!(Scope::parse_multiple("account:invalid repo:*").is_err());
    }

    #[test]
    fn test_parse_multiple_reduced() {
        // Test repo scope reduction - wildcard grants specific
        let scopes =
            Scope::parse_multiple_reduced("atproto repo:app.bsky.feed.post repo:*").unwrap();
        assert_eq!(scopes.len(), 2);
        assert!(scopes.contains(&Scope::Atproto));
        assert!(scopes.contains(&Scope::Repo(RepoScope {
            collection: RepoCollection::All,
            actions: {
                let mut actions = BTreeSet::new();
                actions.insert(RepoAction::Create);
                actions.insert(RepoAction::Update);
                actions.insert(RepoAction::Delete);
                actions
            }
        })));

        // Test reverse order - should get same result
        let scopes =
            Scope::parse_multiple_reduced("atproto repo:* repo:app.bsky.feed.post").unwrap();
        assert_eq!(scopes.len(), 2);
        assert!(scopes.contains(&Scope::Atproto));
        assert!(scopes.contains(&Scope::Repo(RepoScope {
            collection: RepoCollection::All,
            actions: {
                let mut actions = BTreeSet::new();
                actions.insert(RepoAction::Create);
                actions.insert(RepoAction::Update);
                actions.insert(RepoAction::Delete);
                actions
            }
        })));

        // Test account scope reduction - manage grants read
        let scopes =
            Scope::parse_multiple_reduced("account:email account:email?action=manage").unwrap();
        assert_eq!(scopes.len(), 1);
        assert_eq!(
            scopes[0],
            Scope::Account(AccountScope {
                resource: AccountResource::Email,
                action: AccountAction::Manage,
            })
        );

        // Test identity scope reduction - wildcard grants specific
        let scopes = Scope::parse_multiple_reduced("identity:handle identity:*").unwrap();
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0], Scope::Identity(IdentityScope::All));

        // Test blob scope reduction - wildcard grants specific
        let scopes = Scope::parse_multiple_reduced("blob:image/png blob:image/* blob:*/*").unwrap();
        assert_eq!(scopes.len(), 1);
        let mut accept = BTreeSet::new();
        accept.insert(MimePattern::All);
        assert_eq!(scopes[0], Scope::Blob(BlobScope { accept }));

        // Test no reduction needed - different scope types
        let scopes =
            Scope::parse_multiple_reduced("account:email identity:handle blob:image/png").unwrap();
        assert_eq!(scopes.len(), 3);

        // Test repo action reduction
        let scopes = Scope::parse_multiple_reduced(
            "repo:app.bsky.feed.post?action=create repo:app.bsky.feed.post",
        )
        .unwrap();
        assert_eq!(scopes.len(), 1);
        assert_eq!(
            scopes[0],
            Scope::Repo(RepoScope {
                collection: RepoCollection::Nsid(Nsid::new_static("app.bsky.feed.post").unwrap()),
                actions: {
                    let mut actions = BTreeSet::new();
                    actions.insert(RepoAction::Create);
                    actions.insert(RepoAction::Update);
                    actions.insert(RepoAction::Delete);
                    actions
                }
            })
        );

        // Test RPC scope reduction
        let scopes = Scope::parse_multiple_reduced(
            "rpc:com.example.service?aud=did:example:123 rpc:com.example.service rpc:*",
        )
        .unwrap();
        assert_eq!(scopes.len(), 1);
        assert_eq!(
            scopes[0],
            Scope::Rpc(RpcScope {
                lxm: {
                    let mut lxm = BTreeSet::new();
                    lxm.insert(RpcLexicon::All);
                    lxm
                },
                aud: {
                    let mut aud = BTreeSet::new();
                    aud.insert(RpcAudience::All);
                    aud
                }
            })
        );

        // Test duplicate removal
        let scopes = Scope::parse_multiple_reduced("atproto atproto atproto").unwrap();
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0], Scope::Atproto);

        // Test transition scopes - only grant themselves
        let scopes = Scope::parse_multiple_reduced("transition:generic transition:email").unwrap();
        assert_eq!(scopes.len(), 2);
        assert!(scopes.contains(&Scope::Transition(TransitionScope::Generic)));
        assert!(scopes.contains(&Scope::Transition(TransitionScope::Email)));

        // Test empty input
        let scopes = Scope::parse_multiple_reduced("").unwrap();
        assert_eq!(scopes.len(), 0);

        // Test complex scenario with multiple reductions
        let scopes = Scope::parse_multiple_reduced(
            "account:email?action=manage account:email account:repo account:repo?action=read identity:* identity:handle"
        ).unwrap();
        assert_eq!(scopes.len(), 3);
        // Should have: account:email?action=manage, account:repo, identity:*
        assert!(scopes.contains(&Scope::Account(AccountScope {
            resource: AccountResource::Email,
            action: AccountAction::Manage,
        })));
        assert!(scopes.contains(&Scope::Account(AccountScope {
            resource: AccountResource::Repo,
            action: AccountAction::Read,
        })));
        assert!(scopes.contains(&Scope::Identity(IdentityScope::All)));

        // Test that atproto doesn't grant other scopes (per recent change)
        let scopes = Scope::parse_multiple_reduced("atproto account:email repo:*").unwrap();
        assert_eq!(scopes.len(), 3);
        assert!(scopes.contains(&Scope::Atproto));
        assert!(scopes.contains(&Scope::Account(AccountScope {
            resource: AccountResource::Email,
            action: AccountAction::Read,
        })));
        assert!(scopes.contains(&Scope::Repo(RepoScope {
            collection: RepoCollection::All,
            actions: {
                let mut actions = BTreeSet::new();
                actions.insert(RepoAction::Create);
                actions.insert(RepoAction::Update);
                actions.insert(RepoAction::Delete);
                actions
            }
        })));
    }

    #[test]
    fn test_openid_connect_scope_parsing() {
        // Test OpenID scope
        let scope = Scope::parse("openid").unwrap();
        assert_eq!(scope, Scope::OpenId);

        // Test Profile scope
        let scope = Scope::parse("profile").unwrap();
        assert_eq!(scope, Scope::Profile);

        // Test Email scope
        let scope = Scope::parse("email").unwrap();
        assert_eq!(scope, Scope::Email);

        // Test that they don't accept suffixes
        assert!(Scope::parse("openid:something").is_err());
        assert!(Scope::parse("profile:something").is_err());
        assert!(Scope::parse("email:something").is_err());

        // Test that they don't accept query parameters
        assert!(Scope::parse("openid?param=value").is_err());
        assert!(Scope::parse("profile?param=value").is_err());
        assert!(Scope::parse("email?param=value").is_err());
    }

    #[test]
    fn test_openid_connect_scope_normalization() {
        let scope = Scope::parse("openid").unwrap();
        assert_eq!(scope.to_string_normalized(), "openid");

        let scope = Scope::parse("profile").unwrap();
        assert_eq!(scope.to_string_normalized(), "profile");

        let scope = Scope::parse("email").unwrap();
        assert_eq!(scope.to_string_normalized(), "email");
    }

    #[test]
    fn test_openid_connect_scope_grants() {
        let openid = Scope::parse("openid").unwrap();
        let profile = Scope::parse("profile").unwrap();
        let email = Scope::parse("email").unwrap();
        let account = Scope::parse("account:email").unwrap();

        // OpenID Connect scopes only grant themselves
        assert!(openid.grants(&openid));
        assert!(!openid.grants(&profile));
        assert!(!openid.grants(&email));
        assert!(!openid.grants(&account));

        assert!(profile.grants(&profile));
        assert!(!profile.grants(&openid));
        assert!(!profile.grants(&email));
        assert!(!profile.grants(&account));

        assert!(email.grants(&email));
        assert!(!email.grants(&openid));
        assert!(!email.grants(&profile));
        assert!(!email.grants(&account));

        // Other scopes don't grant OpenID Connect scopes
        assert!(!account.grants(&openid));
        assert!(!account.grants(&profile));
        assert!(!account.grants(&email));
    }

    #[test]
    fn test_parse_multiple_with_openid_connect() {
        let scopes = Scope::parse_multiple("openid profile email atproto").unwrap();
        assert_eq!(scopes.len(), 4);
        assert_eq!(scopes[0], Scope::OpenId);
        assert_eq!(scopes[1], Scope::Profile);
        assert_eq!(scopes[2], Scope::Email);
        assert_eq!(scopes[3], Scope::Atproto);

        // Test with mixed scopes
        let scopes = Scope::parse_multiple("openid account:email profile repo:*").unwrap();
        assert_eq!(scopes.len(), 4);
        assert!(scopes.contains(&Scope::OpenId));
        assert!(scopes.contains(&Scope::Profile));
    }

    #[test]
    fn test_parse_multiple_reduced_with_openid_connect() {
        // OpenID Connect scopes don't grant each other, so no reduction
        let scopes = Scope::parse_multiple_reduced("openid profile email openid").unwrap();
        assert_eq!(scopes.len(), 3);
        assert!(scopes.contains(&Scope::OpenId));
        assert!(scopes.contains(&Scope::Profile));
        assert!(scopes.contains(&Scope::Email));

        // Mixed with other scopes
        let scopes = Scope::parse_multiple_reduced(
            "openid account:email account:email?action=manage profile",
        )
        .unwrap();
        assert_eq!(scopes.len(), 3);
        assert!(scopes.contains(&Scope::OpenId));
        assert!(scopes.contains(&Scope::Profile));
        assert!(scopes.contains(&Scope::Account(AccountScope {
            resource: AccountResource::Email,
            action: AccountAction::Manage,
        })));
    }

    #[test]
    fn test_serialize_multiple() {
        // Test empty list
        let scopes: Vec<Scope> = vec![];
        assert_eq!(Scope::serialize_multiple(&scopes), "");

        // Test single scope
        let scopes = vec![Scope::Atproto];
        assert_eq!(Scope::serialize_multiple(&scopes), "atproto");

        // Test multiple scopes - should be sorted alphabetically
        let scopes = vec![
            Scope::parse("repo:*").unwrap(),
            Scope::Atproto,
            Scope::parse("account:email").unwrap(),
        ];
        assert_eq!(
            Scope::serialize_multiple(&scopes),
            "account:email atproto repo:*"
        );

        // Test that sorting is consistent regardless of input order
        let scopes = vec![
            Scope::parse("identity:handle").unwrap(),
            Scope::parse("blob:image/png").unwrap(),
            Scope::parse("account:repo?action=manage").unwrap(),
        ];
        assert_eq!(
            Scope::serialize_multiple(&scopes),
            "account:repo?action=manage blob:image/png identity:handle"
        );

        // Test with OpenID Connect scopes
        let scopes = vec![Scope::Email, Scope::OpenId, Scope::Profile, Scope::Atproto];
        assert_eq!(
            Scope::serialize_multiple(&scopes),
            "atproto email openid profile"
        );

        // Test with complex scopes including query parameters
        let scopes = vec![
            Scope::parse("rpc:com.example.service?aud=did:plc:yfvwmnlztr4dwkb7hwz55r2g&lxm=com.example.method")
                .unwrap(),
            Scope::parse("repo:app.bsky.feed.post?action=create&action=update").unwrap(),
            Scope::parse("blob:image/*?accept=image/png&accept=image/jpeg").unwrap(),
        ];
        let result = Scope::serialize_multiple(&scopes);
        // The result should be sorted alphabetically
        // Note: RPC scope with query params is serialized as "rpc?aud=...&lxm=..."
        assert!(result.starts_with("blob:"));
        assert!(result.contains(" repo:"));
        assert!(
            result.contains("rpc?aud=did:plc:yfvwmnlztr4dwkb7hwz55r2g&lxm=com.example.service")
        );

        // Test with transition scopes
        let scopes = vec![
            Scope::Transition(TransitionScope::Email),
            Scope::Transition(TransitionScope::Generic),
            Scope::Atproto,
        ];
        assert_eq!(
            Scope::serialize_multiple(&scopes),
            "atproto transition:email transition:generic"
        );

        // Test duplicates - they remain in the output (caller's responsibility to dedupe if needed)
        let scopes = vec![
            Scope::Atproto,
            Scope::Atproto,
            Scope::parse("account:email").unwrap(),
        ];
        assert_eq!(
            Scope::serialize_multiple(&scopes),
            "account:email atproto atproto"
        );

        // Test normalization is preserved in serialization
        let scopes = vec![Scope::parse("blob?accept=image/png&accept=image/jpeg").unwrap()];
        // Should normalize query parameters alphabetically
        assert_eq!(
            Scope::serialize_multiple(&scopes),
            "blob?accept=image/jpeg&accept=image/png"
        );
    }

    #[test]
    fn test_serialize_multiple_roundtrip() {
        // Test that parse_multiple and serialize_multiple are inverses (when sorted)
        let original = "account:email atproto blob:image/png identity:handle repo:*";
        let scopes = Scope::parse_multiple(original).unwrap();
        let serialized = Scope::serialize_multiple(&scopes);
        assert_eq!(serialized, original);

        // Test with complex scopes
        let original = "account:repo?action=manage blob?accept=image/jpeg&accept=image/png rpc:*";
        let scopes = Scope::parse_multiple(original).unwrap();
        let serialized = Scope::serialize_multiple(&scopes);
        // Parse again to verify it's valid
        let reparsed = Scope::parse_multiple(&serialized).unwrap();
        assert_eq!(scopes, reparsed);

        // Test with OpenID Connect scopes
        let original = "email openid profile";
        let scopes = Scope::parse_multiple(original).unwrap();
        let serialized = Scope::serialize_multiple(&scopes);
        assert_eq!(serialized, original);
    }

    #[test]
    fn test_remove_scope() {
        // Test removing a scope that exists
        let scopes = vec![
            Scope::parse("repo:*").unwrap(),
            Scope::Atproto,
            Scope::parse("account:email").unwrap(),
        ];
        let to_remove = Scope::Atproto;
        let result = Scope::remove_scope(&scopes, &to_remove);
        assert_eq!(result.len(), 2);
        assert!(!result.contains(&to_remove));
        assert!(result.contains(&Scope::parse("repo:*").unwrap()));
        assert!(result.contains(&Scope::parse("account:email").unwrap()));

        // Test removing a scope that doesn't exist
        let scopes = vec![
            Scope::parse("repo:*").unwrap(),
            Scope::parse("account:email").unwrap(),
        ];
        let to_remove = Scope::parse("identity:handle").unwrap();
        let result = Scope::remove_scope(&scopes, &to_remove);
        assert_eq!(result.len(), 2);
        assert_eq!(result, scopes);

        // Test removing from empty list
        let scopes: Vec<Scope> = vec![];
        let to_remove = Scope::Atproto;
        let result = Scope::remove_scope(&scopes, &to_remove);
        assert_eq!(result.len(), 0);

        // Test removing all instances of a duplicate scope
        let scopes = vec![
            Scope::Atproto,
            Scope::parse("account:email").unwrap(),
            Scope::Atproto,
            Scope::parse("repo:*").unwrap(),
            Scope::Atproto,
        ];
        let to_remove = Scope::Atproto;
        let result = Scope::remove_scope(&scopes, &to_remove);
        assert_eq!(result.len(), 2);
        assert!(!result.contains(&to_remove));
        assert!(result.contains(&Scope::parse("account:email").unwrap()));
        assert!(result.contains(&Scope::parse("repo:*").unwrap()));

        // Test removing complex scopes with query parameters
        let scopes = vec![
            Scope::parse("account:email?action=manage").unwrap(),
            Scope::parse("blob?accept=image/png&accept=image/jpeg").unwrap(),
            Scope::parse("rpc:com.example.service?aud=did:example:123").unwrap(),
        ];
        let to_remove = Scope::parse("blob?accept=image/jpeg&accept=image/png").unwrap(); // Note: normalized order
        let result = Scope::remove_scope(&scopes, &to_remove);
        assert_eq!(result.len(), 2);
        assert!(!result.contains(&to_remove));

        // Test with OpenID Connect scopes
        let scopes = vec![Scope::OpenId, Scope::Profile, Scope::Email, Scope::Atproto];
        let to_remove = Scope::Profile;
        let result = Scope::remove_scope(&scopes, &to_remove);
        assert_eq!(result.len(), 3);
        assert!(!result.contains(&to_remove));
        assert!(result.contains(&Scope::OpenId));
        assert!(result.contains(&Scope::Email));
        assert!(result.contains(&Scope::Atproto));

        // Test with transition scopes
        let scopes = vec![
            Scope::Transition(TransitionScope::Generic),
            Scope::Transition(TransitionScope::Email),
            Scope::Atproto,
        ];
        let to_remove = Scope::Transition(TransitionScope::Email);
        let result = Scope::remove_scope(&scopes, &to_remove);
        assert_eq!(result.len(), 2);
        assert!(!result.contains(&to_remove));
        assert!(result.contains(&Scope::Transition(TransitionScope::Generic)));
        assert!(result.contains(&Scope::Atproto));

        // Test that only exact matches are removed
        let scopes = vec![
            Scope::parse("account:email").unwrap(),
            Scope::parse("account:email?action=manage").unwrap(),
            Scope::parse("account:repo").unwrap(),
        ];
        let to_remove = Scope::parse("account:email").unwrap();
        let result = Scope::remove_scope(&scopes, &to_remove);
        assert_eq!(result.len(), 2);
        assert!(!result.contains(&Scope::parse("account:email").unwrap()));
        assert!(result.contains(&Scope::parse("account:email?action=manage").unwrap()));
        assert!(result.contains(&Scope::parse("account:repo").unwrap()));
    }
}
