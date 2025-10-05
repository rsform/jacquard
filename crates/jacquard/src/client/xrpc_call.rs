use bytes::Bytes;
use http::{HeaderName, HeaderValue};
use url::Url;

use crate::CowStr;
use crate::client::{self as super_mod, Response, error};
use crate::client::{AuthorizationToken, HttpClient};
use jacquard_common::types::xrpc::XrpcRequest;

/// Per-request options for XRPC calls.
#[derive(Debug, Default, Clone)]
pub struct CallOptions<'a> {
    /// Optional Authorization to apply (`Bearer` or `DPoP`).
    pub auth: Option<AuthorizationToken<'a>>,
    /// `atproto-proxy` header value.
    pub atproto_proxy: Option<CowStr<'a>>,
    /// `atproto-accept-labelers` header values.
    pub atproto_accept_labelers: Option<Vec<CowStr<'a>>>,
    /// Extra headers to attach to this request.
    pub extra_headers: Vec<(HeaderName, HeaderValue)>,
}

/// Extension for stateless XRPC calls on any `HttpClient`.
///
/// Example
/// ```ignore
/// use jacquard::client::XrpcExt;
/// use jacquard::api::app_bsky::feed::get_author_feed::GetAuthorFeed;
/// use jacquard::types::ident::AtIdentifier;
/// use miette::IntoDiagnostic;
///
/// #[tokio::main]
/// async fn main() -> miette::Result<()> {
///     let http = reqwest::Client::new();
///     let base = url::Url::parse("https://public.api.bsky.app")?;
///     let resp = http
///         .xrpc(base)
///         .send(
///             GetAuthorFeed::new()
///                 .actor(AtIdentifier::new_static("pattern.atproto.systems").unwrap())
///                 .limit(5)
///                 .build(),
///         )
///         .await?;
///     let out = resp.into_output()?;
///     println!("author feed:\n{}", serde_json::to_string_pretty(&out).into_diagnostic()?);
///     Ok(())
/// }
/// ```
pub trait XrpcExt: HttpClient {
    /// Start building an XRPC call for the given base URL.
    fn xrpc<'a>(&'a self, base: Url) -> XrpcCall<'a, Self>
    where
        Self: Sized,
    {
        XrpcCall {
            client: self,
            base,
            opts: CallOptions::default(),
        }
    }
}

impl<T: HttpClient> XrpcExt for T {}

/// Stateless XRPC call builder.
///
/// Example (per-request overrides)
/// ```ignore
/// use jacquard::client::{XrpcExt, AuthorizationToken};
/// use jacquard::api::app_bsky::feed::get_author_feed::GetAuthorFeed;
/// use jacquard::types::ident::AtIdentifier;
/// use jacquard::CowStr;
/// use miette::IntoDiagnostic;
///
/// #[tokio::main]
/// async fn main() -> miette::Result<()> {
///     let http = reqwest::Client::new();
///     let base = url::Url::parse("https://public.api.bsky.app")?;
///     let resp = http
///         .xrpc(base)
///         .auth(AuthorizationToken::Bearer(CowStr::from("ACCESS_JWT")))
///         .accept_labelers(vec![CowStr::from("did:plc:labelerid")])
///         .header(http::header::USER_AGENT, http::HeaderValue::from_static("jacquard-example"))
///         .send(
///             GetAuthorFeed::new()
///                 .actor(AtIdentifier::new_static("pattern.atproto.systems").unwrap())
///                 .limit(5)
///                 .build(),
///         )
///         .await?;
///     let out = resp.into_output()?;
///     println!("{}", serde_json::to_string_pretty(&out).into_diagnostic()?);
///     Ok(())
/// }
/// ```
pub struct XrpcCall<'a, C: HttpClient> {
    pub(crate) client: &'a C,
    pub(crate) base: Url,
    pub(crate) opts: CallOptions<'a>,
}

impl<'a, C: HttpClient> XrpcCall<'a, C> {
    /// Apply Authorization to this call.
    pub fn auth(mut self, token: AuthorizationToken<'a>) -> Self {
        self.opts.auth = Some(token);
        self
    }
    /// Set `atproto-proxy` header for this call.
    pub fn proxy(mut self, proxy: CowStr<'a>) -> Self {
        self.opts.atproto_proxy = Some(proxy);
        self
    }
    /// Set `atproto-accept-labelers` header(s) for this call.
    pub fn accept_labelers(mut self, labelers: Vec<CowStr<'a>>) -> Self {
        self.opts.atproto_accept_labelers = Some(labelers);
        self
    }
    /// Add an extra header.
    pub fn header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.opts.extra_headers.push((name, value));
        self
    }
    /// Replace the builder's options entirely.
    pub fn with_options(mut self, opts: CallOptions<'a>) -> Self {
        self.opts = opts;
        self
    }

    /// Send the given typed XRPC request and return a response wrapper.
    pub async fn send<R: XrpcRequest + Send>(self, request: R) -> super_mod::Result<Response<R>> {
        let http_request = super_mod::build_http_request(&self.base, &request, &self.opts)
            .map_err(error::TransportError::from)?;

        let http_response = self
            .client
            .send_http(http_request)
            .await
            .map_err(|e| error::TransportError::Other(Box::new(e)))?;

        let status = http_response.status();
        let buffer = Bytes::from(http_response.into_body());

        if !status.is_success() && !matches!(status.as_u16(), 400 | 401) {
            return Err(error::HttpError {
                status,
                body: Some(buffer),
            }
            .into());
        }

        Ok(Response::new(buffer, status))
    }
}
