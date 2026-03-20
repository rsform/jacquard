use jacquard_common::error::{AuthError, ClientError, ClientErrorKind};
use jacquard_common::types::did::Did;
use jacquard_common::types::nsid::Nsid;
use jacquard_common::types::string::{RecordKey, Rkey};
use jacquard_common::xrpc::XrpcError;
use jacquard_common::{Data, IntoStatic};
use smol_str::SmolStr;

/// Boxed error type for wrapping arbitrary errors
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Error type for Agent convenience methods
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub struct AgentError {
    #[diagnostic_source]
    kind: AgentErrorKind,
    #[source]
    source: Option<BoxError>,
    #[help]
    help: Option<SmolStr>,
    context: Option<SmolStr>,
    url: Option<SmolStr>,
    details: Option<SmolStr>,
    location: Option<SmolStr>,
    xrpc: Option<Data<'static>>,
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)?;

        // Add context if available
        if let Some(context) = &self.context {
            write!(f, ": {}", context)?;
        }

        // Add URL if available
        if let Some(url) = &self.url {
            write!(f, " (url: {})", url)?;
        }

        // Add details if available
        if let Some(details) = &self.details {
            write!(f, " [{}]", details)?;
        }

        Ok(())
    }
}

/// Error categories for Agent operations
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum AgentErrorKind {
    /// Transport/network layer failure
    #[error("client error (see context for details)")]
    #[diagnostic(
        code(jacquard::agent::client),
        help("check source error and context for specific failure details")
    )]
    Client,

    /// No session available for operations requiring authentication
    #[error("no session available")]
    #[diagnostic(
        code(jacquard::agent::no_session),
        help("ensure agent is authenticated before performing operations")
    )]
    NoSession,

    /// Authentication error from XRPC layer
    #[error("auth error: {0}")]
    #[diagnostic(code(jacquard::agent::auth))]
    Auth(AuthError),

    /// Record operation failed with typed error from endpoint
    #[error("record operation failed on {collection}/{rkey:?} in repo {repo}")]
    #[diagnostic(code(jacquard::agent::record_operation))]
    RecordOperation {
        /// The repository DID
        repo: Did<'static>,
        /// The collection NSID
        collection: Nsid<'static>,
        /// The record key
        rkey: RecordKey<Rkey<'static>>,
    },

    /// Multi-step operation failed at sub-step (e.g., get failed in update_record)
    #[error("operation failed at step '{step}'")]
    #[diagnostic(code(jacquard::agent::sub_operation))]
    SubOperation {
        /// Description of which step failed
        step: &'static str,
    },
    /// XRPC error
    #[error("xrpc error")]
    #[diagnostic(code(jacquard::agent::xrpc))]
    XrpcError,
}

impl AgentError {
    /// Create a new error with the given kind and optional source
    pub fn new(kind: AgentErrorKind, source: Option<BoxError>) -> Self {
        Self {
            kind,
            source,
            help: None,
            context: None,
            url: None,
            details: None,
            location: None,
            xrpc: None,
        }
    }

    /// Get the error kind
    pub fn kind(&self) -> &AgentErrorKind {
        &self.kind
    }

    /// Get the source error if present
    pub fn source_err(&self) -> Option<&BoxError> {
        self.source.as_ref()
    }

    /// Get the context string if present
    pub fn context(&self) -> Option<&str> {
        self.context.as_ref().map(|s| s.as_str())
    }

    /// Get the URL if present
    pub fn url(&self) -> Option<&str> {
        self.url.as_ref().map(|s| s.as_str())
    }

    /// Get the details if present
    pub fn details(&self) -> Option<&str> {
        self.details.as_ref().map(|s| s.as_str())
    }

    /// Get the location if present
    pub fn location(&self) -> Option<&str> {
        self.location.as_ref().map(|s| s.as_str())
    }

    /// Add help text to this error
    pub fn with_help(mut self, help: impl Into<SmolStr>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Add context to this error
    pub fn with_context(mut self, context: impl Into<SmolStr>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Add URL to this error
    pub fn with_url(mut self, url: impl Into<SmolStr>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Add details to this error
    pub fn with_details(mut self, details: impl Into<SmolStr>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Add location to this error
    pub fn with_location(mut self, location: impl Into<SmolStr>) -> Self {
        self.location = Some(location.into());
        self
    }

    /// Append additional context to existing context string.
    ///
    /// If context already exists, appends with ": " separator.
    /// If no context exists, sets it directly.
    pub fn append_context(mut self, additional: impl AsRef<str>) -> Self {
        self.context = Some(match self.context.take() {
            Some(existing) => smol_str::format_smolstr!("{}: {}", existing, additional.as_ref()),
            None => additional.as_ref().into(),
        });
        self
    }

    /// Add NSID context for XRPC operations.
    ///
    /// Appends the NSID in brackets to existing context, e.g. `"network timeout: [com.atproto.repo.getRecord]"`.
    pub fn for_nsid(self, nsid: &str) -> Self {
        self.append_context(smol_str::format_smolstr!("[{}]", nsid))
    }

    /// Add collection context for record operations.
    ///
    /// Use this when a record operation fails to indicate the target collection.
    pub fn for_collection(self, operation: &str, collection_nsid: &str) -> Self {
        self.append_context(smol_str::format_smolstr!(
            "{} [{}]",
            operation,
            collection_nsid
        ))
    }

    /// Add XRPC error data to this error for observability
    pub fn with_xrpc<E>(mut self, xrpc: XrpcError<E>) -> Self
    where
        E: std::error::Error + jacquard_common::IntoStatic + serde::Serialize,
    {
        use jacquard_common::types::value::to_data;
        // Attempt to serialize XrpcError to Data for observability
        if let Ok(data) = to_data(&xrpc) {
            self.xrpc = Some(data.into_static());
        }
        self
    }

    /// Create an XRPC error with attached error data for observability
    pub fn xrpc<E>(error: XrpcError<E>) -> Self
    where
        E: std::error::Error + jacquard_common::IntoStatic + serde::Serialize + Send + Sync,
        <E as IntoStatic>::Output: IntoStatic + std::error::Error + Send + Sync,
    {
        use jacquard_common::types::value::to_data;
        // Attempt to serialize XrpcError to Data for observability
        if let Ok(data) = to_data(&error) {
            let mut error = Self::new(
                AgentErrorKind::XrpcError,
                Some(Box::new(error.into_static())),
            );
            error.xrpc = Some(data.into_static());
            error
        } else {
            Self::new(
                AgentErrorKind::XrpcError,
                Some(Box::new(error.into_static())),
            )
        }
    }

    // Constructors

    /// Create a no session error
    pub fn no_session() -> Self {
        Self::new(AgentErrorKind::NoSession, None)
    }

    /// Create a sub-operation error for multi-step operations
    pub fn sub_operation(
        step: &'static str,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::new(
            AgentErrorKind::SubOperation { step },
            Some(Box::new(source)),
        )
    }

    /// Create a record operation error
    pub fn record_operation(
        repo: Did<'static>,
        collection: Nsid<'static>,
        rkey: RecordKey<Rkey<'static>>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::new(
            AgentErrorKind::RecordOperation {
                repo,
                collection,
                rkey,
            },
            Some(Box::new(source)),
        )
    }

    /// Create an authentication error
    pub fn auth(auth_error: AuthError) -> Self {
        Self::new(AgentErrorKind::Auth(auth_error), None)
    }
}

impl From<ClientError> for AgentError {
    fn from(e: ClientError) -> Self {
        use smol_str::ToSmolStr;

        let mut context_msg: SmolStr;
        let help_msg: SmolStr;
        let url = e.url().map(|s| s.to_smolstr());
        let details = e.details().map(|s| s.to_smolstr());
        // Preserve original context from ClientError to append later
        let original_context = e.context().map(|s| s.to_smolstr());

        // Build context and help based on the error kind
        match e.kind() {
            ClientErrorKind::Transport => {
                help_msg = "check network connectivity and server availability".to_smolstr();
                context_msg = "network/transport error during request".to_smolstr();
            }
            ClientErrorKind::InvalidRequest(msg) => {
                help_msg = "verify request parameters are valid".to_smolstr();
                context_msg = smol_str::format_smolstr!("invalid request: {}", msg);
            }
            ClientErrorKind::Encode(msg) => {
                help_msg = "check request body format".to_smolstr();
                context_msg = smol_str::format_smolstr!("failed to encode request: {}", msg);
            }
            ClientErrorKind::Decode(msg) => {
                help_msg = "server returned unexpected response format".to_smolstr();
                context_msg = smol_str::format_smolstr!("failed to decode response: {}", msg);
            }
            ClientErrorKind::Http { status } => {
                help_msg = match status.as_u16() {
                    400..=499 => "check request parameters and authentication",
                    500..=599 => "server error - try again later or check server logs",
                    _ => "unexpected HTTP status code",
                }
                .to_smolstr();
                context_msg = smol_str::format_smolstr!("HTTP error {}", status);
            }
            ClientErrorKind::Auth(auth_err) => {
                help_msg = "verify authentication credentials and session".to_smolstr();
                context_msg = smol_str::format_smolstr!("authentication error: {}", auth_err);
            }
            ClientErrorKind::IdentityResolution => {
                help_msg = "check handle/DID is valid and resolvable".to_smolstr();
                context_msg = "identity resolution failed".to_smolstr();
            }
            ClientErrorKind::Storage => {
                help_msg = "verify storage backend is accessible".to_smolstr();
                context_msg = "storage operation failed".to_smolstr();
            }
            _ => {
                help_msg = "see source error for details".to_smolstr();
                context_msg = "client error".to_smolstr();
            }
        }

        // Append original context from ClientError if present
        if let Some(original) = original_context {
            context_msg = smol_str::format_smolstr!("{}: {}", context_msg, original);
        }

        let mut error = Self::new(AgentErrorKind::Client, Some(Box::new(e)));
        error = error.with_context(context_msg);
        error = error.with_help(help_msg);

        if let Some(url) = url {
            error = error.with_url(url);
        }
        if let Some(details) = details {
            error = error.with_details(details);
        }

        error
    }
}

impl From<AuthError> for AgentError {
    fn from(e: AuthError) -> Self {
        Self::new(AgentErrorKind::Auth(e), None)
            .with_help("check authentication credentials and session state")
    }
}

/// Result type for Agent operations
pub type Result<T> = core::result::Result<T, AgentError>;

impl IntoStatic for AgentError {
    type Output = AgentError;

    fn into_static(self) -> Self::Output {
        match self.kind {
            AgentErrorKind::RecordOperation {
                repo,
                collection,
                rkey,
            } => Self {
                kind: AgentErrorKind::RecordOperation {
                    repo: repo.into_static(),
                    collection: collection.into_static(),
                    rkey: rkey.into_static(),
                },
                source: self.source,
                help: self.help,
                context: self.context,
                url: self.url,
                details: self.details,
                location: self.location,
                xrpc: self.xrpc,
            },
            AgentErrorKind::Auth(auth) => Self {
                kind: AgentErrorKind::Auth(auth.into_static()),
                source: self.source,
                help: self.help,
                context: self.context,
                url: self.url,
                details: self.details,
                location: self.location,
                xrpc: self.xrpc,
            },
            _ => self,
        }
    }
}
