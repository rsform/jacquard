//! Provider fixture model: deterministic identities and run coordinates
//! exported by the lifecycle controller.

use std::fmt;

/// The Tranquil fixture's did:web identity. Tranquil fetches did:web
/// documents over plain HTTP when the domain starts with `localhost` (its
/// own documented loopback exception); `localhost.jacquard-e2e.test` rides
/// that exception while remaining a real DNS name inside the bridge. The
/// proxy forwards port 80 (Tranquil's HTTP fetch) to the ingress HTTP
/// listener and port 443 (host-side TLS resolution) to the ingress TLS
/// listener.
pub const TRANQUIL_IDENTITY_DID: &str = "did:web:localhost.jacquard-e2e.test";

pub const TRANQUIL_MEMBER_DID: &str = "did:web:localhost.jacquard-e2e.test:member";

/// Service DID used by the Tranquil proxy scenario. Its document is served by
/// the native ingress and names the generated endpoint fragment.
pub const TRANQUIL_SERVICE_DID: &str = "did:web:localhost.jacquard-e2e.test:service";

/// Which provider fixture a scenario run targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    /// Tranquil PDS (`atcr.io/tranquil.farm/tranquil-pds`).
    Tranquil,
    /// Reference PDS spaces-alpha (`ghcr.io/bluesky-social/atproto:pds-spaces-alpha`).
    Reference,
}

impl Provider {
    /// Stable identifier used in environment variables, Compose profiles,
    /// nextest filters, and artifact paths.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Tranquil => "tranquil",
            Self::Reference => "reference",
        }
    }

    /// Parse from the lifecycle controller's `JACQUARD_E2E_PROVIDER`.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "tranquil" => Some(Self::Tranquil),
            "reference" => Some(Self::Reference),
            _ => None,
        }
    }

    /// Deterministic fixture identity for this provider. DIDs, handles, and
    /// emails are distinct per provider so concurrent runs cannot cross-wire
    /// resolver caches. None of them touch PLC. The Tranquil DID rides
    /// Tranquil's `localhost`-prefix HTTP exception (see
    /// [`TRANQUIL_IDENTITY_DID`]).
    pub const fn primary_identity(&self) -> FixtureIdentity {
        match self {
            Self::Tranquil => FixtureIdentity {
                did: TRANQUIL_IDENTITY_DID,
                handle: "primary.tranquil.jacquard-e2e.test",
                email: "primary@tranquil.jacquard-e2e.test",
            },
            Self::Reference => FixtureIdentity {
                did: "did:web:reference-identity.jacquard-e2e.test",
                handle: "primary.reference.jacquard-e2e.test",
                email: "primary@reference.jacquard-e2e.test",
            },
        }
    }

    /// Second deterministic identity, used only by the reference provider for
    /// authenticated spaces membership boundaries.
    pub const fn member_identity(&self) -> FixtureIdentity {
        match self {
            Self::Tranquil => FixtureIdentity {
                did: TRANQUIL_MEMBER_DID,
                handle: "member.tranquil.jacquard-e2e.test",
                email: "member@tranquil.jacquard-e2e.test",
            },
            Self::Reference => FixtureIdentity {
                did: "did:web:reference-member.jacquard-e2e.test",
                handle: "member.reference.jacquard-e2e.test",
                email: "member@reference.jacquard-e2e.test",
            },
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A deterministic fixture identity imported into a fresh provider volume.
///
/// Signing material lives as fixture data under `e2e/identities/`; the DID
/// document's `#atproto_pds` service always names the owning provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixtureIdentity {
    /// The stable `did:web` identifier.
    pub did: &'static str,
    /// The account handle.
    pub handle: &'static str,
    /// The account email.
    pub email: &'static str,
}

/// Non-secret run coordinates exported by the lifecycle controller.
///
/// Secrets (app passwords, signing keys) never travel through the
/// environment; they are read from fixture files whose root the controller
/// exports as `JACQUARD_E2E_FIXTURE_ROOT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunCoordinates {
    /// Unique run id (also the Compose project name and artifact directory).
    pub run_id: String,
    /// Provider base URL as seen from the host test process.
    pub provider_url: String,
    /// Effective image digest the provider was launched from (recorded by the
    /// controller after tag resolution or an explicit override).
    pub effective_digest: String,
    /// Root directory of fixture data (identities, CA, credentials).
    pub fixture_root: String,
    /// Directory for run diagnostics retained on failure.
    pub artifact_dir: String,
}

impl RunCoordinates {
    /// Read run coordinates from the environment as exported by
    /// `scripts/e2e.sh`. Every variable is required: a scenario must never
    /// guess its fixture topology.
    pub fn from_env() -> Result<Self, String> {
        let req =
            |key: &str| std::env::var(key).map_err(|_| format!("missing required env var {key}"));
        Ok(Self {
            run_id: req("JACQUARD_E2E_RUN_ID")?,
            provider_url: req("JACQUARD_E2E_PROVIDER_URL")?,
            effective_digest: req("JACQUARD_E2E_EFFECTIVE_DIGEST")?,
            fixture_root: req("JACQUARD_E2E_FIXTURE_ROOT")?,
            artifact_dir: req("JACQUARD_E2E_ARTIFACT_DIR")?,
        })
    }
}

/// Fully resolved provider context for one scenario run.
#[derive(Debug, Clone)]
pub struct ProviderContext {
    /// The provider fixture.
    pub provider: Provider,
    /// The provider's stable primary identity.
    pub identity: FixtureIdentity,
    /// Run coordinates from the lifecycle controller.
    pub coordinates: RunCoordinates,
}

impl ProviderContext {
    /// Resolve the provider context from lifecycle environment variables.
    pub fn from_env() -> Result<Self, String> {
        let provider = Provider::from_name(
            &std::env::var("JACQUARD_E2E_PROVIDER")
                .map_err(|_| "missing required env var JACQUARD_E2E_PROVIDER")?,
        )
        .ok_or_else(|| "unknown JACQUARD_E2E_PROVIDER".to_string())?;

        Ok(Self {
            provider,
            identity: provider.primary_identity(),
            coordinates: RunCoordinates::from_env()?,
        })
    }
}
