//! AT Protocol OAuth scopes
//!
//! Originally derived from <https://tangled.org/nickgerakines.me/atproto-crates/raw/main/crates/atproto-oauth/src/scopes.rs>, since substantially modified.
//!
//! This module provides comprehensive support for AT Protocol OAuth scopes,
//! including parsing, serialization, normalization, programmatic construction,
//! and permission checking.
//!
//! See the AT Protocol OAuth spec's
//! [authorization scopes](https://atproto.com/specs/oauth#authorization-scopes)
//! section for the profile-level rules, including the required `atproto` marker
//! scope and current
//! [transitional scopes](https://atproto.com/specs/oauth#transitional-scopes).
//! The AT Protocol docs also provide an interactive
//! [scope builder](https://atproto.com/guides/scope-builder) for choosing
//! appropriate scopes before translating them into [`Scope`](crate::scopes::Scope)
//! constructors or a [`ScopesBuilder`](crate::scopes::ScopesBuilder) chain.
//!
//! Scopes in AT Protocol follow a prefix-based format with optional query parameters:
//! - `account`: Access to account information (email, repo, status)
//! - `identity`: Access to identity information (handle)
//! - `blob`: Access to blob operations with mime type constraints
//! - `repo`: Repository operations with collection and action constraints
//! - `rpc`: RPC method access with lexicon and audience constraints
//! - `include`: Reference a permission-set NSID, optionally with an audience
//! - `atproto`: Required scope to indicate that other AT Protocol scopes will be used
//! - `transition`: Migration operations (generic, email, or chat.bsky)
//!
//! Standard OpenID Connect scopes (no suffixes or query parameters):
//! - `openid`: Required for OpenID Connect authentication
//! - `profile`: Access to user profile information
//! - `email`: Access to user email address
//!
//! Programmatic construction is available through
//! [`Scope`](crate::scopes::Scope) constructors,
//! [`Scopes::from_scopes`](crate::scopes::Scopes::from_scopes), and
//! [`ScopesBuilder`](crate::scopes::ScopesBuilder):
//!
//! ```rust
//! use jacquard_oauth::scopes::{Scope, Scopes};
//!
//! let scopes = Scopes::from_scopes([
//!     Scope::atproto(),
//!     Scope::transition_generic(),
//!     Scope::rpc("app.bsky.feed.getTimeline")?,
//!     Scope::repo_create("app.bsky.feed.post")?,
//! ])?;
//!
//! assert_eq!(
//!     scopes.to_normalized_string(),
//!     "atproto repo:app.bsky.feed.post?action=create rpc:app.bsky.feed.getTimeline transition:generic"
//! );
//! # Ok::<(), jacquard_oauth::scopes::ParseError>(())
//! ```
//!
//! For fluent construction:
//!
//! ```rust
//! use jacquard_oauth::scopes::Scopes;
//!
//! let scopes = Scopes::builder()
//!     .atproto()
//!     .transition_generic()
//!     .rpc("app.bsky.feed.getTimeline")?
//!     .repo_create("app.bsky.feed.post")?
//!     .build()?;
//!
//! assert!(scopes.grants(&jacquard_oauth::scopes::Scope::atproto()));
//! # Ok::<(), jacquard_oauth::scopes::ParseError>(())
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use jacquard_common::bos::{BosStr, DefaultStr};
use jacquard_common::deps::fluent_uri::pct_enc::{
    EStr, EString,
    encoder::{Query, Query as EncQuery},
};
use jacquard_common::types::collection::Collection;
use jacquard_common::types::did::validate_did;
use jacquard_common::types::did_service::validate_did_service;
use jacquard_common::types::nsid::Nsid;
use jacquard_common::types::string::{AtStrError, DidService};
use jacquard_common::xrpc::XrpcRequest;
use jacquard_common::{BorrowOrShare, Bos, FromStaticStr, IntoStatic};
use serde::de::{Error as DeError, Visitor};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use smol_str::{SmolStr, SmolStrBuilder, ToSmolStr, format_smolstr};

/// Represents an AT Protocol OAuth scope
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Scope<S: BosStr = DefaultStr> {
    /// Account scope for accessing account information
    Account(AccountScope),
    /// Identity scope for accessing identity information
    Identity(IdentityScope),
    /// Blob scope for blob operations with mime type constraints
    Blob(BlobScope<S>),
    /// Repository scope for collection operations
    Repo(RepoScope<S>),
    /// RPC scope for method access
    Rpc(RpcScope<S>),
    /// Space scope for permissioned-space access
    Space(SpaceScope<S>),
    /// AT Protocol scope - required to indicate that other AT Protocol scopes will be used
    Atproto,
    /// Transition scope for migration operations
    Transition(TransitionScope),
    /// Include scope referencing a permission set
    Include(IncludeScope<S>),
    /// OpenID Connect scope - required for OpenID Connect authentication
    OpenId,
    /// Profile scope - access to user profile information
    Profile,
    /// Email scope - access to user email address
    Email,
}

impl<S: BosStr + Ord> Serialize for Scope<S> {
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string_normalized())
    }
}

impl<'de, S> Deserialize<'de> for Scope<S>
where
    S: BosStr + Ord + Deserialize<'de> + FromStr,
    <S as FromStr>::Err: core::fmt::Debug,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ScopeVisitor<St: BosStr + Ord + FromStr>(PhantomData<St>);

        impl<St: BosStr + Ord + FromStr> Visitor<'_> for ScopeVisitor<St>
        where
            <St as FromStr>::Err: core::fmt::Debug,
        {
            type Value = Scope<St>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "a scope string")
            }
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Scope::parse(v).map_err(|e| serde::de::Error::custom(format!("{:?}", e)))
            }
        }
        deserializer
            .deserialize_str(ScopeVisitor(PhantomData))
            .map(|scope| scope)
    }
}

impl<S: BosStr + Ord + IntoStatic> IntoStatic for Scope<S>
where
    S::Output: BosStr + Ord,
{
    type Output = Scope<S::Output>;

    fn into_static(self) -> Self::Output {
        match self {
            Scope::Account(scope) => Scope::Account(scope),
            Scope::Identity(scope) => Scope::Identity(scope),
            Scope::Blob(scope) => Scope::Blob(scope.into_static()),
            Scope::Repo(scope) => Scope::Repo(scope.into_static()),
            Scope::Rpc(scope) => Scope::Rpc(scope.into_static()),
            Scope::Space(scope) => Scope::Space(scope.into_static()),
            Scope::Atproto => Scope::Atproto,
            Scope::Transition(scope) => Scope::Transition(scope),
            Scope::Include(scope) => Scope::Include(scope.into_static()),
            Scope::OpenId => Scope::OpenId,
            Scope::Profile => Scope::Profile,
            Scope::Email => Scope::Email,
        }
    }
}

impl Scope<SmolStr> {
    /// Construct the required AT Protocol marker scope.
    pub fn atproto() -> Self {
        Scope::Atproto
    }

    /// Construct the OpenID Connect `openid` scope.
    pub fn openid() -> Self {
        Scope::OpenId
    }

    /// Construct the OpenID Connect `profile` scope.
    pub fn profile() -> Self {
        Scope::Profile
    }

    /// Construct the OpenID Connect `email` scope.
    pub fn email() -> Self {
        Scope::Email
    }

    /// Construct `identity:handle`.
    pub fn identity_handle() -> Self {
        Scope::Identity(IdentityScope::Handle)
    }

    /// Construct `identity:*`.
    pub fn identity_all() -> Self {
        Scope::Identity(IdentityScope::All)
    }

    /// Construct `transition:generic`.
    pub fn transition_generic() -> Self {
        Scope::Transition(TransitionScope::Generic)
    }

    /// Construct `transition:email`.
    pub fn transition_email() -> Self {
        Scope::Transition(TransitionScope::Email)
    }

    /// Construct `transition:chat.bsky`.
    pub fn transition_chat_bsky() -> Self {
        Scope::Transition(TransitionScope::ChatBsky)
    }

    /// Construct `account:email`.
    pub fn account_email_read() -> Self {
        account_scope(AccountResource::Email, AccountAction::Read)
    }

    /// Construct `account:email?action=manage`.
    pub fn account_email_manage() -> Self {
        account_scope(AccountResource::Email, AccountAction::Manage)
    }

    /// Construct `account:repo`.
    pub fn account_repo_read() -> Self {
        account_scope(AccountResource::Repo, AccountAction::Read)
    }

    /// Construct `account:repo?action=manage`.
    pub fn account_repo_manage() -> Self {
        account_scope(AccountResource::Repo, AccountAction::Manage)
    }

    /// Construct `account:status`.
    pub fn account_status_read() -> Self {
        account_scope(AccountResource::Status, AccountAction::Read)
    }

    /// Construct `account:status?action=manage`.
    pub fn account_status_manage() -> Self {
        account_scope(AccountResource::Status, AccountAction::Manage)
    }

    /// Construct `repo:*` with all repository actions.
    pub fn repo_all() -> Self {
        Scope::repo_nsid_actions(None, all_repo_actions())
    }

    /// Construct a repo scope for all actions on one collection.
    pub fn repo_collection(collection: impl AsRef<str>) -> Result<Self, ParseError> {
        Ok(Scope::repo_nsid_actions(
            Some(Nsid::new_owned(collection)?),
            all_repo_actions(),
        ))
    }

    /// Construct a repo scope granting create on one collection.
    pub fn repo_create(collection: impl AsRef<str>) -> Result<Self, ParseError> {
        Ok(Scope::repo_nsid_actions(
            Some(Nsid::new_owned(collection)?),
            [RepoAction::Create],
        ))
    }

    /// Construct a repo scope granting update on one collection.
    pub fn repo_update(collection: impl AsRef<str>) -> Result<Self, ParseError> {
        Ok(Scope::repo_nsid_actions(
            Some(Nsid::new_owned(collection)?),
            [RepoAction::Update],
        ))
    }

    /// Construct a repo scope granting delete on one collection.
    pub fn repo_delete(collection: impl AsRef<str>) -> Result<Self, ParseError> {
        Ok(Scope::repo_nsid_actions(
            Some(Nsid::new_owned(collection)?),
            [RepoAction::Delete],
        ))
    }

    /// Construct `rpc:*`.
    pub fn rpc_all() -> Self {
        let mut lxm = BTreeSet::new();
        lxm.insert(RpcLexicon::All);
        let mut aud = BTreeSet::new();
        aud.insert(RpcAudience::All);
        Scope::Rpc(RpcScope { lxm, aud })
    }

    /// Construct an RPC scope for one XRPC method NSID and all audiences.
    pub fn rpc(lxm: impl AsRef<str>) -> Result<Self, ParseError> {
        Ok(Scope::rpc_nsid(Nsid::new_owned(lxm)?))
    }

    /// Construct an RPC scope for one XRPC method NSID and one DID audience.
    pub fn rpc_aud(lxm: impl AsRef<str>, aud: impl AsRef<str>) -> Result<Self, ParseError> {
        Ok(Scope::rpc_nsid_aud(
            Nsid::new_owned(lxm)?,
            DidService::new_owned(aud)?,
        ))
    }

    /// Construct an RPC scope from an XRPC request type.
    pub fn rpc_request<R: XrpcRequest>() -> Self {
        Scope::rpc_nsid(unsafe { Nsid::unchecked(SmolStr::new_static(R::NSID)) })
    }

    /// Construct an RPC scope from an XRPC request type and one DID audience.
    pub fn rpc_request_aud<R: XrpcRequest>(aud: impl AsRef<str>) -> Result<Self, ParseError> {
        Ok(Scope::rpc_nsid_aud(
            unsafe { Nsid::unchecked(SmolStr::new_static(R::NSID)) },
            DidService::new_owned(aud)?,
        ))
    }

    /// Construct an `include:<nsid>` permission-set scope.
    pub fn include(nsid: impl AsRef<str>) -> Result<Self, ParseError> {
        Ok(Scope::include_nsid(Nsid::new_owned(nsid)?, None))
    }

    /// Construct an `include:<nsid>?aud=<audience>` permission-set scope.
    pub fn include_aud(nsid: impl AsRef<str>, aud: impl AsRef<str>) -> Result<Self, ParseError> {
        let audience = normalize_include_audience(aud.as_ref())?;
        Ok(Scope::include_nsid(Nsid::new_owned(nsid)?, Some(audience)))
    }

    /// Construct a repo scope granting create for a record collection type.
    pub fn repo_create_record<C: Collection>() -> Self {
        Scope::repo_nsid_actions(Some(collection_nsid::<C>()), [RepoAction::Create])
    }

    /// Construct a repo scope granting update for a record collection type.
    pub fn repo_update_record<C: Collection>() -> Self {
        Scope::repo_nsid_actions(Some(collection_nsid::<C>()), [RepoAction::Update])
    }

    /// Construct a repo scope granting delete for a record collection type.
    pub fn repo_delete_record<C: Collection>() -> Self {
        Scope::repo_nsid_actions(Some(collection_nsid::<C>()), [RepoAction::Delete])
    }

    /// Construct a repo scope granting all repo actions for a record collection type.
    pub fn repo_all_record<C: Collection>() -> Self {
        Scope::repo_nsid_actions(Some(collection_nsid::<C>()), all_repo_actions())
    }
}

impl<S: BosStr + Ord> Scope<S> {
    /// Construct a repo scope for a typed collection NSID and explicit actions.
    pub fn repo_nsid(collection: Nsid<S>, actions: impl IntoIterator<Item = RepoAction>) -> Self {
        Self::repo_nsid_actions(Some(collection), actions)
    }

    fn repo_nsid_actions(
        collection: Option<Nsid<S>>,
        actions: impl IntoIterator<Item = RepoAction>,
    ) -> Self {
        let mut actions: BTreeSet<_> = actions.into_iter().collect();
        if actions.is_empty() {
            actions.extend(all_repo_actions());
        }

        Scope::Repo(RepoScope {
            collection: collection.map_or(RepoCollection::All, RepoCollection::Nsid),
            actions,
        })
    }

    /// Construct an RPC scope for a typed XRPC method NSID and all audiences.
    pub fn rpc_nsid(lxm: Nsid<S>) -> Self {
        let mut lxm_set = BTreeSet::new();
        lxm_set.insert(RpcLexicon::Nsid(lxm));
        let mut aud = BTreeSet::new();
        aud.insert(RpcAudience::All);
        Scope::Rpc(RpcScope { lxm: lxm_set, aud })
    }

    /// Construct an RPC scope for a typed XRPC method NSID and typed DID audience.
    pub fn rpc_nsid_aud(lxm: Nsid<S>, aud: DidService<S>) -> Self {
        let mut lxm_set = BTreeSet::new();
        lxm_set.insert(RpcLexicon::Nsid(lxm));
        let mut aud_set = BTreeSet::new();
        aud_set.insert(RpcAudience::Did(aud));
        Scope::Rpc(RpcScope {
            lxm: lxm_set,
            aud: aud_set,
        })
    }

    /// Construct an include permission-set scope from a typed NSID and optional audience string.
    pub fn include_nsid(nsid: Nsid<S>, audience: Option<S>) -> Self {
        Scope::Include(IncludeScope { nsid, audience })
    }
}

fn account_scope(resource: AccountResource, action: AccountAction) -> Scope<SmolStr> {
    Scope::Account(AccountScope { resource, action })
}

fn all_repo_actions() -> [RepoAction; 3] {
    [RepoAction::Create, RepoAction::Update, RepoAction::Delete]
}

fn collection_nsid<C: Collection>() -> Nsid<SmolStr> {
    unsafe { Nsid::unchecked(SmolStr::new_static(C::NSID)) }
}

fn normalize_include_audience(audience: &str) -> Result<SmolStr, ParseError> {
    if audience.contains('%') {
        let estr = EStr::<Query>::new(audience).ok_or_else(|| {
            ParseError::InvalidResource(
                "include audience has invalid percent-encoding".to_smolstr(),
            )
        })?;
        let decoded = estr.decode().to_string().map_err(|_| {
            ParseError::InvalidResource(
                "include audience contains invalid UTF-8 sequence".to_smolstr(),
            )
        })?;
        validate_include_audience(&decoded)?;
        Ok(SmolStr::new(decoded))
    } else {
        validate_include_audience(audience)?;
        Ok(SmolStr::new(audience))
    }
}

fn validate_include_audience(audience: &str) -> Result<(), ParseError> {
    Ok(validate_did_service(audience)?)
}

/// Account scope attributes
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountScope {
    /// The account resource type
    pub resource: AccountResource,
    /// The action permission level
    pub action: AccountAction,
}

// Re-export from common to avoid duplication and allow use in permission set types
pub use jacquard_common::types::scope_primitives::{AccountAction, AccountResource, RepoAction};

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
    /// Chat transition scope for chat.bsky operations.
    ChatBsky,
}

/// Include scope referencing a permission set NSID with optional audience.
///
/// Represents `include:<nsid>[?aud=<did>]` scopes. The audience is a plain
/// validated string - a DID optionally followed by `#fragment`. Stored in
/// decoded form; `#` is percent-encoded as `%23` on serialisation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IncludeScope<S: BosStr = DefaultStr> {
    /// The permission set NSID.
    pub nsid: Nsid<S>,
    /// Optional audience (decoded form). A DID optionally with a `#fragment`.
    pub audience: Option<S>,
}

impl<S: BosStr> IncludeScope<S> {
    /// Convert to an `IncludeScope` with a different backing type.
    pub fn convert<B: Bos<str> + From<S> + AsRef<str> + FromStaticStr>(self) -> IncludeScope<B> {
        IncludeScope {
            nsid: self.nsid.convert(),
            audience: self.audience.map(Into::into),
        }
    }
}

impl<S: BosStr + IntoStatic> IntoStatic for IncludeScope<S>
where
    S::Output: BosStr,
{
    type Output = IncludeScope<S::Output>;

    fn into_static(self) -> Self::Output {
        IncludeScope {
            nsid: self.nsid.into_static(),
            audience: self.audience.map(|s| s.into_static()),
        }
    }
}

/// Blob scope with mime type constraints
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlobScope<S: BosStr = DefaultStr> {
    /// Accepted mime types
    pub accept: BTreeSet<MimePattern<S>>,
}

impl<S: BosStr + AsRef<str> + Ord> BlobScope<S> {
    /// Convert to a `BlobScope` with a different backing type.
    pub fn convert<B: Bos<str> + From<S> + AsRef<str> + FromStaticStr + Ord>(self) -> BlobScope<B> {
        BlobScope {
            accept: self.accept.into_iter().map(|p| p.convert()).collect(),
        }
    }
}

impl<S: BosStr + IntoStatic> IntoStatic for BlobScope<S>
where
    S::Output: BosStr,
    MimePattern<S::Output>: Ord,
{
    type Output = BlobScope<S::Output>;

    fn into_static(self) -> Self::Output {
        BlobScope {
            accept: self.accept.into_iter().map(|p| p.into_static()).collect(),
        }
    }
}

/// The kind of MIME pattern, without carrying string data.
/// Used by validate_mime_pattern() to return the discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MimePatternKind {
    /// Matches any MIME type.
    All,
    /// Matches any subtype of a given type (e.g., `image/*`).
    TypeWildcard,
    /// Matches an exact MIME type (e.g., `image/png`).
    Exact,
}

/// Validate a MIME pattern string without allocating.
///
/// Returns the pattern kind. Valid patterns:
/// - `*/*` -> `MimePatternKind::All`
/// - `<type>/*` (e.g., `image/*`) -> `MimePatternKind::TypeWildcard`
/// - `<type>/<subtype>` (e.g., `image/png`) -> `MimePatternKind::Exact`
pub(crate) fn validate_mime_pattern(s: &str) -> Result<MimePatternKind, ParseError> {
    if s == "*/*" {
        Ok(MimePatternKind::All)
    } else if let Some(slash) = s.find('/') {
        let type_part = &s[..slash];
        let subtype_part = &s[slash + 1..];
        if type_part.is_empty() || subtype_part.is_empty() {
            return Err(ParseError::InvalidMimeType(s.to_smolstr()));
        }
        if subtype_part == "*" {
            Ok(MimePatternKind::TypeWildcard)
        } else {
            Ok(MimePatternKind::Exact)
        }
    } else {
        Err(ParseError::InvalidMimeType(s.to_smolstr()))
    }
}

/// MIME type pattern for blob scope
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MimePattern<S: BosStr = DefaultStr> {
    /// Match all types
    All,
    /// Match all subtypes of a type (e.g., "image/*")
    TypeWildcard(S),
    /// Exact mime type match
    Exact(S),
}

/// Whether space scope `a` grants every permission space scope `b` requires,
/// mirroring the reference implementation's `SpacePermission::matches`:
/// `read` implies `read_self`, wildcards in type/authority/skey/collection
/// cover concrete values, `self` authority never matches a concrete one, and
/// collection constraints only bind write actions.
fn space_scope_grants<T: BosStr, U: BosStr>(a: &SpaceScope<T>, b: &SpaceScope<U>) -> bool {
    let type_match = match (&a.space_type, &b.space_type) {
        (SpaceType::All, _) => true,
        (SpaceType::Nsid(a_nsid), SpaceType::Nsid(b_nsid)) => a_nsid.as_ref() == b_nsid.as_ref(),
        _ => false,
    };
    if !type_match {
        return false;
    }

    let authority_match = match (&a.authority, &b.authority) {
        (SpaceAuthority::All, _) => true,
        (SpaceAuthority::Self_, SpaceAuthority::Self_) => true,
        (SpaceAuthority::Did(a_did), SpaceAuthority::Did(b_did)) => {
            a_did.as_ref() == b_did.as_ref()
        }
        _ => false,
    };
    if !authority_match {
        return false;
    }

    let skey_match = match (&a.skey, &b.skey) {
        (SpaceSkey::All, _) => true,
        (SpaceSkey::Key(a_key), SpaceSkey::Key(b_key)) => a_key.as_ref() == b_key.as_ref(),
        _ => false,
    };
    if !skey_match {
        return false;
    }

    for action in &b.actions {
        let granted = match action {
            SpaceAction::ReadSelf => {
                a.actions.contains(&SpaceAction::Read) || a.actions.contains(&SpaceAction::ReadSelf)
            }
            SpaceAction::Read => a.actions.contains(&SpaceAction::Read),
            SpaceAction::Create | SpaceAction::Update | SpaceAction::Delete => {
                a.actions.contains(action)
            }
        };
        if !granted {
            return false;
        }
    }

    // Collection constraints bind only write actions: if `b` grants writes,
    // `a` must allow every collection `b` names.
    let b_has_writes = b.actions.iter().any(|act| {
        matches!(
            act,
            SpaceAction::Create | SpaceAction::Update | SpaceAction::Delete
        )
    });
    if b_has_writes {
        let a_allows_all = a.collections.is_empty()
            || a.collections
                .iter()
                .any(|c| matches!(c, SpaceCollection::All));
        let within = if b.collections.is_empty() {
            // `b` grants writes on every collection; only a wildcard (or
            // absent) constraint in `a` can cover that.
            a_allows_all
        } else {
            b.collections.iter().all(|bc| match bc {
                SpaceCollection::All => a_allows_all,
                SpaceCollection::Nsid(b_nsid) => {
                    a_allows_all
                        || a.collections.iter().any(|ac| match ac {
                            SpaceCollection::All => true,
                            SpaceCollection::Nsid(a_nsid) => a_nsid.as_ref() == b_nsid.as_ref(),
                        })
                }
            })
        };
        if !within {
            return false;
        }
    }

    b.manage.is_subset(&a.manage)
}

impl<S: BosStr> MimePattern<S> {
    /// Convert to a `MimePattern` with a different backing type.
    pub fn convert<B: Bos<str> + From<S> + AsRef<str> + FromStaticStr>(self) -> MimePattern<B> {
        match self {
            MimePattern::All => MimePattern::All,
            MimePattern::TypeWildcard(s) => MimePattern::TypeWildcard(s.into()),
            MimePattern::Exact(s) => MimePattern::Exact(s.into()),
        }
    }

    /// Construct a MimePattern without validation.
    ///
    /// # Safety
    ///
    /// The caller must ensure `s` is a valid MIME pattern string
    /// and `kind` matches the pattern. `MimePattern`'s API assumes
    /// the invariant holds. Violating it will produce incorrect
    /// results from downstream operations.
    pub unsafe fn unchecked(s: S, kind: MimePatternKind) -> Self {
        match kind {
            MimePatternKind::All => MimePattern::All,
            MimePatternKind::TypeWildcard => MimePattern::TypeWildcard(s),
            MimePatternKind::Exact => MimePattern::Exact(s),
        }
    }
}

impl<S: BosStr + IntoStatic> IntoStatic for MimePattern<S>
where
    S::Output: BosStr,
{
    type Output = MimePattern<S::Output>;

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
pub struct RepoScope<S: BosStr = DefaultStr> {
    /// Collection NSID or wildcard
    pub collection: RepoCollection<S>,
    /// Allowed actions
    pub actions: BTreeSet<RepoAction>,
}

impl<S: BosStr + Ord> RepoScope<S> {
    /// Convert to a `RepoScope` with a different backing type.
    pub fn convert<B: Bos<str> + From<S> + AsRef<str> + FromStaticStr + Ord>(self) -> RepoScope<B> {
        RepoScope {
            collection: self.collection.convert(),
            actions: self.actions,
        }
    }
}

impl<S: BosStr + IntoStatic> IntoStatic for RepoScope<S>
where
    S::Output: BosStr,
{
    type Output = RepoScope<S::Output>;

    fn into_static(self) -> Self::Output {
        RepoScope {
            collection: self.collection.into_static(),
            actions: self.actions,
        }
    }
}

/// Repository collection identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RepoCollection<S: BosStr = DefaultStr> {
    /// All collections (wildcard)
    All,
    /// Specific collection NSID
    Nsid(Nsid<S>),
}

impl<S: BosStr> RepoCollection<S> {
    /// Convert to an `Nsid` with a different backing type.
    pub fn convert<B: Bos<str> + From<S> + AsRef<str> + FromStaticStr>(self) -> RepoCollection<B> {
        match self {
            RepoCollection::All => RepoCollection::All,
            RepoCollection::Nsid(nsid) => RepoCollection::Nsid(nsid.convert()),
        }
    }
}

impl<S: BosStr + IntoStatic> IntoStatic for RepoCollection<S>
where
    S::Output: BosStr,
{
    type Output = RepoCollection<S::Output>;

    fn into_static(self) -> Self::Output {
        match self {
            RepoCollection::All => RepoCollection::All,
            RepoCollection::Nsid(nsid) => RepoCollection::Nsid(nsid.into_static()),
        }
    }
}

/// Space scope: permission over a permissioned-space type with optional
/// authority, skey, collection, action, and manage constraints.
///
/// Wire form: `space:<type>` with query parameters `authority` (default
/// `self`), `skey` (default `*`), `collection`, `action`, and `manage`,
/// matching the reference implementation's scope grammar.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpaceScope<S: BosStr = DefaultStr> {
    /// Space type NSID or wildcard.
    pub space_type: SpaceType<S>,
    /// Space authority: any, the requesting user's own spaces, or a DID.
    pub authority: SpaceAuthority<S>,
    /// Space key: any or a specific record key.
    pub skey: SpaceSkey<S>,
    /// Collections the grant covers; empty means all.
    pub collections: BTreeSet<SpaceCollection<S>>,
    /// Actions the grant covers.
    pub actions: BTreeSet<SpaceAction>,
    /// Space-management operations the grant covers.
    pub manage: BTreeSet<SpaceManageOp>,
}

/// Space type selector.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SpaceType<S: BosStr = DefaultStr> {
    /// Any space type (`*`).
    All,
    /// A specific space-type NSID.
    Nsid(Nsid<S>),
}

impl<S: BosStr> SpaceType<S> {
    /// Convert to a `SpaceType` with a different backing type.
    pub fn convert<B: Bos<str> + From<S> + AsRef<str> + FromStaticStr>(self) -> SpaceType<B> {
        match self {
            SpaceType::All => SpaceType::All,
            SpaceType::Nsid(nsid) => SpaceType::Nsid(nsid.convert()),
        }
    }
}

impl<S: BosStr + IntoStatic> IntoStatic for SpaceType<S>
where
    S::Output: BosStr,
{
    type Output = SpaceType<S::Output>;

    fn into_static(self) -> Self::Output {
        match self {
            SpaceType::All => SpaceType::All,
            SpaceType::Nsid(nsid) => SpaceType::Nsid(nsid.into_static()),
        }
    }
}

/// Space authority selector.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SpaceAuthority<S: BosStr = DefaultStr> {
    /// Any authority (`*`).
    All,
    /// The requesting user's own spaces (the default; not a grant over
    /// anyone else's spaces).
    Self_,
    /// A specific authority DID.
    Did(jacquard_common::types::did::Did<S>),
}

impl<S: BosStr> SpaceAuthority<S> {
    /// Convert to a `SpaceAuthority` with a different backing type.
    pub fn convert<B: Bos<str> + From<S> + AsRef<str> + FromStaticStr>(self) -> SpaceAuthority<B> {
        match self {
            SpaceAuthority::All => SpaceAuthority::All,
            SpaceAuthority::Self_ => SpaceAuthority::Self_,
            SpaceAuthority::Did(did) => SpaceAuthority::Did(did.convert()),
        }
    }
}

impl<S: BosStr + IntoStatic> IntoStatic for SpaceAuthority<S>
where
    S::Output: BosStr,
{
    type Output = SpaceAuthority<S::Output>;

    fn into_static(self) -> Self::Output {
        match self {
            SpaceAuthority::All => SpaceAuthority::All,
            SpaceAuthority::Self_ => SpaceAuthority::Self_,
            SpaceAuthority::Did(did) => SpaceAuthority::Did(did.into_static()),
        }
    }
}

/// Space key selector.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SpaceSkey<S: BosStr = DefaultStr> {
    /// Any skey (`*`, the default).
    All,
    /// A specific space key (record-key syntax).
    Key(S),
}

impl<S: BosStr> SpaceSkey<S> {
    /// Convert to a `SpaceSkey` with a different backing type.
    pub fn convert<B: Bos<str> + From<S> + AsRef<str> + FromStaticStr>(self) -> SpaceSkey<B> {
        match self {
            SpaceSkey::All => SpaceSkey::All,
            SpaceSkey::Key(key) => SpaceSkey::Key(B::from(key)),
        }
    }
}

impl<S: BosStr + IntoStatic> IntoStatic for SpaceSkey<S>
where
    S::Output: BosStr,
{
    type Output = SpaceSkey<S::Output>;

    fn into_static(self) -> Self::Output {
        match self {
            SpaceSkey::All => SpaceSkey::All,
            SpaceSkey::Key(key) => SpaceSkey::Key(key.into_static()),
        }
    }
}

/// Space collection selector.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpaceCollection<S: BosStr = DefaultStr> {
    /// Any collection (`*`).
    All,
    /// A specific collection NSID.
    Nsid(Nsid<S>),
}

impl<S: BosStr> SpaceCollection<S> {
    /// Convert to a `SpaceCollection` with a different backing type.
    pub fn convert<B: Bos<str> + From<S> + AsRef<str> + FromStaticStr>(self) -> SpaceCollection<B> {
        match self {
            SpaceCollection::All => SpaceCollection::All,
            SpaceCollection::Nsid(nsid) => SpaceCollection::Nsid(nsid.convert()),
        }
    }
}

impl<S: BosStr + IntoStatic> IntoStatic for SpaceCollection<S>
where
    S::Output: BosStr,
{
    type Output = SpaceCollection<S::Output>;

    fn into_static(self) -> Self::Output {
        match self {
            SpaceCollection::All => SpaceCollection::All,
            SpaceCollection::Nsid(nsid) => SpaceCollection::Nsid(nsid.into_static()),
        }
    }
}

/// Actions permitted on a space. `Read` implies `ReadSelf`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpaceAction {
    /// Read the holder's own repo within the space.
    ReadSelf,
    /// Read any repo within the space (implies `ReadSelf`).
    Read,
    /// Create records in the space.
    Create,
    /// Update records in the space.
    Update,
    /// Delete records in the space.
    Delete,
}

/// Space-management operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpaceManageOp {
    /// Create spaces.
    Create,
    /// Update spaces.
    Update,
    /// Delete spaces.
    Delete,
}

impl<S: BosStr + Ord> SpaceScope<S> {
    /// Convert to a `SpaceScope` with a different backing type.
    pub fn convert<B: Bos<str> + From<S> + AsRef<str> + FromStaticStr + Ord>(
        self,
    ) -> SpaceScope<B> {
        SpaceScope {
            space_type: self.space_type.convert(),
            authority: self.authority.convert(),
            skey: self.skey.convert(),
            collections: self.collections.into_iter().map(|c| c.convert()).collect(),
            actions: self.actions,
            manage: self.manage,
        }
    }
}

impl<S: BosStr + IntoStatic + Ord> IntoStatic for SpaceScope<S>
where
    S::Output: BosStr + Ord,
{
    type Output = SpaceScope<S::Output>;

    fn into_static(self) -> Self::Output {
        SpaceScope {
            space_type: self.space_type.into_static(),
            authority: self.authority.into_static(),
            skey: self.skey.into_static(),
            collections: self
                .collections
                .into_iter()
                .map(|c| c.into_static())
                .collect(),
            actions: self.actions,
            manage: self.manage,
        }
    }
}

/// RPC scope with lexicon method and audience constraints
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RpcScope<S: BosStr = DefaultStr> {
    /// Lexicon methods (NSIDs or wildcard)
    pub lxm: BTreeSet<RpcLexicon<S>>,
    /// Audiences (DIDs or wildcard)
    pub aud: BTreeSet<RpcAudience<S>>,
}

impl<S: BosStr + Ord> RpcScope<S> {
    /// Convert to a `RpcScope` with a different backing type.
    pub fn convert<B: Bos<str> + From<S> + AsRef<str> + FromStaticStr + Ord>(self) -> RpcScope<B> {
        RpcScope {
            lxm: self.lxm.into_iter().map(|s| s.convert()).collect(),
            aud: self.aud.into_iter().map(|s| s.convert()).collect(),
        }
    }
}

impl<S: BosStr + IntoStatic> IntoStatic for RpcScope<S>
where
    S::Output: BosStr,
    RpcLexicon<S::Output>: Ord,
    RpcAudience<S::Output>: Ord,
{
    type Output = RpcScope<S::Output>;

    fn into_static(self) -> Self::Output {
        RpcScope {
            lxm: self.lxm.into_iter().map(|s| s.into_static()).collect(),
            aud: self.aud.into_iter().map(|s| s.into_static()).collect(),
        }
    }
}

/// RPC lexicon identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RpcLexicon<S: BosStr = DefaultStr> {
    /// All lexicons (wildcard)
    All,
    /// Specific lexicon NSID
    Nsid(Nsid<S>),
}

impl<S: BosStr> RpcLexicon<S> {
    /// Convert to an `Nsid` with a different backing type.
    pub fn convert<B: Bos<str> + From<S> + AsRef<str> + FromStaticStr>(self) -> RpcLexicon<B> {
        match self {
            RpcLexicon::All => RpcLexicon::All,
            RpcLexicon::Nsid(nsid) => RpcLexicon::Nsid(nsid.convert()),
        }
    }
}

impl<S: BosStr + IntoStatic> IntoStatic for RpcLexicon<S>
where
    S::Output: BosStr,
{
    type Output = RpcLexicon<S::Output>;

    fn into_static(self) -> Self::Output {
        match self {
            RpcLexicon::All => RpcLexicon::All,
            RpcLexicon::Nsid(nsid) => RpcLexicon::Nsid(nsid.into_static()),
        }
    }
}

/// RPC audience identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RpcAudience<S: BosStr = DefaultStr> {
    /// All audiences (wildcard)
    All,
    /// Specific DID with optional service id fragment
    Did(DidService<S>),
}

impl<S: BosStr> RpcAudience<S> {
    /// Convert to an `Nsid` with a different backing type.
    pub fn convert<B: Bos<str> + From<S> + AsRef<str> + FromStaticStr>(self) -> RpcAudience<B> {
        match self {
            RpcAudience::All => RpcAudience::All,
            RpcAudience::Did(did) => RpcAudience::Did(did.convert()),
        }
    }
}

impl<S: BosStr + IntoStatic> IntoStatic for RpcAudience<S>
where
    S::Output: BosStr,
{
    type Output = RpcAudience<S::Output>;

    fn into_static(self) -> Self::Output {
        match self {
            RpcAudience::All => RpcAudience::All,
            RpcAudience::Did(did) => RpcAudience::Did(did.into_static()),
        }
    }
}

/// Byte-range indices for a single scope within a `Scopes` buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScopeIndices {
    pub(crate) start: u16,
    pub(crate) end: u16,
    pub(crate) inner: ScopeInnerIndices,
}

/// Pre-parsed structure of a scope, storing only byte-range indices into the buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScopeInnerIndices {
    Account {
        resource: AccountResource,
        action: AccountAction,
    },
    Identity(IdentityScope),
    Transition(TransitionScope),
    Blob {
        accept: SmallVec<[(u16, u16); 2]>,
    },
    Repo {
        collection: Option<(u16, u16)>,
        actions: RepoActionFlags,
    },
    Rpc {
        lxm: SmallVec<[(u16, u16); 2]>,
        aud: SmallVec<[(u16, u16); 2]>,
    },
    Space {
        space_type: Option<(u16, u16)>,
        authority: SmallVec<[(u16, u16); 2]>,
        skey: SmallVec<[(u16, u16); 2]>,
        collections: SmallVec<[(u16, u16); 2]>,
        actions: SmallVec<[(u16, u16); 2]>,
        manage: SmallVec<[(u16, u16); 2]>,
    },
    Include {
        nsid: (u16, u16),
        audience: Option<IncludeAudience>,
    },
    /// Unit scopes: atproto, openid, profile, email.
    Unit(ScopeKind),
}

/// Discriminant for unit scopes (no string data).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScopeKind {
    Atproto,
    OpenId,
    Profile,
    Email,
}

/// Bitflag representation of repo actions for compact index storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RepoActionFlags(u8);

impl RepoActionFlags {
    pub(crate) const CREATE: u8 = 0b001;
    pub(crate) const UPDATE: u8 = 0b010;
    pub(crate) const DELETE: u8 = 0b100;
    pub(crate) const ALL: u8 = 0b111;

    pub(crate) fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }

    pub(crate) fn to_actions(self) -> BTreeSet<RepoAction> {
        let mut set = BTreeSet::new();
        if self.contains(Self::CREATE) {
            set.insert(RepoAction::Create);
        }
        if self.contains(Self::UPDATE) {
            set.insert(RepoAction::Update);
        }
        if self.contains(Self::DELETE) {
            set.insert(RepoAction::Delete);
        }
        set
    }
}

/// Audience encoding state for include scope indices.
///
/// Both variants store byte ranges into the buffer. The discriminant
/// tells `grants()` whether to decode before comparing, and tells
/// `to_string_normalized()` whether the raw form needs encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IncludeAudience {
    /// Audience in buffer is already decoded (no percent-encoding).
    /// `grants()` can compare directly. Serialisation must encode `#` -> `%23`.
    Plain(u16, u16),
    /// Audience in buffer contains percent-encoding (e.g., `%23`).
    /// `grants()` must decode before comparing. Serialisation can pass through.
    Encoded(u16, u16),
}

/// Iterator over scopes in a `Scopes<S>` container.
pub struct ScopesIter<'i, 'o> {
    buffer: &'o str,
    indices: std::slice::Iter<'i, ScopeIndices>,
}

impl<'i, 'o> Iterator for ScopesIter<'i, 'o> {
    type Item = Scope<&'o str>;

    fn next(&mut self) -> Option<Scope<&'o str>> {
        self.indices.next().map(|idx| {
            // Safety: indices computed at construction from the buffer.
            unsafe { reconstruct_scope(self.buffer, idx) }
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.indices.size_hint()
    }

    fn count(self) -> usize {
        self.indices.count()
    }
}

impl<'i, 'o> ExactSizeIterator for ScopesIter<'i, 'o> {
    fn len(&self) -> usize {
        self.indices.len()
    }
}

impl<'i, 'o> std::iter::FusedIterator for ScopesIter<'i, 'o> {}

/// A validated container of space-separated OAuth scopes.
///
/// Owns or borrows a single scope string and stores pre-computed byte-range
/// indices. Typed `Scope<&str>` views are reconstructed on demand from the
/// shared buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scopes<S: Bos<str> + AsRef<str> = DefaultStr> {
    buffer: S,
    indices: Vec<ScopeIndices>,
}

impl<S: Bos<str> + AsRef<str>> Scopes<S> {
    /// Parse a space-separated scope string, validate each scope, and
    /// compute byte-range indices.
    ///
    /// Returns an empty `Scopes` for an empty string. Returns an error
    /// if any individual scope is malformed.
    pub fn new(buffer: S) -> Result<Self, ParseError> {
        let s = buffer.as_ref();

        if s.is_empty() {
            return Ok(Scopes {
                buffer,
                indices: Vec::new(),
            });
        }

        // Check u16 limit on buffer length.
        if s.len() > u16::MAX as usize {
            return Err(ParseError::InvalidResource(
                "scope string exceeds u16 byte limit".to_smolstr(),
            ));
        }

        let mut indices = Vec::new();
        let mut pos: usize = 0;

        for token in s.split(' ') {
            if token.is_empty() {
                pos += 1; // Advance past the space.
                continue;
            }

            let start = pos as u16;
            let end = start + token.len() as u16;

            let inner = parse_scope_indices(token, start)?;
            indices.push(ScopeIndices { start, end, inner });

            pos = end as usize + 1; // +1 for the space delimiter.
        }

        // Reduce the set by removing indices for scopes already granted by a broader scope.
        indices = reduce_indices(s, indices)?;

        Ok(Scopes { buffer, indices })
    }

    /// Return the number of scopes in this container.
    pub fn len(&self) -> usize {
        self.indices.len()
    }

    /// Return whether this container is empty.
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Iterate over scopes as `Scope<&'o str>` views borrowing from the buffer.
    pub fn iter<'i, 'o>(&'i self) -> ScopesIter<'i, 'o>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let buffer: &'o str = self.buffer.borrow_or_share();
        ScopesIter {
            buffer,
            indices: self.indices.iter(),
        }
    }

    /// Get a single scope by positional index.
    pub fn get<'i, 'o>(&'i self, index: usize) -> Option<Scope<&'o str>>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let idx = self.indices.get(index)?;
        let buffer: &'o str = self.buffer.borrow_or_share();
        Some(unsafe { reconstruct_scope(buffer, idx) })
    }

    /// Get a single scope with owned `SmolStr` backing, independent
    /// of the buffer's lifetime.
    pub fn get_owned(&self, index: usize) -> Option<Scope<SmolStr>> {
        let idx = self.indices.get(index)?;
        let buffer: &str = self.buffer.as_ref();
        let scope = unsafe { reconstruct_scope(buffer, idx) };
        Some(scope.convert())
    }

    /// Get a single scope with caller-chosen backing type.
    pub fn get_as<B: BosStr + for<'a> From<&'a str>>(&self, index: usize) -> Option<Scope<B>>
    where
        B: Ord,
    {
        let idx = self.indices.get(index)?;
        let buffer: &str = self.buffer.as_ref();
        let scope = unsafe { reconstruct_scope(buffer, idx) };
        Some(scope.convert())
    }

    /// Borrow as `Scopes<&str>`.
    pub fn borrow(&self) -> Scopes<&str> {
        Scopes {
            buffer: self.buffer.as_ref(),
            indices: self.indices.clone(),
        }
    }

    /// Convert to `Scopes` with a different backing type.
    pub fn convert<B: Bos<str> + AsRef<str> + From<S>>(self) -> Scopes<B> {
        Scopes {
            buffer: B::from(self.buffer),
            indices: self.indices,
        }
    }

    /// Produce the sorted, normalized space-separated scope string.
    pub fn to_normalized_string(&self) -> SmolStr {
        if self.indices.is_empty() {
            return SmolStr::default();
        }
        let buffer = self.buffer.as_ref();
        let mut normalized: Vec<SmolStr> = self
            .indices
            .iter()
            .map(|idx| {
                let scope = unsafe { reconstruct_scope(buffer, idx) };
                scope.to_string_normalized()
            })
            .collect();
        normalized.sort();

        let mut result = SmolStrBuilder::new();
        for (i, s) in normalized.iter().enumerate() {
            if i > 0 {
                result.push(' ');
            }
            result.push_str(s);
        }
        result.finish()
    }

    /// Check if the container has a scope that grants the given scope.
    pub fn grants<T: BosStr>(&self, scope: &Scope<T>) -> bool {
        let buffer = self.buffer.as_ref();
        self.indices.iter().any(|idx| {
            let s = unsafe { reconstruct_scope(buffer, idx) };
            s.grants(scope)
        })
    }

    /// Return the raw buffer as a string slice.
    pub fn as_str(&self) -> &str {
        self.buffer.as_ref()
    }
}

impl<S: Bos<str> + AsRef<str>> Serialize for Scopes<S> {
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: serde::Serializer,
    {
        serializer.serialize_str(&self.to_normalized_string())
    }
}

impl<'de, S> Deserialize<'de> for Scopes<S>
where
    S: Bos<str> + AsRef<str> + Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = S::deserialize(deserializer)?;
        Scopes::new(s).map_err(D::Error::custom)
    }
}

impl Scopes<SmolStr> {
    /// Create an empty `Scopes` with `SmolStr` backing.
    pub fn empty() -> Self {
        Scopes {
            buffer: SmolStr::default(),
            indices: Vec::new(),
        }
    }

    /// Construct scopes from typed scope values, reusing normal parsing, validation,
    /// reduction, and indexing.
    pub fn from_scopes<I>(scopes: I) -> Result<Self, ParseError>
    where
        I: IntoIterator<Item = Scope<SmolStr>>,
    {
        let mut builder = SmolStrBuilder::new();
        for (index, scope) in scopes.into_iter().enumerate() {
            if index > 0 {
                builder.push(' ');
            }
            builder.push_str(&scope.to_string_normalized());
        }
        Scopes::new(builder.finish())
    }

    /// Construct the default atproto marker scope set.
    pub fn atproto() -> Self {
        Scopes::new(SmolStr::new_static("atproto")).expect("static atproto scope is valid")
    }

    /// Construct a conservative local-development scope set.
    pub fn local_development() -> Self {
        Scopes::new(SmolStr::new_static("atproto transition:generic"))
            .expect("static local-development scopes are valid")
    }

    /// Start a fluent scope builder.
    pub fn builder() -> ScopesBuilder {
        ScopesBuilder::new()
    }
}

/// Fluent builder for programmatic OAuth scope construction.
#[derive(Debug, Clone, Default)]
pub struct ScopesBuilder {
    scopes: Vec<Scope<SmolStr>>,
}

impl ScopesBuilder {
    /// Create an empty builder.
    pub fn new() -> Self {
        Self { scopes: Vec::new() }
    }

    /// Add an already-constructed scope.
    pub fn scope(mut self, scope: Scope<SmolStr>) -> Self {
        self.scopes.push(scope);
        self
    }

    /// Add `atproto`.
    pub fn atproto(self) -> Self {
        self.scope(Scope::atproto())
    }

    /// Add `openid`.
    pub fn openid(self) -> Self {
        self.scope(Scope::openid())
    }

    /// Add `profile`.
    pub fn profile(self) -> Self {
        self.scope(Scope::profile())
    }

    /// Add `email`.
    pub fn email(self) -> Self {
        self.scope(Scope::email())
    }

    /// Add `transition:generic`.
    pub fn transition_generic(self) -> Self {
        self.scope(Scope::transition_generic())
    }

    /// Add `transition:email`.
    pub fn transition_email(self) -> Self {
        self.scope(Scope::transition_email())
    }

    /// Add `transition:chat.bsky`.
    pub fn transition_chat_bsky(self) -> Self {
        self.scope(Scope::transition_chat_bsky())
    }

    /// Add `identity:handle`.
    pub fn identity_handle(self) -> Self {
        self.scope(Scope::identity_handle())
    }

    /// Add `identity:*`.
    pub fn identity_all(self) -> Self {
        self.scope(Scope::identity_all())
    }

    /// Add `rpc:*`.
    pub fn rpc_all(self) -> Self {
        self.scope(Scope::rpc_all())
    }

    /// Add an RPC scope for one XRPC method NSID.
    pub fn rpc(self, nsid: impl AsRef<str>) -> Result<Self, ParseError> {
        Ok(self.scope(Scope::rpc(nsid)?))
    }

    /// Add an RPC scope for one XRPC request type.
    pub fn rpc_request<R: XrpcRequest>(self) -> Self {
        self.scope(Scope::rpc_request::<R>())
    }

    /// Add an RPC scope for one XRPC request type.
    pub fn rpc_request_aud<R: XrpcRequest>(self, aud: impl AsRef<str>) -> Result<Self, ParseError> {
        Ok(self.scope(Scope::rpc_request_aud::<R>(aud)?))
    }

    /// Add a repo create scope for one collection NSID.
    pub fn repo_create(self, collection: impl AsRef<str>) -> Result<Self, ParseError> {
        Ok(self.scope(Scope::repo_create(collection)?))
    }

    /// Add a repo update scope for one collection NSID.
    pub fn repo_update(self, collection: impl AsRef<str>) -> Result<Self, ParseError> {
        Ok(self.scope(Scope::repo_update(collection)?))
    }

    /// Add a repo delete scope for one collection NSID.
    pub fn repo_delete(self, collection: impl AsRef<str>) -> Result<Self, ParseError> {
        Ok(self.scope(Scope::repo_delete(collection)?))
    }

    /// Add a repo all-actions scope for one collection NSID.
    pub fn repo_collection(self, collection: impl AsRef<str>) -> Result<Self, ParseError> {
        Ok(self.scope(Scope::repo_collection(collection)?))
    }

    /// Add a repo create scope for a collection type.
    pub fn repo_create_record<C: Collection>(self) -> Self {
        self.scope(Scope::repo_create_record::<C>())
    }

    /// Add a repo update scope for a collection type.
    pub fn repo_update_record<C: Collection>(self) -> Self {
        self.scope(Scope::repo_update_record::<C>())
    }

    /// Add a repo delete scope for a collection type.
    pub fn repo_delete_record<C: Collection>(self) -> Self {
        self.scope(Scope::repo_delete_record::<C>())
    }

    /// Add a repo all-actions scope for a collection type.
    pub fn repo_all_record<C: Collection>(self) -> Self {
        self.scope(Scope::repo_all_record::<C>())
    }

    /// Add an include permission-set scope.
    pub fn include(self, nsid: impl AsRef<str>) -> Result<Self, ParseError> {
        Ok(self.scope(Scope::include(nsid)?))
    }

    /// Add an include permission-set scope with an audience.
    pub fn include_aud(
        self,
        nsid: impl AsRef<str>,
        aud: impl AsRef<str>,
    ) -> Result<Self, ParseError> {
        Ok(self.scope(Scope::include_aud(nsid, aud)?))
    }

    /// Validate, reduce, and build the final scope container.
    pub fn build(self) -> Result<Scopes<SmolStr>, ParseError> {
        Scopes::from_scopes(self.scopes)
    }
}

impl<S: Bos<str> + AsRef<str> + Default + FromStaticStr> Default for Scopes<S> {
    fn default() -> Self {
        let buffer = S::from_static("atproto");
        let end = (buffer.as_ref().len() - 1) as u16;
        Scopes {
            buffer,
            indices: vec![ScopeIndices {
                start: 0,
                end,
                inner: ScopeInnerIndices::Unit(ScopeKind::Atproto),
            }],
        }
    }
}

impl<S: Bos<str> + AsRef<str> + IntoStatic> IntoStatic for Scopes<S>
where
    S::Output: Bos<str> + AsRef<str>,
{
    type Output = Scopes<S::Output>;

    fn into_static(self) -> Scopes<S::Output> {
        Scopes {
            buffer: self.buffer.into_static(),
            indices: self.indices,
        }
    }
}

/// Parse a single scope token into index structure.
///
/// `base` is the byte offset of `token` within the outer buffer.
/// All `(u16, u16)` ranges in the returned indices are absolute
/// offsets into the outer buffer, NOT relative to `token`.
fn parse_scope_indices(token: &str, base: u16) -> Result<ScopeInnerIndices, ParseError> {
    // Determine the prefix by checking for known prefixes.
    let prefixes = [
        "account",
        "identity",
        "blob",
        "repo",
        "rpc",
        "space",
        "atproto",
        "transition",
        "include",
        "openid",
        "profile",
        "email",
    ];

    let mut found_prefix = None;
    let mut suffix = None;

    for prefix in &prefixes {
        if let Some(remainder) = token.strip_prefix(prefix) {
            if remainder.is_empty() || remainder.starts_with(':') || remainder.starts_with('?') {
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
    }

    let prefix = found_prefix.ok_or_else(|| {
        ParseError::UnknownPrefix(token[..token.find(':').unwrap_or(token.len())].to_smolstr())
    })?;

    match prefix {
        "account" => parse_account_indices(suffix),
        "identity" => parse_identity_indices(suffix),
        "blob" => parse_blob_indices(token, suffix, base),
        "repo" => parse_repo_indices(token, suffix, base),
        "rpc" => parse_rpc_indices(token, suffix, base),
        "space" => parse_space_indices(token, suffix, base),
        "atproto" => parse_atproto_indices(suffix),
        "transition" => parse_transition_indices(suffix),
        "include" => parse_include_indices(token, suffix, base),
        "openid" => parse_openid_indices(suffix),
        "profile" => parse_profile_indices(suffix),
        "email" => parse_email_indices(suffix),
        _ => Err(ParseError::UnknownPrefix(prefix.to_smolstr())),
    }
}

/// Parse account scope indices.
fn parse_account_indices(suffix: Option<&str>) -> Result<ScopeInnerIndices, ParseError> {
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
        _ => return Err(ParseError::InvalidResource(resource_str.to_smolstr())),
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
            Some(other) => return Err(ParseError::InvalidAction(other.to_smolstr())),
            None => AccountAction::Read,
        }
    } else {
        AccountAction::Read
    };

    Ok(ScopeInnerIndices::Account { resource, action })
}

/// Parse identity scope indices.
fn parse_identity_indices(suffix: Option<&str>) -> Result<ScopeInnerIndices, ParseError> {
    let scope = match suffix {
        Some("handle") => IdentityScope::Handle,
        Some("*") => IdentityScope::All,
        Some(other) => return Err(ParseError::InvalidResource(other.to_smolstr())),
        None => return Err(ParseError::MissingResource),
    };

    Ok(ScopeInnerIndices::Identity(scope))
}

/// Parse transition scope indices.
fn parse_transition_indices(suffix: Option<&str>) -> Result<ScopeInnerIndices, ParseError> {
    let scope = match suffix {
        Some("generic") => TransitionScope::Generic,
        Some("email") => TransitionScope::Email,
        Some("chat.bsky") => TransitionScope::ChatBsky,
        Some(other) => return Err(ParseError::InvalidResource(other.to_smolstr())),
        None => return Err(ParseError::MissingResource),
    };

    Ok(ScopeInnerIndices::Transition(scope))
}

/// Parse atproto scope indices (unit scope, no suffix allowed).
fn parse_atproto_indices(suffix: Option<&str>) -> Result<ScopeInnerIndices, ParseError> {
    if suffix.is_some() {
        return Err(ParseError::InvalidResource(
            "atproto scope does not accept suffixes".to_smolstr(),
        ));
    }
    Ok(ScopeInnerIndices::Unit(ScopeKind::Atproto))
}

/// Parse openid scope indices (unit scope, no suffix allowed).
fn parse_openid_indices(suffix: Option<&str>) -> Result<ScopeInnerIndices, ParseError> {
    if suffix.is_some() {
        return Err(ParseError::InvalidResource(
            "openid scope does not accept suffixes".to_smolstr(),
        ));
    }
    Ok(ScopeInnerIndices::Unit(ScopeKind::OpenId))
}

/// Parse profile scope indices (unit scope, no suffix allowed).
fn parse_profile_indices(suffix: Option<&str>) -> Result<ScopeInnerIndices, ParseError> {
    if suffix.is_some() {
        return Err(ParseError::InvalidResource(
            "profile scope does not accept suffixes".to_smolstr(),
        ));
    }
    Ok(ScopeInnerIndices::Unit(ScopeKind::Profile))
}

/// Parse email scope indices (unit scope, no suffix allowed).
fn parse_email_indices(suffix: Option<&str>) -> Result<ScopeInnerIndices, ParseError> {
    if suffix.is_some() {
        return Err(ParseError::InvalidResource(
            "email scope does not accept suffixes".to_smolstr(),
        ));
    }
    Ok(ScopeInnerIndices::Unit(ScopeKind::Email))
}

/// Parse blob scope indices, storing byte ranges of MIME patterns.
fn parse_blob_indices(
    token: &str,
    suffix: Option<&str>,
    base: u16,
) -> Result<ScopeInnerIndices, ParseError> {
    let mut accept: SmallVec<[(u16, u16); 2]> = SmallVec::new();

    match suffix {
        Some(s) if s.starts_with('?') => {
            let params = parse_query_string(&s[1..]);
            if let Some(values) = params.get("accept") {
                for value in values {
                    validate_mime_pattern(value)?;
                    // Find the byte position of this value in the token.
                    if let Some(pos) = token.find(value) {
                        let start = base + pos as u16;
                        let end = start + value.len() as u16;
                        accept.push((start, end));
                    }
                }
            }
        }
        Some(s) => {
            validate_mime_pattern(s)?;
            let start = base + ("blob:".len() as u16);
            let end = start + s.len() as u16;
            accept.push((start, end));
        }
        None => {
            // Default to all patterns (bare `blob` token).
            // Store empty SmallVec to signal "all wildcards" on reconstruction.
        }
    }

    // Empty accept SmallVec signals MimePattern::All on reconstruction.

    Ok(ScopeInnerIndices::Blob { accept })
}

/// Parse repo scope indices, storing byte range of collection NSID if present.
fn parse_repo_indices(
    token: &str,
    suffix: Option<&str>,
    base: u16,
) -> Result<ScopeInnerIndices, ParseError> {
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
        Some("*") | None => None,
        Some(nsid_str) => {
            jacquard_common::types::nsid::validate_nsid(nsid_str)?;
            // Find position of the NSID in the token.
            if let Some(pos) = token.find(nsid_str) {
                let start = base + pos as u16;
                let end = start + nsid_str.len() as u16;
                Some((start, end))
            } else {
                return Err(ParseError::InvalidResource(nsid_str.to_smolstr()));
            }
        }
    };

    let mut actions = RepoActionFlags(RepoActionFlags::ALL);

    if let Some(params) = params {
        let parsed_params = parse_query_string(params);
        if let Some(values) = parsed_params.get("action") {
            let mut flags = 0u8;
            for value in values {
                match value.as_ref() {
                    "create" => flags |= RepoActionFlags::CREATE,
                    "update" => flags |= RepoActionFlags::UPDATE,
                    "delete" => flags |= RepoActionFlags::DELETE,
                    "*" => flags = RepoActionFlags::ALL,
                    other => return Err(ParseError::InvalidAction(other.to_smolstr())),
                }
            }
            actions = RepoActionFlags(flags);
        }
    }

    Ok(ScopeInnerIndices::Repo {
        collection,
        actions,
    })
}

/// Parse RPC scope indices, storing byte ranges of lexicon and audience values.
fn parse_rpc_indices(
    token: &str,
    suffix: Option<&str>,
    base: u16,
) -> Result<ScopeInnerIndices, ParseError> {
    let mut lxm: SmallVec<[(u16, u16); 2]> = SmallVec::new();
    let mut aud: SmallVec<[(u16, u16); 2]> = SmallVec::new();

    match suffix {
        Some("*") => {
            let wildcard_pos = token.rfind('*').unwrap_or(token.len() - 1);
            let start = base + wildcard_pos as u16;
            lxm.push((start, start + 1));
            aud.push((start, start + 1));
        }
        Some(s) if s.starts_with('?') => {
            let params = parse_query_string(&s[1..]);

            if let Some(values) = params.get("lxm") {
                for value in values {
                    if *value == "*" {
                        if let Some(pos) = token.rfind('*') {
                            let start = base + pos as u16;
                            lxm.push((start, start + 1));
                        }
                    } else {
                        jacquard_common::types::nsid::validate_nsid(value)?;
                        if let Some(pos) = token.find(value) {
                            let start = base + pos as u16;
                            let end = start + value.len() as u16;
                            lxm.push((start, end));
                        }
                    }
                }
            }

            if let Some(values) = params.get("aud") {
                for value in values {
                    if *value == "*" {
                        if let Some(pos) = token.rfind('*') {
                            let start = base + pos as u16;
                            aud.push((start, start + 1));
                        }
                    } else {
                        validate_did_service(value)?;
                        if let Some(pos) = token.find(value) {
                            let start = base + pos as u16;
                            let end = start + value.len() as u16;
                            aud.push((start, end));
                        }
                    }
                }
            }
        }
        Some(s) => {
            // Single NSID, possibly with query params.
            if let Some(pos) = s.find('?') {
                let nsid_str = &s[..pos];
                let params = parse_query_string(&s[pos + 1..]);

                jacquard_common::types::nsid::validate_nsid(nsid_str)?;
                if let Some(token_pos) = token.find(nsid_str) {
                    let start = base + token_pos as u16;
                    let end = start + nsid_str.len() as u16;
                    lxm.push((start, end));
                }

                if let Some(values) = params.get("aud") {
                    for value in values {
                        if *value == "*" {
                            if let Some(pos) = token.rfind('*') {
                                let start = base + pos as u16;
                                aud.push((start, start + 1));
                            }
                        } else {
                            validate_did_service(value)?;
                            if let Some(pos) = token.find(value) {
                                let start = base + pos as u16;
                                let end = start + value.len() as u16;
                                aud.push((start, end));
                            }
                        }
                    }
                }
            } else {
                // Just an NSID, no query params.
                jacquard_common::types::nsid::validate_nsid(s)?;
                if let Some(pos) = token.find(s) {
                    let start = base + pos as u16;
                    let end = start + s.len() as u16;
                    lxm.push((start, end));
                }
                // aud remains empty, which means wildcard on reconstruction.
            }
        }
        None => {
            // Empty suffix, default to all.
            // Leave both lxm and aud empty to signal wildcards on reconstruction.
        }
    }

    // Empty lxm SmallVec signals RpcLexicon::All on reconstruction.
    // Empty aud SmallVec signals RpcAudience::All on reconstruction (already handled).

    Ok(ScopeInnerIndices::Rpc { lxm, aud })
}

/// Parse space scope indices, validating the type NSID and query parameters.
fn parse_space_indices(
    token: &str,
    suffix: Option<&str>,
    base: u16,
) -> Result<ScopeInnerIndices, ParseError> {
    let (type_str, params) = match suffix {
        Some(s) => {
            if let Some(pos) = s.find('?') {
                (&s[..pos], Some(&s[pos + 1..]))
            } else {
                (s, None)
            }
        }
        None => return Err(ParseError::MissingResource),
    };

    let mut space_type = None;
    if type_str != "*" {
        jacquard_common::types::nsid::validate_nsid(type_str)?;
        if let Some(pos) = token.find(type_str) {
            let start = base + pos as u16;
            space_type = Some((start, start + type_str.len() as u16));
        }
    }

    // Index-bearing query parameters. Empty smallvecs signal defaults on
    // reconstruction (`self` authority, `*` skey, default action set).
    let mut authority = SmallVec::new();
    let mut skey = SmallVec::new();
    let mut collections = SmallVec::new();
    let mut actions = SmallVec::new();
    let mut manage = SmallVec::new();

    let record_index = |value: &str, out: &mut SmallVec<[(u16, u16); 2]>| {
        if let Some(pos) = token.find(value) {
            let start = base + pos as u16;
            out.push((start, start + value.len() as u16));
        }
    };

    if let Some(params) = params {
        let parsed = parse_query_string(params);

        if let Some(values) = parsed.get("authority") {
            for value in values {
                match *value {
                    // Defaults contribute no index: an empty smallvec
                    // reconstructs as the `self` default, `*` as any.
                    "self" => {}
                    "*" => record_index(value, &mut authority),
                    other => {
                        validate_did(other)?;
                        record_index(other, &mut authority);
                    }
                }
            }
        }

        if let Some(values) = parsed.get("skey") {
            for value in values {
                if value.is_empty() {
                    return Err(ParseError::InvalidResource(SmolStr::new_static("skey")));
                }
                // The default `*` contributes no index.
                if *value != "*" {
                    record_index(value, &mut skey);
                }
            }
        }

        if let Some(values) = parsed.get("collection") {
            for value in values {
                if *value != "*" {
                    jacquard_common::types::nsid::validate_nsid(value)?;
                }
                record_index(value, &mut collections);
            }
        }

        if let Some(values) = parsed.get("action") {
            for value in values {
                if !matches!(
                    *value,
                    "read" | "read_self" | "create" | "update" | "delete"
                ) {
                    return Err(ParseError::InvalidAction(value.to_smolstr()));
                }
                record_index(value, &mut actions);
            }
        }

        if let Some(values) = parsed.get("manage") {
            for value in values {
                if !matches!(*value, "create" | "update" | "delete") {
                    return Err(ParseError::InvalidAction(value.to_smolstr()));
                }
                record_index(value, &mut manage);
            }
        }
    }

    Ok(ScopeInnerIndices::Space {
        space_type,
        authority,
        skey,
        collections,
        actions,
        manage,
    })
}

/// Parse include scope indices, validating NSID and optional audience.
fn parse_include_indices(
    token: &str,
    suffix: Option<&str>,
    base: u16,
) -> Result<ScopeInnerIndices, ParseError> {
    let (nsid_str, params) = match suffix {
        Some(s) => {
            if let Some(pos) = s.find('?') {
                (&s[..pos], Some(&s[pos + 1..]))
            } else {
                (s, None)
            }
        }
        None => return Err(ParseError::MissingResource),
    };

    // Validate the NSID.
    jacquard_common::types::nsid::validate_nsid(nsid_str)?;

    // Find the NSID's byte position in the token.
    let nsid_pos = token
        .find(nsid_str)
        .ok_or_else(|| ParseError::InvalidResource(nsid_str.to_smolstr()))?;
    let nsid_start = base + nsid_pos as u16;
    let nsid_end = nsid_start + nsid_str.len() as u16;

    let audience = if let Some(params) = params {
        let parsed_params = parse_query_string(params);
        if let Some(values) = parsed_params.get("aud") {
            if let Some(aud_value) = values.first() {
                // Check if value contains percent-encoding.
                let has_encoding = aud_value.contains('%');

                if has_encoding {
                    // Validate and decode the percent-encoded value using fluent-uri.
                    let estr = EStr::<Query>::new(aud_value).ok_or_else(|| {
                        ParseError::InvalidResource(
                            "include audience has invalid percent-encoding".to_smolstr(),
                        )
                    })?;

                    let decoded = estr.decode().to_string().map_err(|_| {
                        ParseError::InvalidResource(
                            "include audience contains invalid UTF-8 sequence".to_smolstr(),
                        )
                    })?;

                    validate_did_service(&decoded)?;
                } else {
                    validate_did_service(aud_value)?;
                }

                // Find the audience's byte position in the token.
                let aud_pos = token
                    .find(aud_value)
                    .ok_or_else(|| ParseError::InvalidResource(aud_value.to_smolstr()))?;
                let aud_start = base + aud_pos as u16;
                let aud_end = aud_start + aud_value.len() as u16;

                if has_encoding {
                    Some(IncludeAudience::Encoded(aud_start, aud_end))
                } else {
                    Some(IncludeAudience::Plain(aud_start, aud_end))
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    Ok(ScopeInnerIndices::Include {
        nsid: (nsid_start, nsid_end),
        audience,
    })
}

/// Reduce scope indices by removing those already granted by broader scopes.
fn reduce_indices(
    buffer: &str,
    indices: Vec<ScopeIndices>,
) -> Result<Vec<ScopeIndices>, ParseError> {
    if indices.is_empty() {
        return Ok(indices);
    }

    // Partition indices by scope kind.
    let mut unit_or_account_or_identity_or_transition: Vec<_> = Vec::new();
    let mut repo_indices: Vec<_> = Vec::new();
    let mut rpc_indices: Vec<_> = Vec::new();
    let mut space_indices: Vec<_> = Vec::new();
    let mut blob_indices: Vec<_> = Vec::new();
    let mut include_indices: Vec<_> = Vec::new();

    for indices in indices.into_iter() {
        match &indices.inner {
            ScopeInnerIndices::Unit(_)
            | ScopeInnerIndices::Account { .. }
            | ScopeInnerIndices::Identity(_)
            | ScopeInnerIndices::Transition(_) => {
                unit_or_account_or_identity_or_transition.push(indices);
            }
            ScopeInnerIndices::Repo { .. } => repo_indices.push(indices),
            ScopeInnerIndices::Rpc { .. } => rpc_indices.push(indices),
            ScopeInnerIndices::Space { .. } => space_indices.push(indices),
            ScopeInnerIndices::Blob { .. } => blob_indices.push(indices),
            ScopeInnerIndices::Include { .. } => include_indices.push(indices),
        }
    }

    // Deduplicate unit/account/identity/transition scopes.
    let mut seen = std::collections::HashSet::new();
    unit_or_account_or_identity_or_transition.retain(|idx| {
        let scope = unsafe { reconstruct_scope(buffer, idx) };
        seen.insert(scope.to_string_normalized())
    });

    // Pairwise reduction for repo scopes.
    repo_indices = reduce_pairwise(buffer, repo_indices);

    // Pairwise reduction for rpc scopes.
    rpc_indices = reduce_pairwise(buffer, rpc_indices);

    // Combine back.
    let mut result = unit_or_account_or_identity_or_transition;
    result.extend(repo_indices);
    result.extend(rpc_indices);
    result.extend(space_indices);
    result.extend(blob_indices);
    result.extend(include_indices);

    Ok(result)
}

/// Perform pairwise reduction on a set of indices, removing those granted by others.
fn reduce_pairwise(buffer: &str, indices: Vec<ScopeIndices>) -> Vec<ScopeIndices> {
    let mut result: Vec<ScopeIndices> = Vec::new();

    for candidate_idx in indices {
        // Reconstruct the candidate scope.
        let candidate = unsafe { reconstruct_scope(buffer, &candidate_idx) };

        // Check if it's already granted by something in result.
        let mut is_granted = false;
        for existing_idx in &result {
            let existing = unsafe { reconstruct_scope(buffer, existing_idx) };
            if existing.grants(&candidate) && existing != candidate {
                is_granted = true;
                break;
            }
        }

        if is_granted {
            continue;
        }

        // Check if it grants any existing scopes.
        let mut indices_to_remove = Vec::new();
        for (i, existing_idx) in result.iter().enumerate() {
            let existing = unsafe { reconstruct_scope(buffer, existing_idx) };
            if candidate.grants(&existing) && candidate != existing {
                indices_to_remove.push(i);
            }
        }

        for i in indices_to_remove.into_iter().rev() {
            result.remove(i);
        }

        // Add candidate if not a duplicate.
        if !result.iter().any(|idx| {
            let existing = unsafe { reconstruct_scope(buffer, idx) };
            existing == candidate
        }) {
            result.push(candidate_idx);
        }
    }

    result
}

/// Reconstruct a typed `Scope<&str>` from pre-computed indices.
///
/// # Safety
///
/// `indices` must have been computed from `buffer` during `Scopes::new()`.
/// All byte ranges in `indices` must be valid for `buffer`.
unsafe fn reconstruct_scope<'a>(buffer: &'a str, indices: &ScopeIndices) -> Scope<&'a str> {
    match &indices.inner {
        ScopeInnerIndices::Unit(kind) => match kind {
            ScopeKind::Atproto => Scope::Atproto,
            ScopeKind::OpenId => Scope::OpenId,
            ScopeKind::Profile => Scope::Profile,
            ScopeKind::Email => Scope::Email,
        },

        ScopeInnerIndices::Account { resource, action } => Scope::Account(AccountScope {
            resource: *resource,
            action: *action,
        }),

        ScopeInnerIndices::Identity(scope) => Scope::Identity(scope.clone()),

        ScopeInnerIndices::Transition(scope) => Scope::Transition(scope.clone()),

        ScopeInnerIndices::Blob { accept } => {
            let mut patterns = BTreeSet::new();
            if accept.is_empty() {
                // Empty accept SmallVec signals MimePattern::All (bare `blob` token).
                patterns.insert(MimePattern::All);
            } else {
                for &(start, end) in accept.iter() {
                    let s = &buffer[start as usize..end as usize];
                    let pattern = if s == "*/*" {
                        MimePattern::All
                    } else if s.ends_with("/*") {
                        MimePattern::TypeWildcard(&s[..s.len() - 2])
                    } else {
                        MimePattern::Exact(s)
                    };
                    patterns.insert(pattern);
                }
            }
            Scope::Blob(BlobScope { accept: patterns })
        }

        ScopeInnerIndices::Repo {
            collection,
            actions,
        } => {
            let collection = match collection {
                None => RepoCollection::All,
                Some((start, end)) => {
                    let s = &buffer[*start as usize..*end as usize];
                    RepoCollection::Nsid(unsafe { Nsid::unchecked(s) })
                }
            };
            Scope::Repo(RepoScope {
                collection,
                actions: actions.to_actions(),
            })
        }

        ScopeInnerIndices::Rpc { lxm, aud } => {
            let mut lxm_set = BTreeSet::new();
            let mut aud_set = BTreeSet::new();

            if lxm.is_empty() {
                // Empty lxm means wildcard (bare `rpc` token).
                lxm_set.insert(RpcLexicon::All);
            } else {
                for &(start, end) in lxm.iter() {
                    let s = &buffer[start as usize..end as usize];
                    if s == "*" {
                        lxm_set.insert(RpcLexicon::All);
                    } else {
                        lxm_set.insert(RpcLexicon::Nsid(unsafe { Nsid::unchecked(s) }));
                    }
                }
            }

            if aud.is_empty() {
                // Empty aud means wildcard.
                aud_set.insert(RpcAudience::All);
            } else {
                for &(start, end) in aud.iter() {
                    let s = &buffer[start as usize..end as usize];
                    if s == "*" {
                        aud_set.insert(RpcAudience::All);
                    } else {
                        aud_set.insert(RpcAudience::Did(unsafe { DidService::unchecked(s) }));
                    }
                }
            }

            Scope::Rpc(RpcScope {
                lxm: lxm_set,
                aud: aud_set,
            })
        }

        ScopeInnerIndices::Space {
            space_type,
            authority,
            skey,
            collections,
            actions,
            manage,
        } => {
            let space_type = match space_type {
                None => SpaceType::All,
                Some((start, end)) => SpaceType::Nsid(unsafe {
                    Nsid::unchecked(&buffer[*start as usize..*end as usize])
                }),
            };

            // Empty authority signals the `self` default; `*` any.
            let authority = match authority.first() {
                None => SpaceAuthority::Self_,
                Some(&(start, end)) => match &buffer[start as usize..end as usize] {
                    "*" => SpaceAuthority::All,
                    s => SpaceAuthority::Did(unsafe {
                        jacquard_common::types::did::Did::unchecked(s)
                    }),
                },
            };

            let skey = match skey.first() {
                None => SpaceSkey::All,
                Some(&(start, end)) => SpaceSkey::Key(&buffer[start as usize..end as usize]),
            };

            let mut collection_set = BTreeSet::new();
            for &(start, end) in collections.iter() {
                let s = &buffer[start as usize..end as usize];
                if s == "*" {
                    collection_set.insert(SpaceCollection::All);
                } else {
                    collection_set.insert(SpaceCollection::Nsid(unsafe { Nsid::unchecked(s) }));
                }
            }

            let parse_action = |s: &str| match s {
                "read" => SpaceAction::Read,
                "read_self" => SpaceAction::ReadSelf,
                "create" => SpaceAction::Create,
                "update" => SpaceAction::Update,
                _ => SpaceAction::Delete,
            };
            let parse_manage = |s: &str| match s {
                "create" => SpaceManageOp::Create,
                "update" => SpaceManageOp::Update,
                _ => SpaceManageOp::Delete,
            };

            let mut action_set = BTreeSet::new();
            for &(start, end) in actions.iter() {
                action_set.insert(parse_action(&buffer[start as usize..end as usize]));
            }
            let mut manage_set = BTreeSet::new();
            for &(start, end) in manage.iter() {
                manage_set.insert(parse_manage(&buffer[start as usize..end as usize]));
            }

            // The reference grammar defaults an action-less grant to the
            // standard action set (read implied by the write actions).
            if action_set.is_empty() && manage_set.is_empty() {
                action_set.insert(SpaceAction::Read);
                action_set.insert(SpaceAction::Create);
                action_set.insert(SpaceAction::Update);
                action_set.insert(SpaceAction::Delete);
            }

            Scope::Space(SpaceScope {
                space_type,
                authority,
                skey,
                collections: collection_set,
                actions: action_set,
                manage: manage_set,
            })
        }

        ScopeInnerIndices::Include { nsid, audience } => {
            let (ns, ne) = *nsid;
            let nsid_str = &buffer[ns as usize..ne as usize];

            let aud = match audience {
                None => None,
                Some(IncludeAudience::Plain(start, end))
                | Some(IncludeAudience::Encoded(start, end)) => {
                    Some(&buffer[*start as usize..*end as usize])
                }
            };

            Scope::Include(IncludeScope {
                nsid: unsafe { Nsid::unchecked(nsid_str) },
                audience: aud,
            })
        }
    }
}

impl<S: BosStr + Ord> Scope<S> {
    /// Convert to a `Scope` with a different backing type.
    pub fn convert<B: Bos<str> + From<S> + AsRef<str> + FromStaticStr + Ord>(self) -> Scope<B> {
        match self {
            Scope::Account(scope) => Scope::Account(scope),
            Scope::Identity(scope) => Scope::Identity(scope),
            Scope::Blob(scope) => Scope::Blob(scope.convert()),
            Scope::Repo(scope) => Scope::Repo(scope.convert()),
            Scope::Rpc(scope) => Scope::Rpc(scope.convert()),
            Scope::Space(scope) => Scope::Space(scope.convert()),
            Scope::Atproto => Scope::Atproto,
            Scope::Transition(scope) => Scope::Transition(scope),
            Scope::Include(scope) => Scope::Include(scope.convert()),
            Scope::OpenId => Scope::OpenId,
            Scope::Profile => Scope::Profile,
            Scope::Email => Scope::Email,
        }
    }

    /// Parse a scope from a string
    pub fn parse<'a>(s: &'a str) -> Result<Self, ParseError>
    where
        S: FromStr,
        <S as FromStr>::Err: core::fmt::Debug,
    {
        // Determine the prefix first by checking for known prefixes
        let prefixes = [
            "account",
            "identity",
            "blob",
            "repo",
            "rpc",
            "space",
            "atproto",
            "transition",
            "include",
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
            ParseError::UnknownPrefix(s[..end].to_smolstr())
        })?;

        match prefix {
            "account" => Self::parse_account(suffix),
            "identity" => Self::parse_identity(suffix),
            "blob" => Self::parse_blob(suffix),
            "repo" => Self::parse_repo(suffix),
            "rpc" => Self::parse_rpc(suffix),
            "space" => Self::parse_space(suffix),
            "atproto" => Self::parse_atproto(suffix),
            "transition" => Self::parse_transition(suffix),
            "include" => Self::parse_include(suffix),
            "openid" => Self::parse_openid(suffix),
            "profile" => Self::parse_profile(suffix),
            "email" => Self::parse_email(suffix),
            _ => Err(ParseError::UnknownPrefix(prefix.to_smolstr())),
        }
    }

    fn parse_space<'a>(suffix: Option<&'a str>) -> Result<Self, ParseError>
    where
        S: FromStr,
    {
        let (type_str, params) = match suffix {
            Some(s) => {
                if let Some(pos) = s.find('?') {
                    (&s[..pos], Some(&s[pos + 1..]))
                } else {
                    (s, None)
                }
            }
            None => return Err(ParseError::MissingResource),
        };

        let space_type = match type_str {
            "*" => SpaceType::All,
            nsid => SpaceType::Nsid(Nsid::from_str(nsid)?),
        };

        let mut authority = SpaceAuthority::Self_;
        let mut skey = SpaceSkey::All;
        let mut collections = BTreeSet::new();
        let mut actions = BTreeSet::new();
        let mut manage = BTreeSet::new();

        if let Some(params) = params {
            let parsed = parse_query_string(params);

            if let Some(values) = parsed.get("authority") {
                for value in values {
                    authority = match *value {
                        "*" => SpaceAuthority::All,
                        "self" => SpaceAuthority::Self_,
                        did => {
                            SpaceAuthority::Did(jacquard_common::types::did::Did::from_str(did)?)
                        }
                    };
                }
            }

            if let Some(values) = parsed.get("skey") {
                for value in values {
                    if value.is_empty() {
                        return Err(ParseError::InvalidResource(SmolStr::new_static("skey")));
                    }
                    skey = match *value {
                        "*" => SpaceSkey::All,
                        key => SpaceSkey::Key(
                            S::from_str(key)
                                .map_err(|_| ParseError::InvalidResource(key.to_smolstr()))?,
                        ),
                    };
                }
            }

            if let Some(values) = parsed.get("collection") {
                for value in values {
                    if *value == "*" {
                        collections.insert(SpaceCollection::All);
                    } else {
                        collections.insert(SpaceCollection::Nsid(Nsid::from_str(*value)?));
                    }
                }
            }

            if let Some(values) = parsed.get("action") {
                for value in values {
                    let action = match *value {
                        "read" => SpaceAction::Read,
                        "read_self" => SpaceAction::ReadSelf,
                        "create" => SpaceAction::Create,
                        "update" => SpaceAction::Update,
                        "delete" => SpaceAction::Delete,
                        other => return Err(ParseError::InvalidAction(other.to_smolstr())),
                    };
                    actions.insert(action);
                }
            }

            if let Some(values) = parsed.get("manage") {
                for value in values {
                    let op = match *value {
                        "create" => SpaceManageOp::Create,
                        "update" => SpaceManageOp::Update,
                        "delete" => SpaceManageOp::Delete,
                        other => return Err(ParseError::InvalidAction(other.to_smolstr())),
                    };
                    manage.insert(op);
                }
            }
        }

        if actions.is_empty() && manage.is_empty() {
            actions.insert(SpaceAction::Read);
            actions.insert(SpaceAction::Create);
            actions.insert(SpaceAction::Update);
            actions.insert(SpaceAction::Delete);
        }

        Ok(Scope::Space(SpaceScope {
            space_type,
            authority,
            skey,
            collections,
            actions,
            manage,
        }))
    }

    fn parse_account(suffix: Option<&str>) -> Result<Self, ParseError> {
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
            _ => return Err(ParseError::InvalidResource(resource_str.to_smolstr())),
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
                Some(other) => return Err(ParseError::InvalidAction(other.to_smolstr())),
                None => AccountAction::Read,
            }
        } else {
            AccountAction::Read
        };

        Ok(Scope::Account(AccountScope { resource, action }))
    }

    fn parse_identity(suffix: Option<&str>) -> Result<Self, ParseError> {
        let scope = match suffix {
            Some("handle") => IdentityScope::Handle,
            Some("*") => IdentityScope::All,
            Some(other) => return Err(ParseError::InvalidResource(other.to_smolstr())),
            None => return Err(ParseError::MissingResource),
        };

        Ok(Scope::Identity(scope))
    }

    fn parse_blob<'a>(suffix: Option<&'a str>) -> Result<Self, ParseError>
    where
        S: FromStr,
        <S as FromStr>::Err: core::fmt::Debug,
    {
        let mut accept: BTreeSet<MimePattern<S>> = BTreeSet::new();

        match suffix {
            Some(s) if s.starts_with('?') => {
                let params = parse_query_string(&s[1..]);
                if let Some(values) = params.get("accept") {
                    for value in values {
                        accept.insert(MimePattern::from_str(*value)?);
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

    fn parse_repo<'a>(suffix: Option<&'a str>) -> Result<Self, ParseError>
    where
        S: FromStr,
    {
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
            Some(nsid) => RepoCollection::Nsid(Nsid::from_str(nsid)?),
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
                        other => return Err(ParseError::InvalidAction(other.to_smolstr())),
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

    fn parse_rpc<'a>(suffix: Option<&'a str>) -> Result<Self, ParseError>
    where
        S: FromStr,
    {
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
                        if *value == "*" {
                            lxm.insert(RpcLexicon::All);
                        } else {
                            lxm.insert(RpcLexicon::Nsid(Nsid::from_str(*value)?));
                        }
                    }
                }

                if let Some(values) = params.get("aud") {
                    for value in values {
                        if *value == "*" {
                            aud.insert(RpcAudience::All);
                        } else {
                            aud.insert(RpcAudience::Did(DidService::from_str(*value)?));
                        }
                    }
                }
            }
            Some(s) => {
                // Check if there's a query string in the suffix
                if let Some(pos) = s.find('?') {
                    let nsid = &s[..pos];
                    let params = parse_query_string(&s[pos + 1..]);

                    lxm.insert(RpcLexicon::Nsid(Nsid::from_str(nsid)?));

                    if let Some(values) = params.get("aud") {
                        for value in values {
                            if *value == "*" {
                                aud.insert(RpcAudience::All);
                            } else {
                                aud.insert(RpcAudience::Did(DidService::from_str(*value)?));
                            }
                        }
                    }
                } else {
                    lxm.insert(RpcLexicon::Nsid(Nsid::from_str(s)?));
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
                "atproto scope does not accept suffixes".to_smolstr(),
            ));
        }
        Ok(Scope::Atproto)
    }

    fn parse_transition(suffix: Option<&str>) -> Result<Self, ParseError> {
        let scope = match suffix {
            Some("generic") => TransitionScope::Generic,
            Some("email") => TransitionScope::Email,
            Some("chat.bsky") => TransitionScope::ChatBsky,
            Some(other) => return Err(ParseError::InvalidResource(other.to_smolstr())),
            None => return Err(ParseError::MissingResource),
        };

        Ok(Scope::Transition(scope))
    }

    fn parse_include<'a>(suffix: Option<&'a str>) -> Result<Self, ParseError>
    where
        S: FromStr,
    {
        let (nsid_str, params) = match suffix {
            Some(s) => {
                if let Some(pos) = s.find('?') {
                    (&s[..pos], Some(&s[pos + 1..]))
                } else {
                    (s, None)
                }
            }
            None => return Err(ParseError::MissingResource),
        };

        let audience = if let Some(params) = params {
            let parsed_params = parse_query_string(params);
            parsed_params
                .get("aud")
                .and_then(|values| values.first())
                .map(|audience| {
                    let normalized = normalize_include_audience(audience)?;
                    S::from_str(normalized.as_str()).map_err(|_| {
                        ParseError::InvalidResource("invalid include audience".to_smolstr())
                    })
                })
                .transpose()?
        } else {
            None
        };

        Ok(Scope::Include(IncludeScope {
            nsid: Nsid::from_str(nsid_str)?,
            audience,
        }))
    }

    fn parse_openid(suffix: Option<&str>) -> Result<Self, ParseError> {
        if suffix.is_some() {
            return Err(ParseError::InvalidResource(
                "openid scope does not accept suffixes".to_smolstr(),
            ));
        }
        Ok(Scope::OpenId)
    }

    fn parse_profile(suffix: Option<&str>) -> Result<Self, ParseError> {
        if suffix.is_some() {
            return Err(ParseError::InvalidResource(
                "profile scope does not accept suffixes".to_smolstr(),
            ));
        }
        Ok(Scope::Profile)
    }

    fn parse_email(suffix: Option<&str>) -> Result<Self, ParseError> {
        if suffix.is_some() {
            return Err(ParseError::InvalidResource(
                "email scope does not accept suffixes".to_smolstr(),
            ));
        }
        Ok(Scope::Email)
    }
}

impl<S: BosStr + Ord> Scope<S> {
    /// Convert the scope to its normalized string representation
    pub fn to_string_normalized(&self) -> SmolStr {
        match self {
            Scope::Account(scope) => {
                let resource = match scope.resource {
                    AccountResource::Email => "email",
                    AccountResource::Repo => "repo",
                    AccountResource::Status => "status",
                };

                match scope.action {
                    AccountAction::Read => format_smolstr!("account:{}", resource),
                    AccountAction::Manage => format_smolstr!("account:{}?action=manage", resource),
                }
            }
            Scope::Identity(scope) => match scope {
                IdentityScope::Handle => "identity:handle".to_smolstr(),
                IdentityScope::All => "identity:*".to_smolstr(),
            },
            Scope::Blob(scope) => {
                if scope.accept.len() == 1 {
                    if let Some(pattern) = scope.accept.iter().next() {
                        match pattern {
                            MimePattern::All => "blob:*/*".to_smolstr(),
                            MimePattern::TypeWildcard(t) => {
                                format_smolstr!("blob:{}/*", t.as_ref())
                            }
                            MimePattern::Exact(mime) => format_smolstr!("blob:{}", mime.as_ref()),
                        }
                    } else {
                        "blob:*/*".to_smolstr()
                    }
                } else {
                    let mut params = Vec::new();
                    for pattern in &scope.accept {
                        match pattern {
                            MimePattern::All => params.push("accept=*/*".to_smolstr()),
                            MimePattern::TypeWildcard(t) => {
                                params.push(format_smolstr!("accept={}/*", t.as_ref()))
                            }
                            MimePattern::Exact(mime) => {
                                params.push(format_smolstr!("accept={}", mime.as_ref()))
                            }
                        }
                    }
                    params.sort();
                    format_smolstr!("blob?{}", params.join("&"))
                }
            }
            Scope::Repo(scope) => {
                let collection = match &scope.collection {
                    RepoCollection::All => "*",
                    RepoCollection::Nsid(nsid) => nsid,
                };

                if scope.actions.len() == 3 {
                    format_smolstr!("repo:{}", collection)
                } else {
                    let mut params = Vec::new();
                    for action in &scope.actions {
                        match action {
                            RepoAction::Create => params.push("action=create"),
                            RepoAction::Update => params.push("action=update"),
                            RepoAction::Delete => params.push("action=delete"),
                        }
                    }
                    format_smolstr!("repo:{}?{}", collection, params.join("&"))
                }
            }
            Scope::Space(scope) => {
                let space_type = match &scope.space_type {
                    SpaceType::All => "*",
                    SpaceType::Nsid(nsid) => nsid,
                };
                let mut params = Vec::new();
                match &scope.authority {
                    SpaceAuthority::All => params.push("authority=*".to_smolstr()),
                    SpaceAuthority::Self_ => {}
                    SpaceAuthority::Did(did) => params.push(format_smolstr!("authority={}", did)),
                }
                if let SpaceSkey::Key(key) = &scope.skey {
                    params.push(format_smolstr!("skey={}", key.as_ref()));
                }
                for collection in &scope.collections {
                    match collection {
                        SpaceCollection::All => params.push("collection=*".to_smolstr()),
                        SpaceCollection::Nsid(nsid) => {
                            params.push(format_smolstr!("collection={}", nsid))
                        }
                    }
                }
                for action in &scope.actions {
                    let name = match action {
                        SpaceAction::ReadSelf => "read_self",
                        SpaceAction::Read => "read",
                        SpaceAction::Create => "create",
                        SpaceAction::Update => "update",
                        SpaceAction::Delete => "delete",
                    };
                    params.push(format_smolstr!("action={}", name));
                }
                for op in &scope.manage {
                    let name = match op {
                        SpaceManageOp::Create => "create",
                        SpaceManageOp::Update => "update",
                        SpaceManageOp::Delete => "delete",
                    };
                    params.push(format_smolstr!("manage={}", name));
                }
                if params.is_empty() {
                    format_smolstr!("space:{}", space_type)
                } else {
                    params.sort();
                    format_smolstr!("space:{}?{}", space_type, params.join("&"))
                }
            }
            Scope::Rpc(scope) => {
                if scope.lxm.len() == 1
                    && scope.lxm.contains(&RpcLexicon::All)
                    && scope.aud.len() == 1
                    && scope.aud.contains(&RpcAudience::All)
                {
                    "rpc:*".to_smolstr()
                } else if scope.lxm.len() == 1
                    && scope.aud.len() == 1
                    && scope.aud.contains(&RpcAudience::All)
                {
                    if let Some(lxm) = scope.lxm.iter().next() {
                        match lxm {
                            RpcLexicon::All => "rpc:*".to_smolstr(),
                            RpcLexicon::Nsid(nsid) => format_smolstr!("rpc:{}", nsid),
                        }
                    } else {
                        "rpc:*".to_smolstr()
                    }
                } else {
                    let mut params = Vec::new();

                    for lxm in &scope.lxm {
                        match lxm {
                            RpcLexicon::All => params.push("lxm=*".to_smolstr()),
                            RpcLexicon::Nsid(nsid) => params.push(format_smolstr!("lxm={}", nsid)),
                        }
                    }

                    for aud in &scope.aud {
                        match aud {
                            RpcAudience::All => params.push("aud=*".to_smolstr()),
                            RpcAudience::Did(did) => params.push(format_smolstr!("aud={}", did)),
                        }
                    }

                    params.sort();

                    if params.is_empty() {
                        "rpc:*".to_smolstr()
                    } else {
                        format_smolstr!("rpc?{}", params.join("&"))
                    }
                }
            }
            Scope::Include(scope) => {
                if let Some(ref aud) = scope.audience {
                    // Encode audience using fluent-uri Query encoder.
                    // '#' is not in the Query table, so it gets encoded as %23.
                    // DID-safe characters (:, ., etc.) are in the Query table
                    // and pass through unencoded.
                    let mut encoded = EString::<EncQuery>::new();
                    encoded.encode_str::<EncQuery>(aud.as_ref());
                    format_smolstr!("include:{}?aud={}", scope.nsid, encoded.as_str())
                } else {
                    format_smolstr!("include:{}", scope.nsid)
                }
            }
            Scope::Atproto => "atproto".to_smolstr(),
            Scope::Transition(scope) => match scope {
                TransitionScope::Generic => "transition:generic".to_smolstr(),
                TransitionScope::Email => "transition:email".to_smolstr(),
                TransitionScope::ChatBsky => "transition:chat.bsky".to_smolstr(),
            },
            Scope::OpenId => "openid".to_smolstr(),
            Scope::Profile => "profile".to_smolstr(),
            Scope::Email => "email".to_smolstr(),
        }
    }

    /// Check if this scope grants the permissions of another scope
    pub fn grants<T: BosStr>(&self, other: &Scope<T>) -> bool {
        match (self, other) {
            // Atproto only grants itself
            (Scope::Atproto, Scope::Atproto) => true,
            (Scope::Atproto, _) => false,
            // Nothing else grants atproto
            (_, Scope::Atproto) => false,
            // Transition scopes only grant themselves
            (Scope::Transition(a), Scope::Transition(b)) => a == b,
            // Other scopes don't grant transition scopes
            (_, Scope::Transition(_)) => false,
            (Scope::Transition(_), _) => false,
            // Include scopes only grant exact match (opaque until resolved).
            (Scope::Include(a), Scope::Include(b)) => {
                a.nsid.as_ref() == b.nsid.as_ref()
                    && match (&a.audience, &b.audience) {
                        (Some(a_aud), Some(b_aud)) => a_aud.as_ref() == b_aud.as_ref(),
                        (None, None) => true,
                        _ => false,
                    }
            }
            (_, Scope::Include(_)) => false,
            (Scope::Include(_), _) => false,
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
            (Scope::Space(a), Scope::Space(b)) => space_scope_grants(a, b),
            (Scope::Repo(a), Scope::Repo(b)) => {
                let collection_match = match (&a.collection, &b.collection) {
                    (RepoCollection::All, _) => true,
                    (RepoCollection::Nsid(a_nsid), RepoCollection::Nsid(b_nsid)) => {
                        a_nsid.as_ref() == b_nsid.as_ref()
                    }
                    _ => false,
                };

                if !collection_match {
                    return false;
                }

                b.actions.is_subset(&a.actions) || a.actions.len() == 3
            }
            (Scope::Rpc(a), Scope::Rpc(b)) => {
                let lxm_match = if a.lxm.iter().any(|l| matches!(l, RpcLexicon::All)) {
                    true
                } else {
                    b.lxm.iter().all(|b_lxm| match b_lxm {
                        RpcLexicon::All => false,
                        RpcLexicon::Nsid(b_nsid) => a.lxm.iter().any(|a_lxm| match a_lxm {
                            RpcLexicon::All => false,
                            RpcLexicon::Nsid(a_nsid) => a_nsid.as_ref() == b_nsid.as_ref(),
                        }),
                    })
                };

                let aud_match = if a.aud.iter().any(|a| matches!(a, RpcAudience::All)) {
                    true
                } else {
                    b.aud.iter().all(|b_aud| match b_aud {
                        RpcAudience::All => false,
                        RpcAudience::Did(b_did) => a.aud.iter().any(|a_aud| match a_aud {
                            RpcAudience::All => false,
                            RpcAudience::Did(a_did) => a_did.as_ref() == b_did.as_ref(),
                        }),
                    })
                };

                lxm_match && aud_match
            }
            _ => false,
        }
    }
}

impl<S: BosStr> MimePattern<S> {
    fn grants<T: BosStr>(&self, other: &MimePattern<T>) -> bool {
        match (self, other) {
            (MimePattern::All, _) => true,
            (MimePattern::TypeWildcard(a_type), MimePattern::TypeWildcard(b_type)) => {
                // Compare as strings to support cross-type-parameter equality.
                a_type.as_ref() == b_type.as_ref()
            }
            (MimePattern::TypeWildcard(a_type), MimePattern::Exact(b_mime)) => b_mime
                .as_ref()
                .starts_with(format_smolstr!("{}/", a_type.as_ref()).as_str()),
            (MimePattern::Exact(a), MimePattern::Exact(b)) => a.as_ref() == b.as_ref(),
            _ => false,
        }
    }
}

impl<S: BosStr + FromStr> FromStr for MimePattern<S>
where
    <S as FromStr>::Err: core::fmt::Debug,
{
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "*/*" {
            Ok(MimePattern::All)
        } else if let Some(stripped) = s.strip_suffix("/*") {
            Ok(MimePattern::TypeWildcard(S::from_str(stripped).unwrap()))
        } else if s.contains('/') {
            Ok(MimePattern::Exact(S::from_str(s).unwrap()))
        } else {
            Err(ParseError::InvalidMimeType(s.to_smolstr()))
        }
    }
}

impl<'a, S: BosStr + From<&'a str>> TryFrom<&'a str> for MimePattern<S> {
    type Error = ParseError;

    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        if s == "*/*" {
            Ok(MimePattern::All)
        } else if let Some(stripped) = s.strip_suffix("/*") {
            Ok(MimePattern::TypeWildcard(S::from(stripped)))
        } else if s.contains('/') {
            Ok(MimePattern::Exact(S::from(s)))
        } else {
            Err(ParseError::InvalidMimeType(s.to_smolstr()))
        }
    }
}

impl<S: BosStr + Ord> fmt::Display for Scope<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_normalized())
    }
}

/// Parse a query string into a map of keys to lists of values
fn parse_query_string(query: &str) -> BTreeMap<SmolStr, Vec<&str>> {
    let mut params = BTreeMap::new();

    for pair in query.split('&') {
        if let Some(pos) = pair.find('=') {
            let key = &pair[..pos];
            let value = &pair[pos + 1..];
            params
                .entry(key.to_smolstr())
                .or_insert_with(Vec::new)
                .push(value);
        }
    }

    params
}

/// Error type for permission set expansion and conversion
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PermissionSetConversionError {
    /// Unknown identity attribute in permission set
    #[error("unknown identity attribute: {0}")]
    UnknownIdentityAttr(SmolStr),

    /// Unknown account attribute in permission set
    #[error("unknown account attribute: {0}")]
    UnknownAccountAttr(SmolStr),

    /// Invalid MIME pattern in blob permission
    #[error("invalid MIME pattern: {0}")]
    InvalidMimePattern(SmolStr),
}

/// Error type for scope parsing
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum ParseError {
    /// Unknown scope prefix
    UnknownPrefix(SmolStr),
    /// Missing required resource
    MissingResource,
    /// Invalid resource type
    InvalidResource(SmolStr),
    /// Invalid action type
    InvalidAction(SmolStr),
    /// Invalid MIME type
    InvalidMimeType(SmolStr),
    /// An AT Protocol string type (DID, NSID, etc.) failed validation during scope parsing.
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

/// Convert a resolved permission set into its constituent scope values.
///
/// Each permission entry expands to one or more concrete scopes:
/// - Repo: one `Scope::Repo` per collection NSID
/// - Rpc: one `Scope::Rpc` per lxm NSID (with shared aud)
/// - Blob: one `Scope::Blob` with all accept patterns
/// - Identity: `Scope::Identity` based on attr
/// - Account: `Scope::Account` based on attr and action
/// `inherited_audience` is the audience from the `include:` scope's `?aud=`
/// parameter. Passed to RPC permissions with `inherit_aud: true`.
#[cfg(feature = "scope-check")]
pub fn expand_permission_set(
    perm_set: &jacquard_lexicon::lexicon::LexPermissionSet<'static>,
    inherited_audience: Option<&DidService<SmolStr>>,
) -> Result<Vec<Scope<SmolStr>>, PermissionSetConversionError> {
    use jacquard_lexicon::lexicon::{LexPermission, LexPermissionResource};

    let mut scopes = Vec::new();

    for perm in &perm_set.permissions {
        let LexPermission::Permission { resource } = perm;
        match resource {
            LexPermissionResource::Repo { collection, action } => {
                let actions = action
                    .as_ref()
                    .map(|a| a.iter().copied().collect())
                    .unwrap_or_else(|| {
                        let mut all = BTreeSet::new();
                        all.insert(RepoAction::Create);
                        all.insert(RepoAction::Update);
                        all.insert(RepoAction::Delete);
                        all
                    });

                for col_nsid in collection {
                    scopes.push(Scope::Repo(RepoScope {
                        collection: RepoCollection::Nsid(col_nsid.clone().convert()),
                        actions: actions.clone(),
                    }));
                }
            }
            LexPermissionResource::Rpc {
                lxm,
                aud,
                inherit_aud,
            } => {
                // Build the audience set based on priority order
                let mut aud_set = BTreeSet::new();
                if let Some(explicit_aud) = aud {
                    aud_set.insert(RpcAudience::Did(explicit_aud.clone().convert()));
                } else if inherit_aud.unwrap_or(false) && inherited_audience.is_some() {
                    aud_set.insert(RpcAudience::Did(inherited_audience.unwrap().clone()));
                } else {
                    aud_set.insert(RpcAudience::All);
                }

                // Create one RpcScope with all lxm NSIDs and the resolved audience
                let mut lxm_set = BTreeSet::new();
                for lxm_nsid in lxm {
                    lxm_set.insert(RpcLexicon::Nsid(lxm_nsid.clone().convert()));
                }

                if !lxm_set.is_empty() {
                    scopes.push(Scope::Rpc(RpcScope {
                        lxm: lxm_set,
                        aud: aud_set,
                    }));
                }
            }
            LexPermissionResource::Blob { accept, .. } => {
                let mut patterns = BTreeSet::new();
                for mime_type in accept {
                    let pattern_str = mime_type.as_ref();
                    match validate_mime_pattern(pattern_str) {
                        Ok(kind) => {
                            // For TypeWildcard, strip the `/*` suffix before storing.
                            let mime_str = match kind {
                                MimePatternKind::TypeWildcard => {
                                    SmolStr::new(&pattern_str[..pattern_str.len() - 2])
                                }
                                _ => SmolStr::new(pattern_str),
                            };
                            let pattern = unsafe { MimePattern::unchecked(mime_str, kind) };
                            patterns.insert(pattern);
                        }
                        Err(_) => {
                            return Err(PermissionSetConversionError::InvalidMimePattern(
                                pattern_str.to_smolstr(),
                            ));
                        }
                    }
                }

                if !patterns.is_empty() {
                    scopes.push(Scope::Blob(BlobScope { accept: patterns }));
                }
            }
            LexPermissionResource::Identity { attr } => {
                let identity_scope = match attr.as_ref() {
                    "handle" => IdentityScope::Handle,
                    "*" => IdentityScope::All,
                    other => {
                        return Err(PermissionSetConversionError::UnknownIdentityAttr(
                            other.to_smolstr(),
                        ));
                    }
                };
                scopes.push(Scope::Identity(identity_scope));
            }
            LexPermissionResource::Account { attr, action } => {
                let resource = match attr.as_ref() {
                    "email" => AccountResource::Email,
                    "repo" => AccountResource::Repo,
                    "status" => AccountResource::Status,
                    other => {
                        return Err(PermissionSetConversionError::UnknownAccountAttr(
                            other.to_smolstr(),
                        ));
                    }
                };

                // Take the highest privilege level. Manage subsumes Read.
                let act = action
                    .as_ref()
                    .map(|actions| {
                        if actions.contains(&AccountAction::Manage) {
                            AccountAction::Manage
                        } else {
                            AccountAction::Read
                        }
                    })
                    .unwrap_or(AccountAction::Read);

                scopes.push(Scope::Account(AccountScope {
                    resource,
                    action: act,
                }));
            }
        }
    }

    Ok(scopes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "scope-check")]
    use jacquard_common::CowStr;
    use jacquard_common::xrpc::{XrpcMethod, XrpcResp};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    struct TestRequest;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
    #[error("test error")]
    struct TestError;

    struct TestResponse;

    impl XrpcResp for TestResponse {
        const NSID: &'static str = "app.bsky.feed.getTimeline";
        const ENCODING: &'static str = "application/json";
        type Output<S: BosStr> = ();
        type Err = TestError;
    }

    impl XrpcRequest for TestRequest {
        const NSID: &'static str = "app.bsky.feed.getTimeline";
        const METHOD: XrpcMethod = XrpcMethod::Query;
        type Response = TestResponse;
    }

    struct TestCollection;

    impl Collection for TestCollection {
        const NSID: &'static str = "app.bsky.feed.post";
        type Record = TestResponse;
    }

    #[test]
    fn test_scope_constructors() {
        assert_eq!(Scope::atproto().to_string_normalized(), "atproto");
        assert_eq!(
            Scope::identity_handle().to_string_normalized(),
            "identity:handle"
        );
        assert_eq!(
            Scope::account_repo_manage().to_string_normalized(),
            "account:repo?action=manage"
        );
        assert_eq!(Scope::repo_all().to_string_normalized(), "repo:*");
        assert_eq!(
            Scope::repo_create("app.bsky.feed.post")
                .unwrap()
                .to_string_normalized(),
            "repo:app.bsky.feed.post?action=create"
        );
        assert_eq!(
            Scope::rpc("app.bsky.feed.getTimeline")
                .unwrap()
                .to_string_normalized(),
            "rpc:app.bsky.feed.getTimeline"
        );
        assert_eq!(
            Scope::rpc_aud(
                "app.bsky.feed.getTimeline",
                "did:plc:yfvwmnlztr4dwkb7hwz55r2g"
            )
            .unwrap()
            .to_string_normalized(),
            "rpc?aud=did:plc:yfvwmnlztr4dwkb7hwz55r2g&lxm=app.bsky.feed.getTimeline"
        );
        assert_eq!(
            Scope::include_aud("app.bsky.authFull", "did:web:example.com#svc")
                .unwrap()
                .to_string_normalized(),
            "include:app.bsky.authFull?aud=did:web:example.com%23svc"
        );
    }

    #[test]
    fn test_endpoint_and_collection_scope_constructors() {
        assert_eq!(
            Scope::rpc_request::<TestRequest>().to_string_normalized(),
            "rpc:app.bsky.feed.getTimeline"
        );
        assert_eq!(
            Scope::repo_create_record::<TestCollection>().to_string_normalized(),
            "repo:app.bsky.feed.post?action=create"
        );
        assert_eq!(
            Scope::repo_all_record::<TestCollection>().to_string_normalized(),
            "repo:app.bsky.feed.post"
        );
    }

    #[test]
    fn test_scopes_from_scopes_and_builder() {
        let scopes = Scopes::from_scopes([
            Scope::repo_create("app.bsky.feed.post").unwrap(),
            Scope::repo_all(),
            Scope::atproto(),
        ])
        .unwrap();

        assert_eq!(scopes.to_normalized_string(), "atproto repo:*");

        let built = Scopes::builder()
            .atproto()
            .transition_generic()
            .rpc_request::<TestRequest>()
            .repo_create_record::<TestCollection>()
            .include("app.bsky.authFull")
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(
            built.to_normalized_string(),
            "atproto include:app.bsky.authFull repo:app.bsky.feed.post?action=create rpc:app.bsky.feed.getTimeline transition:generic"
        );
    }

    #[test]
    fn test_single_scope_include_parsing() {
        let scope: Scope =
            Scope::parse("include:app.bsky.authFull?aud=did:web:example.com#svc").unwrap();
        assert_eq!(
            scope.to_string_normalized(),
            "include:app.bsky.authFull?aud=did:web:example.com%23svc"
        );
    }

    #[test]
    fn test_account_scope_parsing() {
        let scope: Scope = Scope::parse("account:email").unwrap();
        assert_eq!(
            scope,
            Scope::Account(AccountScope {
                resource: AccountResource::Email,
                action: AccountAction::Read,
            })
        );

        let scope: Scope = Scope::parse("account:repo?action=manage").unwrap();
        assert_eq!(
            scope,
            Scope::Account(AccountScope {
                resource: AccountResource::Repo,
                action: AccountAction::Manage,
            })
        );

        let scope: Scope = Scope::parse("account:status?action=read").unwrap();
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
        let scope: Scope = Scope::parse("identity:handle").unwrap();
        assert_eq!(scope, Scope::Identity(IdentityScope::Handle));

        let scope: Scope = Scope::parse("identity:*").unwrap();
        assert_eq!(scope, Scope::Identity(IdentityScope::All));
    }

    #[test]
    fn test_blob_scope_parsing() {
        let scope: Scope = Scope::parse("blob:*/*").unwrap();
        let mut accept = BTreeSet::new();
        accept.insert(MimePattern::All);
        assert_eq!(scope, Scope::Blob(BlobScope { accept }));

        let scope: Scope<SmolStr> = Scope::parse("blob:image/png").unwrap();
        let mut accept = BTreeSet::new();
        accept.insert(MimePattern::Exact(SmolStr::new_static("image/png")));
        assert_eq!(scope, Scope::Blob(BlobScope { accept }));

        let scope = Scope::parse("blob?accept=image/png&accept=image/jpeg").unwrap();
        let mut accept = BTreeSet::new();
        accept.insert(MimePattern::Exact(SmolStr::new_static("image/png")));
        accept.insert(MimePattern::Exact(SmolStr::new_static("image/jpeg")));
        assert_eq!(scope, Scope::Blob(BlobScope { accept }));

        let scope = Scope::parse("blob:image/*").unwrap();
        let mut accept = BTreeSet::new();
        accept.insert(MimePattern::TypeWildcard(SmolStr::new_static("image")));
        assert_eq!(scope, Scope::Blob(BlobScope { accept }));
    }

    #[test]
    fn test_repo_scope_parsing() {
        let scope: Scope<SmolStr> = Scope::parse("repo:*?action=create").unwrap();
        let mut actions = BTreeSet::new();
        actions.insert(RepoAction::Create);
        assert_eq!(
            scope,
            Scope::Repo(RepoScope {
                collection: RepoCollection::All,
                actions,
            })
        );

        let scope: Scope =
            Scope::parse("repo:app.bsky.feed.post?action=create&action=update").unwrap();
        let mut actions = BTreeSet::new();
        actions.insert(RepoAction::Create);
        actions.insert(RepoAction::Update);
        assert_eq!(
            scope,
            Scope::Repo(RepoScope {
                collection: RepoCollection::Nsid(Nsid::new_owned("app.bsky.feed.post").unwrap()),
                actions,
            })
        );

        let scope: Scope = Scope::parse("repo:app.bsky.feed.post").unwrap();
        let mut actions = BTreeSet::new();
        actions.insert(RepoAction::Create);
        actions.insert(RepoAction::Update);
        actions.insert(RepoAction::Delete);
        assert_eq!(
            scope,
            Scope::Repo(RepoScope {
                collection: RepoCollection::Nsid(Nsid::new_owned("app.bsky.feed.post").unwrap()),
                actions,
            })
        );
    }

    #[test]
    fn test_rpc_scope_parsing() {
        let scope: Scope = Scope::parse("rpc:*").unwrap();
        let mut lxm = BTreeSet::new();
        let mut aud = BTreeSet::new();
        lxm.insert(RpcLexicon::All);
        aud.insert(RpcAudience::All);
        assert_eq!(scope, Scope::Rpc(RpcScope { lxm, aud }));

        let scope: Scope<SmolStr> = Scope::parse("rpc:com.example.service").unwrap();
        let mut lxm = BTreeSet::new();
        let mut aud = BTreeSet::new();
        lxm.insert(RpcLexicon::Nsid(
            Nsid::new_static("com.example.service").unwrap(),
        ));
        aud.insert(RpcAudience::All);
        assert_eq!(scope, Scope::Rpc(RpcScope { lxm, aud }));

        let scope: Scope =
            Scope::parse("rpc:com.example.service?aud=did:plc:yfvwmnlztr4dwkb7hwz55r2g").unwrap();
        let mut lxm = BTreeSet::new();
        let mut aud = BTreeSet::new();
        lxm.insert(RpcLexicon::Nsid(
            Nsid::new_owned("com.example.service").unwrap(),
        ));
        aud.insert(RpcAudience::Did(
            DidService::new_owned("did:plc:yfvwmnlztr4dwkb7hwz55r2g").unwrap(),
        ));
        assert_eq!(scope, Scope::Rpc(RpcScope { lxm, aud }));

        let scope: Scope =
            Scope::parse("rpc?lxm=com.example.method1&lxm=com.example.method2&aud=did:plc:yfvwmnlztr4dwkb7hwz55r2g")
                .unwrap();
        let mut lxm = BTreeSet::new();
        let mut aud = BTreeSet::new();
        lxm.insert(RpcLexicon::Nsid(
            Nsid::new_owned("com.example.method1").unwrap(),
        ));
        lxm.insert(RpcLexicon::Nsid(
            Nsid::new_owned("com.example.method2").unwrap(),
        ));
        aud.insert(RpcAudience::Did(
            DidService::new_owned("did:plc:yfvwmnlztr4dwkb7hwz55r2g").unwrap(),
        ));
        assert_eq!(scope, Scope::Rpc(RpcScope { lxm, aud }));
    }

    #[test]
    fn test_space_scope_parsing_and_round_trip() {
        // Bare type: defaults to self authority, wildcard skey, and the
        // default action set.
        let scope: Scope = Scope::parse("space:dev.jacquard.e2e.space").unwrap();
        match &scope {
            Scope::Space(s) => {
                assert_eq!(
                    s.space_type,
                    SpaceType::Nsid(Nsid::new_owned("dev.jacquard.e2e.space").unwrap())
                );
                assert_eq!(s.authority, SpaceAuthority::Self_);
                assert_eq!(s.skey, SpaceSkey::All);
                assert!(s.actions.contains(&SpaceAction::Read));
                assert!(s.actions.contains(&SpaceAction::Create));
                assert!(s.collections.is_empty());
            }
            other => panic!("expected space scope, got {other:?}"),
        }

        // Fully constrained form round-trips through normalization.
        let input = "space:dev.jacquard.e2e.space?action=read&authority=*";
        let scope: Scope = Scope::parse(input).unwrap();
        assert_eq!(scope.to_string_normalized(), input);

        // Wildcard type and explicit authority/skey.
        let scope: Scope = Scope::parse(
            "space:*?authority=did:web:example.test&skey=main&collection=com.example.post&manage=create&manage=delete",
        )
        .unwrap();
        match &scope {
            Scope::Space(s) => {
                assert_eq!(s.space_type, SpaceType::All);
                assert_eq!(
                    s.authority,
                    SpaceAuthority::Did(
                        jacquard_common::types::did::Did::new_owned("did:web:example.test")
                            .unwrap()
                    )
                );
                assert_eq!(s.skey, SpaceSkey::Key(SmolStr::new("main")));
                assert!(s.collections.contains(&SpaceCollection::Nsid(
                    Nsid::new_owned("com.example.post").unwrap()
                )));
                assert!(s.manage.contains(&SpaceManageOp::Create));
                assert!(s.manage.contains(&SpaceManageOp::Delete));
            }
            other => panic!("expected space scope, got {other:?}"),
        }

        // Default authority and skey are omitted from the normalized form.
        let scope: Scope =
            Scope::parse("space:dev.jacquard.e2e.space?authority=self&skey=*&action=read").unwrap();
        assert_eq!(
            scope.to_string_normalized(),
            "space:dev.jacquard.e2e.space?action=read"
        );

        // Invalid action and invalid authority DID are rejected.
        assert!(Scope::<SmolStr>::parse("space:dev.jacquard.e2e.space?action=manage").is_err());
        assert!(
            Scope::<SmolStr>::parse("space:dev.jacquard.e2e.space?authority=not-a-did").is_err()
        );
        assert!(Scope::<SmolStr>::parse("space:not an nsid").is_err());
        assert!(Scope::<SmolStr>::parse("space").is_err());
    }

    #[test]
    fn test_space_scope_grants() {
        let parse = |s: &str| -> Scope { Scope::parse(s).unwrap() };

        // Read implies read_self.
        let read = parse("space:dev.t.example?authority=*&action=read");
        let read_self = parse("space:dev.t.example?authority=*&action=read_self");
        assert!(read.grants(&read_self));
        assert!(!read_self.grants(&read));

        // Wildcard type and authority cover concrete values.
        let wildcard = parse("space:*?authority=*&action=read");
        let concrete = parse("space:dev.t.example?authority=did:web:example.test&action=read");
        assert!(wildcard.grants(&concrete));
        assert!(!concrete.grants(&wildcard));

        // A different concrete authority never grants.
        let other = parse("space:dev.t.example?authority=did:web:other.test&action=read");
        assert!(!concrete.grants(&other));

        // `self` never grants a concrete authority.
        let self_auth = parse("space:dev.t.example?action=read");
        assert!(!self_auth.grants(&concrete));

        // skey narrowing.
        let skey = parse("space:dev.t.example?authority=*&skey=main&action=read");
        let any_skey = parse("space:dev.t.example?authority=*&action=read");
        assert!(any_skey.grants(&skey));
        assert!(!skey.grants(&any_skey));

        // Collection constraints bind write actions.
        let write_post =
            parse("space:dev.t.example?authority=*&action=create&collection=com.example.post");
        let write_any = parse("space:dev.t.example?authority=*&action=create");
        assert!(write_any.grants(&write_post));
        assert!(!write_post.grants(&write_any));

        // Manage operations.
        let manage = parse("space:dev.t.example?authority=*&manage=create&manage=delete");
        let manage_create = parse("space:dev.t.example?authority=*&manage=create");
        assert!(manage.grants(&manage_create));
        assert!(!manage_create.grants(&manage));

        // Non-space scopes never grant space scopes.
        assert!(!parse("atproto").grants(&concrete));
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
            let scope: Scope = Scope::parse(input).unwrap();
            assert_eq!(scope.to_string_normalized(), expected);
        }
    }

    #[test]
    fn test_account_scope_grants() {
        let manage: Scope = Scope::parse("account:email?action=manage").unwrap();
        let read: Scope = Scope::parse("account:email?action=read").unwrap();
        let other_read: Scope = Scope::parse("account:repo?action=read").unwrap();

        assert!(manage.grants(&read));
        assert!(manage.grants(&manage));
        assert!(!read.grants(&manage));
        assert!(read.grants(&read));
        assert!(!read.grants(&other_read));
    }

    #[test]
    fn test_identity_scope_grants() {
        let all: Scope = Scope::parse("identity:*").unwrap();
        let handle: Scope = Scope::parse("identity:handle").unwrap();

        assert!(all.grants(&handle));
        assert!(all.grants(&all));
        assert!(!handle.grants(&all));
        assert!(handle.grants(&handle));
    }

    #[test]
    fn test_blob_scope_grants() {
        let all: Scope = Scope::parse("blob:*/*").unwrap();
        let image_all: Scope = Scope::parse("blob:image/*").unwrap();
        let image_png: Scope = Scope::parse("blob:image/png").unwrap();
        let text_plain: Scope = Scope::parse("blob:text/plain").unwrap();

        assert!(all.grants(&image_all));
        assert!(all.grants(&image_png));
        assert!(all.grants(&text_plain));
        assert!(image_all.grants(&image_png));
        assert!(!image_all.grants(&text_plain));
        assert!(!image_png.grants(&image_all));
    }

    #[test]
    fn test_repo_scope_grants() {
        let all_all: Scope = Scope::parse("repo:*").unwrap();
        let all_create: Scope = Scope::parse("repo:*?action=create").unwrap();
        let specific_all: Scope = Scope::parse("repo:app.bsky.feed.post").unwrap();
        let specific_create: Scope = Scope::parse("repo:app.bsky.feed.post?action=create").unwrap();
        let other_create: Scope =
            Scope::parse("repo:pub.leaflet.publication?action=create").unwrap();

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
        let all: Scope = Scope::parse("rpc:*").unwrap();
        let specific_lxm: Scope = Scope::parse("rpc:com.example.service").unwrap();
        let specific_both: Scope =
            Scope::parse("rpc:com.example.service?aud=did:example:123").unwrap();

        assert!(all.grants(&specific_lxm));
        assert!(all.grants(&specific_both));
        assert!(specific_lxm.grants(&specific_both));
        assert!(!specific_both.grants(&specific_lxm));
        assert!(!specific_both.grants(&all));
    }

    #[test]
    fn test_cross_scope_grants() {
        let account: Scope = Scope::parse("account:email").unwrap();
        let identity: Scope = Scope::parse("identity:handle").unwrap();

        assert!(!account.grants(&identity));
        assert!(!identity.grants(&account));
    }

    #[test]
    fn test_parse_errors() {
        assert!(matches!(
            Scope::<SmolStr>::parse("unknown:test"),
            Err(ParseError::UnknownPrefix(_))
        ));

        assert!(matches!(
            Scope::<SmolStr>::parse("account"),
            Err(ParseError::MissingResource)
        ));

        assert!(matches!(
            Scope::<SmolStr>::parse("account:invalid"),
            Err(ParseError::InvalidResource(_))
        ));

        assert!(matches!(
            Scope::<SmolStr>::parse("account:email?action=invalid"),
            Err(ParseError::InvalidAction(_))
        ));
    }

    #[test]
    fn test_query_parameter_sorting() {
        let scope = Scope::<SmolStr>::parse(
            "blob?accept=image/png&accept=application/pdf&accept=image/jpeg",
        )
        .unwrap();
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
        let scope = Scope::<SmolStr>::parse("repo:app.bsky.feed.post?action=*").unwrap();
        let mut actions = BTreeSet::new();
        actions.insert(RepoAction::Create);
        actions.insert(RepoAction::Update);
        actions.insert(RepoAction::Delete);
        assert_eq!(
            scope,
            Scope::Repo(RepoScope {
                collection: RepoCollection::Nsid(Nsid::new_owned("app.bsky.feed.post").unwrap()),
                actions,
            })
        );
    }

    #[test]
    fn test_multiple_blob_accepts() {
        let scope = Scope::<SmolStr>::parse("blob?accept=image/*&accept=text/plain").unwrap();
        assert!(scope.grants(&Scope::<SmolStr>::parse("blob:image/png").unwrap()));
        assert!(scope.grants(&Scope::<SmolStr>::parse("blob:text/plain").unwrap()));
        assert!(!scope.grants(&Scope::<SmolStr>::parse("blob:application/json").unwrap()));
    }

    #[test]
    fn test_rpc_default_wildcards() {
        let scope = Scope::<SmolStr>::parse("rpc").unwrap();
        let mut lxm = BTreeSet::new();
        let mut aud = BTreeSet::new();
        lxm.insert(RpcLexicon::All);
        aud.insert(RpcAudience::All);
        assert_eq!(scope, Scope::Rpc(RpcScope { lxm, aud }));
    }

    #[test]
    fn test_atproto_scope_parsing() {
        let scope = Scope::<SmolStr>::parse("atproto").unwrap();
        assert_eq!(scope, Scope::Atproto);

        // Atproto should not accept suffixes
        assert!(Scope::<SmolStr>::parse("atproto:something").is_err());
        assert!(Scope::<SmolStr>::parse("atproto?param=value").is_err());
    }

    #[test]
    fn test_transition_scope_parsing() {
        let scope = Scope::<SmolStr>::parse("transition:generic").unwrap();
        assert_eq!(scope, Scope::Transition(TransitionScope::Generic));

        let scope = Scope::<SmolStr>::parse("transition:email").unwrap();
        assert_eq!(scope, Scope::Transition(TransitionScope::Email));

        // Test invalid transition types
        assert!(matches!(
            Scope::<SmolStr>::parse("transition:invalid"),
            Err(ParseError::InvalidResource(_))
        ));

        // Test missing suffix
        assert!(matches!(
            Scope::<SmolStr>::parse("transition"),
            Err(ParseError::MissingResource)
        ));

        // Test transition doesn't accept query parameters
        assert!(matches!(
            Scope::<SmolStr>::parse("transition:generic?param=value"),
            Err(ParseError::InvalidResource(_))
        ));
    }

    #[test]
    fn test_atproto_scope_normalization() {
        let scope = Scope::<SmolStr>::parse("atproto").unwrap();
        assert_eq!(scope.to_string_normalized(), "atproto");
    }

    #[test]
    fn test_transition_scope_normalization() {
        let tests = vec![
            ("transition:generic", "transition:generic"),
            ("transition:email", "transition:email"),
        ];

        for (input, expected) in tests {
            let scope = Scope::<SmolStr>::parse(input).unwrap();
            assert_eq!(scope.to_string_normalized(), expected);
        }
    }

    #[test]
    fn test_transition_chat_bsky() {
        // Test parsing.
        let scope = Scope::<SmolStr>::parse("transition:chat.bsky").unwrap();
        assert_eq!(scope, Scope::Transition(TransitionScope::ChatBsky));

        // Test serialization.
        assert_eq!(scope.to_string_normalized(), "transition:chat.bsky");

        // Test grants itself.
        let other: Scope<SmolStr> = Scope::Transition(TransitionScope::ChatBsky);
        assert!(scope.grants(&other));

        // Test doesn't grant other transition scopes.
        let generic: Scope<SmolStr> = Scope::Transition(TransitionScope::Generic);
        let email: Scope<SmolStr> = Scope::Transition(TransitionScope::Email);
        assert!(!scope.grants(&generic));
        assert!(!scope.grants(&email));

        // Test other scopes don't grant ChatBsky.
        assert!(!generic.grants(&scope));
        assert!(!email.grants(&scope));

        // Test typo is rejected.
        assert!(matches!(
            Scope::<SmolStr>::parse("transition:chat.bsk"),
            Err(ParseError::InvalidResource(_))
        ));
    }

    #[test]
    fn test_include_scope_serialisation() {
        // Test with audience containing '#' — should encode as %23.
        let scope: Scope<SmolStr> = Scope::Include(IncludeScope {
            nsid: Nsid::new_static("app.bsky.full").unwrap(),
            audience: Some(SmolStr::new_static("did:web:api.example.com#svc_appview")),
        });
        assert_eq!(
            scope.to_string_normalized(),
            "include:app.bsky.full?aud=did:web:api.example.com%23svc_appview"
        );

        // Test without audience.
        let scope: Scope<SmolStr> = Scope::Include(IncludeScope {
            nsid: Nsid::new_static("app.bsky.authFull").unwrap(),
            audience: None,
        });
        assert_eq!(scope.to_string_normalized(), "include:app.bsky.authFull");

        // Test with simple audience (no '#').
        let scope: Scope<SmolStr> = Scope::Include(IncludeScope {
            nsid: Nsid::new_static("com.example.perm").unwrap(),
            audience: Some(SmolStr::new_static("did:plc:yfvwmnlztr4dwkb7hwz55r2g")),
        });
        assert_eq!(
            scope.to_string_normalized(),
            "include:com.example.perm?aud=did:plc:yfvwmnlztr4dwkb7hwz55r2g"
        );
    }

    #[test]
    fn test_include_scope_grants() {
        // Test identical include scopes grant each other.
        let scope1: Scope<SmolStr> = Scope::Include(IncludeScope {
            nsid: Nsid::new_static("app.bsky.full").unwrap(),
            audience: Some(SmolStr::new_static("did:web:api.example.com")),
        });
        let scope2: Scope<SmolStr> = Scope::Include(IncludeScope {
            nsid: Nsid::new_static("app.bsky.full").unwrap(),
            audience: Some(SmolStr::new_static("did:web:api.example.com")),
        });
        assert!(scope1.grants(&scope2));

        // Test different NSIDs don't grant.
        let scope3: Scope<SmolStr> = Scope::Include(IncludeScope {
            nsid: Nsid::new_static("app.bsky.authFull").unwrap(),
            audience: Some(SmolStr::new_static("did:web:api.example.com")),
        });
        assert!(!scope1.grants(&scope3));

        // Test same NSID but different audiences don't grant.
        let scope4: Scope<SmolStr> = Scope::Include(IncludeScope {
            nsid: Nsid::new_static("app.bsky.full").unwrap(),
            audience: Some(SmolStr::new_static("did:plc:yfvwmnlztr4dwkb7hwz55r2g")),
        });
        assert!(!scope1.grants(&scope4));

        // Test audience vs no audience don't grant.
        let scope5: Scope<SmolStr> = Scope::Include(IncludeScope {
            nsid: Nsid::new_static("app.bsky.full").unwrap(),
            audience: None,
        });
        assert!(!scope1.grants(&scope5));

        // Test no-audience scopes grant each other only if NSID matches.
        let scope6: Scope<SmolStr> = Scope::Include(IncludeScope {
            nsid: Nsid::new_static("app.bsky.full").unwrap(),
            audience: None,
        });
        assert!(scope5.grants(&scope6));

        // Test non-include scopes don't grant include scopes and vice versa.
        let account = Scope::<SmolStr>::parse("account:email").unwrap();
        assert!(!account.grants(&scope1));
        assert!(!scope1.grants(&account));
    }

    #[test]
    fn test_atproto_scope_grants() {
        let atproto = Scope::<SmolStr>::parse("atproto").unwrap();
        let account = Scope::<SmolStr>::parse("account:email").unwrap();
        let identity = Scope::<SmolStr>::parse("identity:handle").unwrap();
        let blob = Scope::<SmolStr>::parse("blob:image/png").unwrap();
        let repo = Scope::<SmolStr>::parse("repo:app.bsky.feed.post").unwrap();
        let rpc = Scope::<SmolStr>::parse("rpc:com.example.service").unwrap();
        let transition_generic = Scope::<SmolStr>::parse("transition:generic").unwrap();
        let transition_email = Scope::<SmolStr>::parse("transition:email").unwrap();

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
        let transition_generic = Scope::<SmolStr>::parse("transition:generic").unwrap();
        let transition_email = Scope::<SmolStr>::parse("transition:email").unwrap();
        let account = Scope::<SmolStr>::parse("account:email").unwrap();

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
    fn test_openid_connect_scope_parsing() {
        // Test OpenID scope
        let scope = Scope::<SmolStr>::parse("openid").unwrap();
        assert_eq!(scope, Scope::OpenId);

        // Test Profile scope
        let scope = Scope::<SmolStr>::parse("profile").unwrap();
        assert_eq!(scope, Scope::Profile);

        // Test Email scope
        let scope = Scope::<SmolStr>::parse("email").unwrap();
        assert_eq!(scope, Scope::Email);

        // Test that they don't accept suffixes
        assert!(Scope::<SmolStr>::parse("openid:something").is_err());
        assert!(Scope::<SmolStr>::parse("profile:something").is_err());
        assert!(Scope::<SmolStr>::parse("email:something").is_err());

        // Test that they don't accept query parameters
        assert!(Scope::<SmolStr>::parse("openid?param=value").is_err());
        assert!(Scope::<SmolStr>::parse("profile?param=value").is_err());
        assert!(Scope::<SmolStr>::parse("email?param=value").is_err());
    }

    #[test]
    fn test_openid_connect_scope_normalization() {
        let scope = Scope::<SmolStr>::parse("openid").unwrap();
        assert_eq!(scope.to_string_normalized(), "openid");

        let scope = Scope::<SmolStr>::parse("profile").unwrap();
        assert_eq!(scope.to_string_normalized(), "profile");

        let scope = Scope::<SmolStr>::parse("email").unwrap();
        assert_eq!(scope.to_string_normalized(), "email");
    }

    #[test]
    fn test_openid_connect_scope_grants() {
        let openid = Scope::<SmolStr>::parse("openid").unwrap();
        let profile = Scope::<SmolStr>::parse("profile").unwrap();
        let email = Scope::<SmolStr>::parse("email").unwrap();
        let account = Scope::<SmolStr>::parse("account:email").unwrap();

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

    // ========================================================================
    // Tests for Task 1: Scopes<S> container and constructor
    // ========================================================================

    #[test]
    fn test_scopes_new_multiple() {
        // Test AC3.1: Parse multiple scopes and create indices.
        let scopes =
            Scopes::new(SmolStr::new_static("atproto rpc:* repo:app.bsky.feed.post")).unwrap();
        assert_eq!(scopes.len(), 3);
    }

    #[test]
    fn test_scopes_new_empty() {
        // Test AC3.7: Empty string produces empty Scopes.
        let scopes = Scopes::new(SmolStr::new_static("")).unwrap();
        assert!(scopes.is_empty());
    }

    #[test]
    fn test_scopes_new_with_spaces() {
        // Test consecutive spaces are handled.
        let scopes = Scopes::new(SmolStr::new_static("atproto  rpc:*")).unwrap();
        assert_eq!(scopes.len(), 2);
    }

    #[test]
    fn test_scopes_new_invalid_scope() {
        // Test AC3.8: Invalid scope is rejected.
        let result = Scopes::new(SmolStr::new_static("atproto badscope"));
        assert!(result.is_err());
        match result.unwrap_err() {
            ParseError::UnknownPrefix(_) => {}
            e => panic!("expected UnknownPrefix error, got {:?}", e),
        }
    }

    #[test]
    fn test_scopes_buffer_size_limit() {
        // Test buffer exceeding u16 limit is rejected.
        let too_long = "a".repeat(u16::MAX as usize + 1);
        let smol = too_long.as_str().to_smolstr();
        let result = Scopes::new(smol);
        assert!(result.is_err());
    }

    #[test]
    fn test_scopes_unit_scope_parsing() {
        // Test each unit scope parses correctly.
        let test_cases = vec![
            ("atproto", 1),
            ("openid", 1),
            ("profile", 1),
            ("email", 1),
            ("atproto openid profile", 3),
        ];

        for (input, expected_count) in test_cases {
            let scopes = Scopes::new(SmolStr::new_static(input)).unwrap();
            assert_eq!(scopes.len(), expected_count, "failed for: {}", input);
        }
    }

    #[test]
    fn test_scopes_account_scope_parsing() {
        // Test account scopes parse correctly.
        let test_cases = vec![
            ("account:email", 1),
            ("account:repo", 1),
            ("account:status", 1),
            ("account:email?action=manage", 1),
            ("account:email?action=read", 1),
        ];

        for (input, expected_count) in test_cases {
            let scopes = Scopes::new(SmolStr::new_static(input)).unwrap();
            assert_eq!(scopes.len(), expected_count, "failed for: {}", input);
        }
    }

    #[test]
    fn test_scopes_identity_scope_parsing() {
        // Test identity scopes parse correctly.
        let test_cases = vec![
            ("identity:handle", 1),
            ("identity:*", 1),
            ("identity:handle identity:*", 2),
        ];

        for (input, expected_count) in test_cases {
            let scopes = Scopes::new(SmolStr::new_static(input)).unwrap();
            assert_eq!(scopes.len(), expected_count, "failed for: {}", input);
        }
    }

    #[test]
    fn test_scopes_blob_scope_parsing() {
        // Test blob scopes parse correctly.
        let test_cases = vec![
            ("blob:*/*", 1),
            ("blob:image/png", 1),
            ("blob:image/*", 1),
            ("blob?accept=image/png&accept=image/jpeg", 1),
        ];

        for (input, expected_count) in test_cases {
            let scopes = Scopes::new(SmolStr::new_static(input)).unwrap();
            assert_eq!(scopes.len(), expected_count, "failed for: {}", input);
        }
    }

    #[test]
    fn test_scopes_repo_scope_parsing() {
        // Test repo scopes parse correctly.
        let test_cases = vec![
            ("repo:*", 1),
            ("repo:app.bsky.feed.post", 1),
            ("repo:app.bsky.feed.post?action=create", 1),
            ("repo:app.bsky.feed.post?action=create&action=update", 1),
        ];

        for (input, expected_count) in test_cases {
            let scopes = Scopes::new(SmolStr::new_static(input)).unwrap();
            assert_eq!(scopes.len(), expected_count, "failed for: {}", input);
        }
    }

    #[test]
    fn test_scopes_rpc_scope_parsing() {
        // Test rpc scopes parse correctly.
        let test_cases = vec![
            ("rpc:*", 1),
            ("rpc:com.example.service", 1),
            ("rpc:com.example.service?aud=did:web:example.com", 1),
            ("rpc?lxm=com.example.service&aud=did:web:example.com", 1),
        ];

        for (input, expected_count) in test_cases {
            let scopes = Scopes::new(SmolStr::new_static(input)).unwrap();
            assert_eq!(scopes.len(), expected_count, "failed for: {}", input);
        }
    }

    #[test]
    fn test_scopes_include_scope_parsing() {
        // Test include scopes parse correctly.
        let test_cases = vec![
            ("include:app.bsky.authFull", 1),
            ("include:app.bsky.full?aud=did:web:api.example.com", 1),
            (
                "include:app.bsky.full?aud=did:web:api.example.com%23svc_appview",
                1,
            ),
        ];

        for (input, expected_count) in test_cases {
            let scopes = Scopes::new(SmolStr::new_static(input)).unwrap();
            assert_eq!(scopes.len(), expected_count, "failed for: {}", input);
        }
    }

    #[test]
    fn test_scopes_include_missing_nsid() {
        // Test AC2.6: include: with no NSID is rejected.
        let result = Scopes::new(SmolStr::new_static("include:"));
        assert!(result.is_err());
    }

    #[test]
    fn test_scopes_include_invalid_audience_did() {
        // Test AC2.7: include scope with invalid DID audience is rejected.
        let result = Scopes::new(SmolStr::new_static("include:app.bsky.authFull?aud=notadid"));
        assert!(result.is_err());
    }

    #[test]
    fn test_scopes_transition_scope_parsing() {
        // Test transition scopes parse correctly.
        let test_cases = vec![
            ("transition:generic", 1),
            ("transition:email", 1),
            ("transition:chat.bsky", 1),
            ("transition:generic transition:email", 2),
        ];

        for (input, expected_count) in test_cases {
            let scopes = Scopes::new(SmolStr::new_static(input)).unwrap();
            assert_eq!(scopes.len(), expected_count, "failed for: {}", input);
        }
    }

    #[test]
    fn test_scopes_reduction_removes_broader_scope() {
        // Test that broader scopes subsume narrower ones.
        // repo:* grants repo:app.bsky.feed.post, so only repo:* should remain.
        let scopes = Scopes::new(SmolStr::new_static("repo:app.bsky.feed.post repo:*")).unwrap();
        assert_eq!(scopes.len(), 1);
    }

    // ========================================================================
    // Tests for Task 2: Scope reconstruction from indices (via Scopes container)
    // ========================================================================

    #[test]
    fn test_scopes_reconstruction_unit() {
        // Reconstruct unit scopes from indices.
        let scopes = Scopes::new(SmolStr::new_static("atproto openid")).unwrap();
        assert_eq!(scopes.len(), 2);
    }

    #[test]
    fn test_scopes_reconstruction_account() {
        // Reconstruct account scopes from indices.
        let scopes = Scopes::new(SmolStr::new_static(
            "account:email account:repo?action=manage",
        ))
        .unwrap();
        assert_eq!(scopes.len(), 2);
    }

    #[test]
    fn test_scopes_reconstruction_identity() {
        // Reconstruct identity scopes from indices.
        let scopes = Scopes::new(SmolStr::new_static("identity:handle identity:*")).unwrap();
        assert_eq!(scopes.len(), 2);
    }

    #[test]
    fn test_scopes_reconstruction_blob() {
        // Reconstruct blob scopes from indices.
        let scopes = Scopes::new(SmolStr::new_static("blob:image/png blob:*/*")).unwrap();
        assert_eq!(scopes.len(), 2);
    }

    #[test]
    fn test_scopes_reconstruction_repo() {
        // Reconstruct repo scopes from indices.
        let scopes = Scopes::new(SmolStr::new_static("repo:app.bsky.feed.post repo:*")).unwrap();
        // repo:* grants repo:app.bsky.feed.post, so only repo:* should remain.
        assert_eq!(scopes.len(), 1);
    }

    #[test]
    fn test_scopes_reconstruction_rpc() {
        // Reconstruct rpc scopes from indices.
        let scopes = Scopes::new(SmolStr::new_static("rpc:com.example.service rpc:*")).unwrap();
        // rpc:* grants rpc:com.example.service, so only rpc:* should remain.
        assert_eq!(scopes.len(), 1);
    }

    #[test]
    fn test_scopes_reconstruction_include() {
        // Reconstruct include scopes from indices.
        let scopes = Scopes::new(SmolStr::new_static(
            "include:app.bsky.authFull include:app.bsky.full?aud=did:web:api.example.com",
        ))
        .unwrap();
        assert_eq!(scopes.len(), 2);
    }

    // ========================================================================
    // Task 3: Accessor Tests
    // ========================================================================

    #[test]
    fn test_scopes_iter() {
        // oauth-scopes-rework.AC3.2: `iter()` yields correctly typed `Scope<&str>`
        // views borrowing from the buffer.
        let scopes =
            Scopes::new(SmolStr::new_static("atproto rpc:* repo:app.bsky.feed.post")).unwrap();

        let collected: Vec<_> = scopes.iter().collect();

        // Verify we got scopes back
        assert!(!collected.is_empty());

        // Verify we can iterate and get expected scope types
        let has_atproto = collected.iter().any(|s| matches!(s, Scope::Atproto));
        let has_rpc = collected.iter().any(|s| matches!(s, Scope::Rpc(_)));
        let has_repo = collected.iter().any(|s| matches!(s, Scope::Repo(_)));

        assert!(has_atproto, "Should contain Atproto scope");
        assert!(has_rpc, "Should contain Rpc scope");
        assert!(has_repo, "Should contain Repo scope");
    }

    #[test]
    fn test_scopes_get() {
        // Test `get()` accessor for positional index access.
        let scopes = Scopes::new(SmolStr::new_static("atproto rpc:*")).unwrap();
        assert_eq!(scopes.len(), 2);

        let first = scopes.get(0).expect("First scope should exist");
        match first {
            Scope::Atproto => (),
            _ => panic!("Expected Atproto scope"),
        }

        let second = scopes.get(1).expect("Second scope should exist");
        match second {
            Scope::Rpc(_) => (),
            _ => panic!("Expected Rpc scope"),
        }

        let third = scopes.get(2);
        assert!(third.is_none(), "Third scope should not exist");
    }

    #[test]
    fn test_scopes_get_owned() {
        // oauth-scopes-rework.AC3.3: `get_owned()` returns `Scope<SmolStr>`
        // independent of the buffer's lifetime.
        let scopes = Scopes::new(SmolStr::new_static("atproto repo:app.bsky.feed.post")).unwrap();
        assert_eq!(scopes.len(), 2);

        let owned = scopes.get_owned(0).expect("First scope should exist");
        match owned {
            Scope::Atproto => (),
            _ => panic!("Expected Atproto scope"),
        }

        let repo_owned = scopes.get_owned(1).expect("Second scope should exist");
        match repo_owned {
            Scope::Repo(_) => (),
            _ => panic!("Expected Repo scope"),
        }

        let none = scopes.get_owned(99);
        assert!(none.is_none(), "Out-of-bounds access should return None");
    }

    #[test]
    fn test_scopes_get_as() {
        // Test `get_as()` with caller-chosen backing type.
        let scopes = Scopes::new(SmolStr::new_static("atproto")).unwrap();

        // Convert to String backing
        let as_string: Option<Scope<String>> = scopes.get_as(0);
        assert!(as_string.is_some());
        match as_string {
            Some(Scope::Atproto) => (),
            _ => panic!("Expected Atproto scope as String"),
        }

        // Verify get_as handles out of bounds
        let out_of_bounds: Option<Scope<String>> = scopes.get_as(10);
        assert!(out_of_bounds.is_none());
    }

    #[test]
    fn test_scopes_iter_multiple() {
        // Verify iterator works with multiple scope types.
        let scopes = Scopes::new(SmolStr::new_static(
            "atproto rpc:* repo:app.bsky.feed.post account:email identity:handle",
        ))
        .unwrap();

        let mut count = 0;
        for scope in scopes.iter() {
            count += 1;
            // Just verify we can iterate and get back a Scope
            let _ = scope;
        }

        assert_eq!(count, scopes.len());
    }

    #[test]
    fn test_scopes_iter_empty() {
        // Verify iterator works on empty Scopes.
        let scopes = Scopes::new(SmolStr::new_static("")).unwrap();
        assert!(scopes.is_empty());

        let collected: Vec<_> = scopes.iter().collect();
        assert_eq!(collected.len(), 0);
    }

    // ========================================================================
    // Task 4: Conversion Tests
    // ========================================================================

    #[test]
    fn test_scopes_borrow() {
        // oauth-scopes-rework.AC3.4: `borrow()` produces `Scopes<&str>` cheaply.
        let original: Scopes<SmolStr> = Scopes::new(SmolStr::new_static("atproto rpc:*")).unwrap();
        assert_eq!(original.len(), 2);

        let borrowed: Scopes<&str> = original.borrow();
        assert_eq!(borrowed.len(), 2);

        // Verify the borrowed version has the same scopes
        let iter_count: usize = borrowed.iter().count();
        assert_eq!(iter_count, 2);

        // Verify content matches
        let orig_iter = original.iter().collect::<Vec<_>>();
        let borrow_iter = borrowed.iter().collect::<Vec<_>>();
        assert_eq!(orig_iter.len(), borrow_iter.len());
    }

    #[test]
    fn test_scopes_convert() {
        // oauth-scopes-rework.AC3.5: `convert()` produces correct backing type conversions.
        let original: Scopes<SmolStr> = Scopes::new(SmolStr::new_static("atproto repo:*")).unwrap();
        assert_eq!(original.len(), 2);

        // Convert to String
        let converted: Scopes<String> = original.convert();
        assert_eq!(converted.len(), 2);

        // Verify content is preserved
        let converted_iter = converted.iter().collect::<Vec<_>>();
        assert_eq!(converted_iter.len(), 2);

        match &converted_iter[0] {
            Scope::Atproto => (),
            _ => panic!("Expected Atproto scope"),
        }
    }

    #[test]
    fn test_scopes_into_static() {
        // Test IntoStatic trait implementation.
        use jacquard_common::CowStr;

        let original = Scopes::new(CowStr::copy_from_str("atproto rpc:*")).unwrap();
        assert_eq!(original.len(), 2);

        let static_scopes = original.into_static();
        assert_eq!(static_scopes.len(), 2);

        // Verify content is preserved
        let iter_count: usize = static_scopes.iter().count();
        assert_eq!(iter_count, 2);
    }

    #[test]
    fn test_scopes_conversions_preserve_content() {
        // Verify that all conversion methods preserve scope content.
        let input = "atproto repo:app.bsky.feed.post?action=create account:repo";
        let original: Scopes<SmolStr> = Scopes::new(SmolStr::new(input)).unwrap();
        let original_count = original.len();

        // Test borrow
        let borrowed = original.borrow();
        assert_eq!(borrowed.len(), original_count);

        // Verify both have the same normalized output before converting
        let orig_normalized = original.to_normalized_string();
        let borrow_normalized = borrowed.to_normalized_string();
        assert_eq!(orig_normalized, borrow_normalized);

        // Test convert (this moves original)
        let converted: Scopes<String> = original.convert();
        assert_eq!(converted.len(), original_count);

        let conv_normalized = converted.to_normalized_string();
        assert_eq!(orig_normalized, conv_normalized);
    }

    // ========================================================================
    // Task 5: Serialize Tests
    // ========================================================================

    #[test]
    fn test_scopes_serialize_single() {
        // Test serialization of a single scope.
        let scopes = Scopes::new(SmolStr::new_static("atproto")).unwrap();
        let json = serde_json::to_string(&scopes).unwrap();
        assert_eq!(json, "\"atproto\"");
    }

    #[test]
    fn test_scopes_serialize_multiple_sorted() {
        // oauth-scopes-rework.AC3.6: Serialize produces sorted output
        // regardless of input order.
        let scopes =
            Scopes::new(SmolStr::new_static("rpc:* atproto repo:app.bsky.feed.post")).unwrap();
        let json = serde_json::to_string(&scopes).unwrap();
        // Should be sorted: atproto, repo:app.bsky.feed.post, rpc:*
        assert_eq!(json, "\"atproto repo:app.bsky.feed.post rpc:*\"");
    }

    #[test]
    fn test_scopes_serialize_empty() {
        // Test serialization of empty Scopes.
        let scopes: Scopes<SmolStr> = Scopes::empty();
        let json = serde_json::to_string(&scopes).unwrap();
        assert_eq!(json, "\"\"");
    }

    #[test]
    fn test_scopes_serialize_with_reduction() {
        // Test serialization when scopes reduce (e.g., repo:* includes repo:app.bsky.feed.post).
        let scopes = Scopes::new(SmolStr::new_static("repo:* repo:app.bsky.feed.post")).unwrap();
        // Should reduce to just repo:*
        assert_eq!(scopes.len(), 1);
        let json = serde_json::to_string(&scopes).unwrap();
        assert_eq!(json, "\"repo:*\"");
    }

    #[test]
    fn test_scopes_serialize_includes_include_scope() {
        // Test serialization with include scope.
        let scopes = Scopes::new(SmolStr::new_static("atproto include:app.bsky.authFull")).unwrap();
        let json = serde_json::to_string(&scopes).unwrap();
        // Should be sorted and include normalized form
        assert_eq!(json, "\"atproto include:app.bsky.authFull\"");
    }

    // ========================================================================
    // Task 6: Deserialize Tests
    // ========================================================================

    #[test]
    fn test_scopes_deserialize_single() {
        // Test deserialization of a single scope.
        let json = "\"atproto\"";
        let scopes: Scopes<SmolStr> = serde_json::from_str(json).unwrap();
        assert_eq!(scopes.len(), 1);
        match scopes.get(0) {
            Some(Scope::Atproto) => (),
            _ => panic!("Expected Atproto scope"),
        }
    }

    #[test]
    fn test_scopes_deserialize_multiple() {
        // Test deserialization of multiple scopes.
        let json = "\"atproto rpc:* repo:app.bsky.feed.post\"";
        let scopes: Scopes<SmolStr> = serde_json::from_str(json).unwrap();
        assert_eq!(scopes.len(), 3);
    }

    #[test]
    fn test_scopes_deserialize_empty() {
        // Test deserialization of empty string.
        let json = "\"\"";
        let scopes: Scopes<SmolStr> = serde_json::from_str(json).unwrap();
        assert_eq!(scopes.len(), 0);
        assert!(scopes.is_empty());
    }

    #[test]
    fn test_scopes_serde_roundtrip() {
        // oauth-scopes-rework.AC3.6: Round-trip test with sorting verification.
        let input = "rpc:* atproto repo:app.bsky.feed.post account:email";
        let scopes: Scopes<SmolStr> = Scopes::new(SmolStr::new(input)).unwrap();

        // Serialize
        let json = serde_json::to_string(&scopes).unwrap();

        // Should be sorted
        assert_eq!(
            json,
            "\"account:email atproto repo:app.bsky.feed.post rpc:*\""
        );

        // Deserialize
        let deserialized: Scopes<SmolStr> = serde_json::from_str(&json).unwrap();

        // Should have same len (reduction applied)
        assert_eq!(deserialized.len(), scopes.len());

        // Verify content matches by collecting scopes
        let orig_normalized = scopes.to_normalized_string();
        let deser_normalized = deserialized.to_normalized_string();
        assert_eq!(orig_normalized, deser_normalized);
    }

    #[test]
    fn test_scopes_deserialize_invalid() {
        // Test deserialization of invalid scope.
        let json = "\"invalid:notagoodscope\"";
        let result: Result<Scopes<SmolStr>, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_scopes_roundtrip_with_encoded_audience() {
        // AC2.3: include scope with audience (including special chars) can be serialized and deserialized.
        // Create an include scope with audience containing special character
        let input = "include:app.bsky.authFull?aud=did:web:example.com%23svc";
        let scopes: Scopes<SmolStr> = Scopes::new(SmolStr::new(input)).unwrap();
        assert_eq!(scopes.len(), 1);

        // Serialize to JSON - should not panic
        let json = serde_json::to_string(&scopes).unwrap();
        assert!(json.contains("include:app.bsky.authFull"));

        // Deserialize back - should not panic
        let deserialized: Scopes<SmolStr> = serde_json::from_str(&json).unwrap();

        // Scopes should have the same length
        assert_eq!(scopes.len(), deserialized.len());
        assert_eq!(deserialized.len(), 1);
    }

    // ========================================================================
    // Task 7: Convenience Methods Tests
    // ========================================================================

    #[test]
    fn test_scopes_len() {
        // AC3.1: len() returns correct count after reduction.
        let scopes = Scopes::new(SmolStr::new_static(
            "atproto repo:* repo:app.bsky.feed.post",
        ))
        .unwrap();
        // repo:* should reduce the more specific one
        assert_eq!(scopes.len(), 2); // atproto + repo:*
    }

    #[test]
    fn test_scopes_is_empty() {
        // AC3.7: is_empty() returns true for empty Scopes.
        let empty: Scopes<SmolStr> = Scopes::empty();
        assert!(empty.is_empty());

        let nonempty = Scopes::new(SmolStr::new_static("atproto")).unwrap();
        assert!(!nonempty.is_empty());
    }

    #[test]
    fn test_scopes_as_str() {
        // Test as_str() returns the raw buffer.
        let scopes = Scopes::new(SmolStr::new_static("atproto rpc:*")).unwrap();
        let s = scopes.as_str();
        assert_eq!(s, "atproto rpc:*");
    }

    #[test]
    fn test_scopes_to_normalized_string() {
        // Test to_normalized_string() produces same output as serialize.
        let scopes = Scopes::new(SmolStr::new_static("rpc:* atproto")).unwrap();
        let normalized = scopes.to_normalized_string();
        assert_eq!(normalized, "atproto rpc:*");

        // Serialize should match
        let json = serde_json::to_string(&scopes).unwrap();
        assert_eq!(json, "\"atproto rpc:*\"");
    }

    #[test]
    fn test_scopes_empty_constructor() {
        // Test Scopes::<SmolStr>::empty() creates empty container.
        let empty = Scopes::empty();
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
        assert_eq!(empty.to_normalized_string(), SmolStr::default());
    }

    #[test]
    fn test_scopes_default() {
        // Test Default trait for Scopes.
        // should return atproto scope
        let default: Scopes<SmolStr> = Default::default();
        assert_eq!(default.buffer.as_str(), "atproto");
        assert!(matches!(default.get(0), Some(Scope::Atproto)));
    }

    #[test]
    fn test_scopes_grants_single() {
        // Test grants() method with single scope.
        let scopes = Scopes::new(SmolStr::new_static("repo:*")).unwrap();
        let queried: Scope<SmolStr> = Scope::parse("repo:app.bsky.feed.post").unwrap();
        assert!(scopes.grants(&queried));

        let queried2: Scope<SmolStr> = Scope::parse("atproto").unwrap();
        assert!(!scopes.grants(&queried2));
    }

    #[test]
    fn test_scopes_grants_multiple() {
        // Test grants() with multiple scopes.
        let scopes = Scopes::new(SmolStr::new_static("atproto rpc:*")).unwrap();
        let queried: Scope<SmolStr> = Scope::parse("rpc:com.atproto.server.createSession").unwrap();
        assert!(scopes.grants(&queried));
    }

    #[test]
    fn test_scopes_construction() {
        // AC3.1: Construct multi-scope string, verify len and individual scopes.
        let scopes =
            Scopes::new(SmolStr::new_static("atproto rpc:* repo:app.bsky.feed.post")).unwrap();
        assert_eq!(scopes.len(), 3);

        // Verify individual scopes
        match scopes.get(0) {
            Some(Scope::Atproto) => (),
            _ => panic!("Expected Atproto at index 0"),
        }
        assert!(scopes.get(1).is_some());
        assert!(scopes.get(2).is_some());
        assert!(scopes.get(3).is_none());
    }

    #[test]
    fn test_scopes_empty_string() {
        // AC3.7: Empty string produces empty Scopes.
        let scopes = Scopes::new(SmolStr::new_static("")).unwrap();
        assert_eq!(scopes.len(), 0);
        assert!(scopes.is_empty());
    }

    #[test]
    fn test_scopes_invalid_scope() {
        // AC3.8: Invalid scope in string causes construction failure.
        let result = Scopes::new(SmolStr::new("invalid:nosuchprefix"));
        assert!(result.is_err());
    }

    #[test]
    fn test_scopes_iter_collection() {
        // AC3.2: Iterate, collect, verify typed views.
        let scopes = Scopes::new(SmolStr::new_static("atproto rpc:*")).unwrap();
        let collected: Vec<_> = scopes.iter().collect();
        assert_eq!(collected.len(), 2);
        assert!(matches!(collected[0], Scope::Atproto));
    }

    #[test]
    fn test_scopes_consecutive_spaces() {
        // Test handling of multiple spaces between scopes.
        let scopes = Scopes::new(SmolStr::new("atproto  rpc:*")).unwrap();
        assert_eq!(scopes.len(), 2);
    }

    #[test]
    fn test_scopes_reduction() {
        // Test scope reduction (broader scope removes more specific ones).
        let scopes = Scopes::new(SmolStr::new_static("repo:* repo:app.bsky.feed.post")).unwrap();
        assert_eq!(scopes.len(), 1); // Should reduce to just repo:*
    }

    #[test]
    fn test_scopes_include_no_audience() {
        // AC2.1: include scope with no audience parses correctly.
        let scopes = Scopes::new(SmolStr::new_static("include:app.bsky.authFull")).unwrap();
        assert_eq!(scopes.len(), 1);
        match scopes.get(0) {
            Some(Scope::Include(inc)) => {
                assert_eq!(inc.nsid.as_ref(), "app.bsky.authFull");
                assert_eq!(inc.audience, None);
            }
            _ => panic!("Expected Include scope"),
        }
    }

    #[test]
    fn test_scopes_include_plain_audience() {
        // AC2.2: include scope with plain unencoded audience.
        let scopes = Scopes::new(SmolStr::new_static(
            "include:app.bsky.authFull?aud=did:web:api.example.com",
        ))
        .unwrap();
        assert_eq!(scopes.len(), 1);
        match scopes.get(0) {
            Some(Scope::Include(inc)) => {
                assert_eq!(inc.nsid.as_ref(), "app.bsky.authFull");
                assert!(inc.audience.is_some());
            }
            _ => panic!("Expected Include scope"),
        }
    }

    #[test]
    fn test_scopes_include_empty_nsid() {
        // AC2.6: include with no NSID is rejected.
        let result = Scopes::new(SmolStr::new("include:"));
        assert!(result.is_err());
    }

    #[test]
    fn test_scopes_include_invalid_did_audience() {
        // AC2.7: include with invalid DID audience is rejected.
        let result = Scopes::new(SmolStr::new("include:app.bsky.authFull?aud=notadid"));
        assert!(result.is_err());
    }

    #[test]
    fn test_scopes_all_prefixes() {
        // Test every scope prefix parses correctly in a Scopes container.
        let prefixes = vec![
            "account:email",
            "identity:handle",
            "blob:*/*",
            "repo:*",
            "rpc:*",
            "atproto",
            "transition:generic",
            "openid",
            "profile",
            "email",
        ];

        for prefix in prefixes {
            let scopes = Scopes::new(SmolStr::new(prefix)).unwrap();
            assert_eq!(scopes.len(), 1, "Failed to parse: {}", prefix);
        }
    }

    #[test]
    fn test_scopes_borrow_borrowshare() {
        // AC3.4: borrow() produces Scopes<&str> with BorrowOrShare semantics.
        let original: Scopes<SmolStr> = Scopes::new(SmolStr::new_static("atproto rpc:*")).unwrap();
        let borrowed: Scopes<&str> = original.borrow();
        assert_eq!(borrowed.len(), original.len());

        // Both should iterate the same
        let orig_iter = original.iter().collect::<Vec<_>>();
        let borrow_iter = borrowed.iter().collect::<Vec<_>>();
        assert_eq!(orig_iter.len(), borrow_iter.len());
    }

    #[test]
    fn test_scopes_convert_type() {
        // AC3.5: convert() produces correct backing type.
        let original: Scopes<SmolStr> = Scopes::new(SmolStr::new_static("atproto")).unwrap();
        let converted: Scopes<String> = original.convert();
        assert_eq!(converted.len(), 1);
        assert!(matches!(converted.get(0), Some(Scope::Atproto)));
    }

    #[test]
    fn test_scopes_bare_blob_defaults_to_all() {
        // Critical fix: bare `blob` token (without suffix) should default to MimePattern::All.
        // This tests that we don't store unsound byte ranges past the token.
        let scopes = Scopes::new(SmolStr::new("blob")).unwrap();
        assert_eq!(scopes.len(), 1);

        let scope = scopes.get(0).unwrap();
        if let Scope::Blob(blob_scope) = scope {
            // Should accept all mime types.
            assert_eq!(blob_scope.accept.len(), 1);
            assert!(blob_scope.accept.contains(&MimePattern::All));
        } else {
            panic!("Expected Scope::Blob, got {:?}", scope);
        }

        // Verify reconstruction and normalization work.
        // Normalized form expands bare `blob` to explicit `blob:*/*`.
        let normalized = scopes.to_normalized_string();
        assert_eq!(normalized, "blob:*/*");
    }

    #[test]
    fn test_scopes_bare_rpc_defaults_to_all() {
        // Critical fix: bare `rpc` token (without suffix) should default to all lexicons and audiences.
        // This tests that we don't store unsound byte ranges past the token.
        let scopes = Scopes::new(SmolStr::new("rpc")).unwrap();
        assert_eq!(scopes.len(), 1);

        let scope = scopes.get(0).unwrap();
        if let Scope::Rpc(rpc_scope) = scope {
            // Should accept all lexicons and audiences.
            assert_eq!(rpc_scope.lxm.len(), 1);
            assert!(rpc_scope.lxm.contains(&RpcLexicon::All));
            assert_eq!(rpc_scope.aud.len(), 1);
            assert!(rpc_scope.aud.contains(&RpcAudience::All));
        } else {
            panic!("Expected Scope::Rpc, got {:?}", scope);
        }

        // Verify reconstruction and normalization work.
        // Normalized form expands bare `rpc` to explicit `rpc:*`.
        let normalized = scopes.to_normalized_string();
        assert_eq!(normalized, "rpc:*");
    }

    #[cfg(feature = "scope-check")]
    #[test]
    fn test_expand_permission_set_repo() {
        use jacquard_lexicon::lexicon::{LexPermission, LexPermissionResource, LexPermissionSet};

        // Create a simple permission set with a repo permission
        let mut perms = Vec::new();
        perms.push(LexPermission::Permission {
            resource: LexPermissionResource::Repo {
                collection: vec![
                    Nsid::new_static("app.bsky.feed.post").unwrap(),
                    Nsid::new_static("app.bsky.graph.follow").unwrap(),
                ],
                action: Some(vec![RepoAction::Create]),
            },
        });

        let perm_set = LexPermissionSet {
            title: None,
            title_lang: None,
            detail: None,
            detail_lang: None,
            permissions: perms,
        };

        let scopes = expand_permission_set(&perm_set, None).unwrap();
        assert_eq!(scopes.len(), 2);

        // Check that we got the expected repo scopes
        let mut found_post = false;
        let mut found_follow = false;

        for scope in &scopes {
            if let Scope::Repo(repo_scope) = scope {
                if let RepoCollection::Nsid(nsid) = &repo_scope.collection {
                    if nsid.as_ref() == "app.bsky.feed.post" {
                        assert_eq!(repo_scope.actions.len(), 1);
                        assert!(repo_scope.actions.contains(&RepoAction::Create));
                        found_post = true;
                    } else if nsid.as_ref() == "app.bsky.graph.follow" {
                        assert_eq!(repo_scope.actions.len(), 1);
                        assert!(repo_scope.actions.contains(&RepoAction::Create));
                        found_follow = true;
                    }
                }
            }
        }

        assert!(found_post, "Expected post scope");
        assert!(found_follow, "Expected follow scope");
    }

    #[cfg(feature = "scope-check")]
    #[test]
    fn test_expand_permission_set_identity() {
        use jacquard_lexicon::lexicon::{LexPermission, LexPermissionResource, LexPermissionSet};

        let mut perms = Vec::new();
        perms.push(LexPermission::Permission {
            resource: LexPermissionResource::Identity {
                attr: CowStr::Borrowed("handle"),
            },
        });

        let perm_set = LexPermissionSet {
            title: None,
            title_lang: None,
            detail: None,
            detail_lang: None,
            permissions: perms,
        };

        let scopes = expand_permission_set(&perm_set, None).unwrap();
        assert_eq!(scopes.len(), 1);

        assert_eq!(scopes[0], Scope::Identity(IdentityScope::Handle));
    }

    #[cfg(feature = "scope-check")]
    #[test]
    fn test_expand_permission_set_account() {
        use jacquard_lexicon::lexicon::{LexPermission, LexPermissionResource, LexPermissionSet};

        let mut perms = Vec::new();
        perms.push(LexPermission::Permission {
            resource: LexPermissionResource::Account {
                attr: CowStr::Borrowed("email"),
                action: Some(vec![AccountAction::Manage]),
            },
        });

        let perm_set = LexPermissionSet {
            title: None,
            title_lang: None,
            detail: None,
            detail_lang: None,
            permissions: perms,
        };

        let scopes = expand_permission_set(&perm_set, None).unwrap();
        assert_eq!(scopes.len(), 1);

        assert_eq!(
            scopes[0],
            Scope::Account(AccountScope {
                resource: AccountResource::Email,
                action: AccountAction::Manage,
            })
        );
    }

    #[cfg(feature = "scope-check")]
    #[test]
    fn test_expand_permission_set_account_highest_privilege() {
        // Regression test: when both Read and Manage are in the action list,
        // the highest privilege (Manage) must be selected, not the first.
        use jacquard_lexicon::lexicon::{LexPermission, LexPermissionResource, LexPermissionSet};

        let perm_set = LexPermissionSet {
            title: None,
            title_lang: None,
            detail: None,
            detail_lang: None,
            permissions: vec![LexPermission::Permission {
                resource: LexPermissionResource::Account {
                    attr: CowStr::Borrowed("email"),
                    action: Some(vec![AccountAction::Read, AccountAction::Manage]),
                },
            }],
        };

        let scopes = expand_permission_set(&perm_set, None).unwrap();
        assert_eq!(scopes.len(), 1);
        assert_eq!(
            scopes[0],
            Scope::Account(AccountScope {
                resource: AccountResource::Email,
                action: AccountAction::Manage,
            }),
            "should select Manage (highest privilege), not Read (first in list)"
        );
    }

    #[cfg(feature = "scope-check")]
    #[test]
    fn test_expand_permission_set_rpc_with_inherit_aud() {
        use jacquard_lexicon::lexicon::{LexPermission, LexPermissionResource, LexPermissionSet};

        let mut perms = Vec::new();
        perms.push(LexPermission::Permission {
            resource: LexPermissionResource::Rpc {
                lxm: vec![Nsid::new_static("app.bsky.feed.getTimeline").unwrap()],
                aud: None,
                inherit_aud: Some(true),
            },
        });

        let perm_set = LexPermissionSet {
            title: None,
            title_lang: None,
            detail: None,
            detail_lang: None,
            permissions: perms,
        };

        let inherited_did = DidService::new_static("did:web:example.com").unwrap();
        let scopes = expand_permission_set(&perm_set, Some(&inherited_did)).unwrap();
        assert_eq!(scopes.len(), 1);

        if let Scope::Rpc(rpc_scope) = &scopes[0] {
            assert_eq!(rpc_scope.lxm.len(), 1);
            assert_eq!(rpc_scope.aud.len(), 1);
            assert!(
                matches!(rpc_scope.aud.iter().next(), Some(RpcAudience::Did(d)) if d.as_ref() == "did:web:example.com")
            );
        } else {
            panic!("Expected Rpc scope");
        }
    }

    #[cfg(feature = "scope-check")]
    #[test]
    fn test_expand_permission_set_rpc_explicit_aud() {
        use jacquard_lexicon::lexicon::{LexPermission, LexPermissionResource, LexPermissionSet};

        let mut perms = Vec::new();
        perms.push(LexPermission::Permission {
            resource: LexPermissionResource::Rpc {
                lxm: vec![Nsid::new_static("app.bsky.feed.getTimeline").unwrap()],
                aud: Some(DidService::new_static("did:web:custom.com").unwrap()),
                inherit_aud: None,
            },
        });

        let perm_set = LexPermissionSet {
            title: None,
            title_lang: None,
            detail: None,
            detail_lang: None,
            permissions: perms,
        };

        let scopes = expand_permission_set(&perm_set, None).unwrap();
        assert_eq!(scopes.len(), 1);

        if let Scope::Rpc(rpc_scope) = &scopes[0] {
            assert_eq!(rpc_scope.aud.len(), 1);
            assert!(
                matches!(rpc_scope.aud.iter().next(), Some(RpcAudience::Did(d)) if d.as_ref() == "did:web:custom.com")
            );
        } else {
            panic!("Expected Rpc scope");
        }
    }

    #[cfg(feature = "scope-check")]
    #[test]
    fn test_expand_permission_set_unknown_identity_attr() {
        use jacquard_lexicon::lexicon::{LexPermission, LexPermissionResource, LexPermissionSet};

        let mut perms = Vec::new();
        perms.push(LexPermission::Permission {
            resource: LexPermissionResource::Identity {
                attr: CowStr::Borrowed("invalid"),
            },
        });

        let perm_set = LexPermissionSet {
            title: None,
            title_lang: None,
            detail: None,
            detail_lang: None,
            permissions: perms,
        };

        let result = expand_permission_set(&perm_set, None);
        assert!(matches!(
            result,
            Err(PermissionSetConversionError::UnknownIdentityAttr(_))
        ));
    }

    #[cfg(feature = "scope-check")]
    #[test]
    fn test_expand_permission_set_unknown_account_attr() {
        use jacquard_lexicon::lexicon::{LexPermission, LexPermissionResource, LexPermissionSet};

        let mut perms = Vec::new();
        perms.push(LexPermission::Permission {
            resource: LexPermissionResource::Account {
                attr: CowStr::Borrowed("invalid"),
                action: None,
            },
        });

        let perm_set = LexPermissionSet {
            title: None,
            title_lang: None,
            detail: None,
            detail_lang: None,
            permissions: perms,
        };

        let result = expand_permission_set(&perm_set, None);
        assert!(matches!(
            result,
            Err(PermissionSetConversionError::UnknownAccountAttr(_))
        ));
    }

    #[cfg(feature = "scope-check")]
    #[test]
    fn test_expand_permission_set_blob() {
        use jacquard_common::types::blob::MimeType;
        use jacquard_lexicon::lexicon::{LexPermission, LexPermissionResource, LexPermissionSet};

        // Test exact type
        let mut perms = Vec::new();
        perms.push(LexPermission::Permission {
            resource: LexPermissionResource::Blob {
                accept: vec![MimeType::new(CowStr::Borrowed("image/png"))],
                max_size: None,
            },
        });

        let perm_set = LexPermissionSet {
            title: None,
            title_lang: None,
            detail: None,
            detail_lang: None,
            permissions: perms,
        };

        let scopes = expand_permission_set(&perm_set, None).expect("should expand blob");
        assert_eq!(scopes.len(), 1);
        match &scopes[0] {
            Scope::Blob(blob_scope) => {
                assert_eq!(blob_scope.accept.len(), 1);
                for pattern in &blob_scope.accept {
                    if let MimePattern::Exact(s) = pattern {
                        assert_eq!(s.as_ref() as &str, "image/png");
                    } else {
                        panic!("expected Exact pattern");
                    }
                }
            }
            _ => panic!("expected Blob scope"),
        }

        // Test type wildcard
        let mut perms = Vec::new();
        perms.push(LexPermission::Permission {
            resource: LexPermissionResource::Blob {
                accept: vec![MimeType::new(CowStr::Borrowed("image/*"))],
                max_size: None,
            },
        });

        let perm_set = LexPermissionSet {
            title: None,
            title_lang: None,
            detail: None,
            detail_lang: None,
            permissions: perms,
        };

        let scopes = expand_permission_set(&perm_set, None).expect("should expand blob");
        assert_eq!(scopes.len(), 1);
        match &scopes[0] {
            Scope::Blob(blob_scope) => {
                assert_eq!(blob_scope.accept.len(), 1);
                // TypeWildcard should store only the type prefix (e.g., "image")
                for pattern in &blob_scope.accept {
                    if let MimePattern::TypeWildcard(s) = pattern {
                        assert_eq!(s.as_ref() as &str, "image");
                    } else {
                        panic!("expected TypeWildcard pattern");
                    }
                }
            }
            _ => panic!("expected Blob scope"),
        }

        // Test all wildcard
        let mut perms = Vec::new();
        perms.push(LexPermission::Permission {
            resource: LexPermissionResource::Blob {
                accept: vec![MimeType::new(CowStr::Borrowed("*/*"))],
                max_size: None,
            },
        });

        let perm_set = LexPermissionSet {
            title: None,
            title_lang: None,
            detail: None,
            detail_lang: None,
            permissions: perms,
        };

        let scopes = expand_permission_set(&perm_set, None).expect("should expand blob");
        assert_eq!(scopes.len(), 1);
        match &scopes[0] {
            Scope::Blob(blob_scope) => {
                assert_eq!(blob_scope.accept.len(), 1);
                assert!(
                    blob_scope
                        .accept
                        .iter()
                        .any(|p| matches!(p, MimePattern::All))
                );
            }
            _ => panic!("expected Blob scope"),
        }
    }

    #[cfg(feature = "scope-check")]
    #[test]
    fn test_expand_permission_set_blob_invalid_mime() {
        use jacquard_common::types::blob::MimeType;
        use jacquard_lexicon::lexicon::{LexPermission, LexPermissionResource, LexPermissionSet};

        let mut perms = Vec::new();
        perms.push(LexPermission::Permission {
            resource: LexPermissionResource::Blob {
                accept: vec![MimeType::new(CowStr::Borrowed("invalid-mime-type"))],
                max_size: None,
            },
        });

        let perm_set = LexPermissionSet {
            title: None,
            title_lang: None,
            detail: None,
            detail_lang: None,
            permissions: perms,
        };

        let result = expand_permission_set(&perm_set, None);
        assert!(matches!(
            result,
            Err(PermissionSetConversionError::InvalidMimePattern(_))
        ));
    }
}
