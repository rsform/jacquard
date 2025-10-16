#[cfg(not(target_arch = "wasm32"))]
use std::future::Future;

use crate::types::{OAuthAuthorizationServerMetadata, OAuthProtectedResourceMetadata};
use http::{Request, StatusCode};
use jacquard_common::CowStr;
use jacquard_common::types::did_doc::DidDocument;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::{IntoStatic, error::TransportError};
use jacquard_common::{http_client::HttpClient, types::did::Did};
use jacquard_identity::resolver::{IdentityError, IdentityResolver};
use url::Url;

/// Compare two issuer strings strictly but without spuriously failing on trivial differences.
///
/// Rules:
/// - Schemes must match exactly.
/// - Hostnames and effective ports must match (treat missing port the same as default port).
/// - Path must match, except that an empty path and `/` are equivalent.
/// - Query/fragment are not considered; if present on either side, the comparison fails.
pub(crate) fn issuer_equivalent(a: &str, b: &str) -> bool {
    fn normalize(url: &Url) -> Option<(String, String, u16, String)> {
        if url.query().is_some() || url.fragment().is_some() {
            return None;
        }
        let scheme = url.scheme().to_string();
        let host = url.host_str()?.to_string();
        let port = url.port_or_known_default()?;
        let path = match url.path() {
            "" => "/".to_string(),
            "/" => "/".to_string(),
            other => other.to_string(),
        };
        Some((scheme, host, port, path))
    }

    match (Url::parse(a), Url::parse(b)) {
        (Ok(ua), Ok(ub)) => match (normalize(&ua), normalize(&ub)) {
            (Some((sa, ha, pa, pa_path)), Some((sb, hb, pb, pb_path))) => {
                if sa != sb || ha != hb || pa != pb {
                    return false;
                }
                if pa_path == "/" && pb_path == "/" {
                    return true;
                }
                pa_path == pb_path
            }
            _ => false,
        },
        _ => a == b,
    }
}

#[derive(thiserror::Error, Debug, miette::Diagnostic)]
pub enum ResolverError {
    #[error("resource not found")]
    #[diagnostic(
        code(jacquard_oauth::resolver::not_found),
        help("check the base URL or identifier")
    )]
    NotFound,
    #[error("invalid at identifier: {0}")]
    #[diagnostic(
        code(jacquard_oauth::resolver::at_identifier),
        help("ensure a valid handle or DID was provided")
    )]
    AtIdentifier(String),
    #[error("invalid did: {0}")]
    #[diagnostic(
        code(jacquard_oauth::resolver::did),
        help("ensure DID is correctly formed (did:plc or did:web)")
    )]
    Did(String),
    #[error("invalid did document: {0}")]
    #[diagnostic(
        code(jacquard_oauth::resolver::did_document),
        help("verify the DID document structure and service entries")
    )]
    DidDocument(String),
    #[error("protected resource metadata is invalid: {0}")]
    #[diagnostic(
        code(jacquard_oauth::resolver::protected_resource_metadata),
        help("PDS must advertise an authorization server in its protected resource metadata")
    )]
    ProtectedResourceMetadata(String),
    #[error("authorization server metadata is invalid: {0}")]
    #[diagnostic(
        code(jacquard_oauth::resolver::authorization_server_metadata),
        help("issuer must match and include the PDS resource")
    )]
    AuthorizationServerMetadata(String),
    #[error("error resolving identity: {0}")]
    #[diagnostic(code(jacquard_oauth::resolver::identity))]
    IdentityResolverError(#[from] IdentityError),
    #[error("unsupported did method: {0:?}")]
    #[diagnostic(
        code(jacquard_oauth::resolver::unsupported_did_method),
        help("supported DID methods: did:web, did:plc")
    )]
    UnsupportedDidMethod(Did<'static>),
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::resolver::transport))]
    Transport(#[from] TransportError),
    #[error("http status: {0:?}")]
    #[diagnostic(
        code(jacquard_oauth::resolver::http_status),
        help("check well-known paths and server configuration")
    )]
    HttpStatus(StatusCode),
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::resolver::serde_json))]
    SerdeJson(#[from] serde_json::Error),
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::resolver::serde_form))]
    SerdeHtmlForm(#[from] serde_html_form::ser::Error),
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::resolver::url))]
    Uri(#[from] url::ParseError),
}

#[cfg(not(target_arch = "wasm32"))]
async fn verify_issuer_impl<T: OAuthResolver + Sync + ?Sized>(
    resolver: &T,
    server_metadata: &OAuthAuthorizationServerMetadata<'_>,
    sub: &Did<'_>,
) -> Result<Url, ResolverError> {
    let (metadata, identity) = resolver.resolve_from_identity(sub.as_str()).await?;
    if !issuer_equivalent(&metadata.issuer, &server_metadata.issuer) {
        return Err(ResolverError::AuthorizationServerMetadata(
            "issuer mismatch".to_string(),
        ));
    }
    Ok(identity
        .pds_endpoint()
        .ok_or(ResolverError::DidDocument(format!("{:?}", identity).into()))?)
}

#[cfg(target_arch = "wasm32")]
async fn verify_issuer_impl<T: OAuthResolver + ?Sized>(
    resolver: &T,
    server_metadata: &OAuthAuthorizationServerMetadata<'_>,
    sub: &Did<'_>,
) -> Result<Url, ResolverError> {
    let (metadata, identity) = resolver.resolve_from_identity(sub.as_str()).await?;
    if !issuer_equivalent(&metadata.issuer, &server_metadata.issuer) {
        return Err(ResolverError::AuthorizationServerMetadata(
            "issuer mismatch".to_string(),
        ));
    }
    Ok(identity
        .pds_endpoint()
        .ok_or(ResolverError::DidDocument(format!("{:?}", identity).into()))?)
}

#[cfg(not(target_arch = "wasm32"))]
async fn resolve_oauth_impl<T: OAuthResolver + Sync + ?Sized>(
    resolver: &T,
    input: &str,
) -> Result<
    (
        OAuthAuthorizationServerMetadata<'static>,
        Option<DidDocument<'static>>,
    ),
    ResolverError,
> {
    // Allow using an entryway, or PDS url, directly as login input (e.g.
    // when the user forgot their handle, or when the handle does not
    // resolve to a DID)
    Ok(if input.starts_with("https://") {
        let url = Url::parse(input).map_err(|_| ResolverError::NotFound)?;
        (resolver.resolve_from_service(&url).await?, None)
    } else {
        let (metadata, identity) = resolver.resolve_from_identity(input).await?;
        (metadata, Some(identity))
    })
}

#[cfg(target_arch = "wasm32")]
async fn resolve_oauth_impl<T: OAuthResolver + ?Sized>(
    resolver: &T,
    input: &str,
) -> Result<
    (
        OAuthAuthorizationServerMetadata<'static>,
        Option<DidDocument<'static>>,
    ),
    ResolverError,
> {
    // Allow using an entryway, or PDS url, directly as login input (e.g.
    // when the user forgot their handle, or when the handle does not
    // resolve to a DID)
    Ok(if input.starts_with("https://") {
        let url = Url::parse(input).map_err(|_| ResolverError::NotFound)?;
        (resolver.resolve_from_service(&url).await?, None)
    } else {
        let (metadata, identity) = resolver.resolve_from_identity(input).await?;
        (metadata, Some(identity))
    })
}

#[cfg(not(target_arch = "wasm32"))]
async fn resolve_from_service_impl<T: OAuthResolver + Sync + ?Sized>(
    resolver: &T,
    input: &Url,
) -> Result<OAuthAuthorizationServerMetadata<'static>, ResolverError> {
    // Assume first that input is a PDS URL (as required by ATPROTO)
    if let Ok(metadata) = resolver.get_resource_server_metadata(input).await {
        return Ok(metadata);
    }
    // Fallback to trying to fetch as an issuer (Entryway)
    resolver.get_authorization_server_metadata(input).await
}

#[cfg(target_arch = "wasm32")]
async fn resolve_from_service_impl<T: OAuthResolver + ?Sized>(
    resolver: &T,
    input: &Url,
) -> Result<OAuthAuthorizationServerMetadata<'static>, ResolverError> {
    // Assume first that input is a PDS URL (as required by ATPROTO)
    if let Ok(metadata) = resolver.get_resource_server_metadata(input).await {
        return Ok(metadata);
    }
    // Fallback to trying to fetch as an issuer (Entryway)
    resolver.get_authorization_server_metadata(input).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn resolve_from_identity_impl<T: OAuthResolver + Sync + ?Sized>(
    resolver: &T,
    input: &str,
) -> Result<
    (
        OAuthAuthorizationServerMetadata<'static>,
        DidDocument<'static>,
    ),
    ResolverError,
> {
    let actor =
        AtIdentifier::new(input).map_err(|e| ResolverError::AtIdentifier(format!("{:?}", e)))?;
    let identity = resolver.resolve_ident_owned(&actor).await?;
    if let Some(pds) = &identity.pds_endpoint() {
        let metadata = resolver.get_resource_server_metadata(pds).await?;
        Ok((metadata, identity))
    } else {
        Err(ResolverError::DidDocument(format!("Did doc lacking pds")))
    }
}

#[cfg(target_arch = "wasm32")]
async fn resolve_from_identity_impl<T: OAuthResolver + ?Sized>(
    resolver: &T,
    input: &str,
) -> Result<
    (
        OAuthAuthorizationServerMetadata<'static>,
        DidDocument<'static>,
    ),
    ResolverError,
> {
    let actor =
        AtIdentifier::new(input).map_err(|e| ResolverError::AtIdentifier(format!("{:?}", e)))?;
    let identity = resolver.resolve_ident_owned(&actor).await?;
    if let Some(pds) = &identity.pds_endpoint() {
        let metadata = resolver.get_resource_server_metadata(pds).await?;
        Ok((metadata, identity))
    } else {
        Err(ResolverError::DidDocument(format!("Did doc lacking pds")))
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn get_authorization_server_metadata_impl<T: HttpClient + Sync + ?Sized>(
    client: &T,
    issuer: &Url,
) -> Result<OAuthAuthorizationServerMetadata<'static>, ResolverError> {
    let mut md = resolve_authorization_server(client, issuer).await?;
    // Normalize issuer string to the input URL representation to avoid slash quirks
    md.issuer = jacquard_common::CowStr::from(issuer.as_str()).into_static();
    Ok(md)
}

#[cfg(target_arch = "wasm32")]
async fn get_authorization_server_metadata_impl<T: HttpClient + ?Sized>(
    client: &T,
    issuer: &Url,
) -> Result<OAuthAuthorizationServerMetadata<'static>, ResolverError> {
    let mut md = resolve_authorization_server(client, issuer).await?;
    // Normalize issuer string to the input URL representation to avoid slash quirks
    md.issuer = jacquard_common::CowStr::from(issuer.as_str()).into_static();
    Ok(md)
}

#[cfg(not(target_arch = "wasm32"))]
async fn get_resource_server_metadata_impl<T: OAuthResolver + Sync + ?Sized>(
    resolver: &T,
    pds: &Url,
) -> Result<OAuthAuthorizationServerMetadata<'static>, ResolverError> {
    let rs_metadata = resolve_protected_resource_info(resolver, pds).await?;
    // ATPROTO requires one, and only one, authorization server entry
    // > That document MUST contain a single item in the authorization_servers array.
    // https://github.com/bluesky-social/proposals/tree/main/0004-oauth#server-metadata
    let issuer = match &rs_metadata.authorization_servers {
        Some(servers) if !servers.is_empty() => {
            if servers.len() > 1 {
                return Err(ResolverError::ProtectedResourceMetadata(format!(
                    "unable to determine authorization server for PDS: {pds}"
                )));
            }
            &servers[0]
        }
        _ => {
            return Err(ResolverError::ProtectedResourceMetadata(format!(
                "no authorization server found for PDS: {pds}"
            )));
        }
    };
    let as_metadata = resolver.get_authorization_server_metadata(issuer).await?;
    // https://datatracker.ietf.org/doc/html/draft-ietf-oauth-resource-metadata-08#name-authorization-server-metada
    if let Some(protected_resources) = &as_metadata.protected_resources {
        let resource_url = rs_metadata
            .resource
            .strip_suffix('/')
            .unwrap_or(rs_metadata.resource.as_str());
        if !protected_resources.contains(&CowStr::Borrowed(resource_url)) {
            return Err(ResolverError::AuthorizationServerMetadata(format!(
                "pds {pds}, resource {0} not protected by issuer: {issuer}, protected resources: {1:?}",
                rs_metadata.resource, protected_resources
            )));
        }
    }

    // TODO: atproot specific validation?
    // https://github.com/bluesky-social/proposals/tree/main/0004-oauth#server-metadata
    //
    // eg.
    // https://drafts.aaronpk.com/draft-parecki-oauth-client-id-metadata-document/draft-parecki-oauth-client-id-metadata-document.html
    // if as_metadata.client_id_metadata_document_supported != Some(true) {
    //     return Err(Error::AuthorizationServerMetadata(format!(
    //         "authorization server does not support client_id_metadata_document: {issuer}"
    //     )));
    // }

    Ok(as_metadata)
}

#[cfg(target_arch = "wasm32")]
async fn get_resource_server_metadata_impl<T: OAuthResolver + ?Sized>(
    resolver: &T,
    pds: &Url,
) -> Result<OAuthAuthorizationServerMetadata<'static>, ResolverError> {
    let rs_metadata = resolve_protected_resource_info(resolver, pds).await?;
    // ATPROTO requires one, and only one, authorization server entry
    // > That document MUST contain a single item in the authorization_servers array.
    // https://github.com/bluesky-social/proposals/tree/main/0004-oauth#server-metadata
    let issuer = match &rs_metadata.authorization_servers {
        Some(servers) if !servers.is_empty() => {
            if servers.len() > 1 {
                return Err(ResolverError::ProtectedResourceMetadata(format!(
                    "unable to determine authorization server for PDS: {pds}"
                )));
            }
            &servers[0]
        }
        _ => {
            return Err(ResolverError::ProtectedResourceMetadata(format!(
                "no authorization server found for PDS: {pds}"
            )));
        }
    };
    let as_metadata = resolver.get_authorization_server_metadata(issuer).await?;
    // https://datatracker.ietf.org/doc/html/draft-ietf-oauth-resource-metadata-08#name-authorization-server-metada
    if let Some(protected_resources) = &as_metadata.protected_resources {
        let resource_url = rs_metadata
            .resource
            .strip_suffix('/')
            .unwrap_or(rs_metadata.resource.as_str());
        if !protected_resources.contains(&CowStr::Borrowed(resource_url)) {
            return Err(ResolverError::AuthorizationServerMetadata(format!(
                "pds {pds}, resource {0} not protected by issuer: {issuer}, protected resources: {1:?}",
                rs_metadata.resource, protected_resources
            )));
        }
    }

    // TODO: atproot specific validation?
    // https://github.com/bluesky-social/proposals/tree/main/0004-oauth#server-metadata
    //
    // eg.
    // https://drafts.aaronpk.com/draft-parecki-oauth-client-id-metadata-document/draft-parecki-oauth-client-id-metadata-document.html
    // if as_metadata.client_id_metadata_document_supported != Some(true) {
    //     return Err(Error::AuthorizationServerMetadata(format!(
    //         "authorization server does not support client_id_metadata_document: {issuer}"
    //     )));
    // }

    Ok(as_metadata)
}

#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait OAuthResolver: IdentityResolver + HttpClient {
    #[cfg(not(target_arch = "wasm32"))]
    fn verify_issuer(
        &self,
        server_metadata: &OAuthAuthorizationServerMetadata<'_>,
        sub: &Did<'_>,
    ) -> impl Future<Output = Result<Url, ResolverError>> + Send
    where
        Self: Sync,
    {
        verify_issuer_impl(self, server_metadata, sub)
    }

    #[cfg(target_arch = "wasm32")]
    fn verify_issuer(
        &self,
        server_metadata: &OAuthAuthorizationServerMetadata<'_>,
        sub: &Did<'_>,
    ) -> impl Future<Output = Result<Url, ResolverError>> {
        verify_issuer_impl(self, server_metadata, sub)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn resolve_oauth(
        &self,
        input: &str,
    ) -> impl Future<
        Output = Result<
            (
                OAuthAuthorizationServerMetadata<'static>,
                Option<DidDocument<'static>>,
            ),
            ResolverError,
        >,
    > + Send
    where
        Self: Sync,
    {
        resolve_oauth_impl(self, input)
    }

    #[cfg(target_arch = "wasm32")]
    fn resolve_oauth(
        &self,
        input: &str,
    ) -> impl Future<
        Output = Result<
            (
                OAuthAuthorizationServerMetadata<'static>,
                Option<DidDocument<'static>>,
            ),
            ResolverError,
        >,
    > {
        resolve_oauth_impl(self, input)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn resolve_from_service(
        &self,
        input: &Url,
    ) -> impl Future<Output = Result<OAuthAuthorizationServerMetadata<'static>, ResolverError>> + Send
    where
        Self: Sync,
    {
        resolve_from_service_impl(self, input)
    }

    #[cfg(target_arch = "wasm32")]
    fn resolve_from_service(
        &self,
        input: &Url,
    ) -> impl Future<Output = Result<OAuthAuthorizationServerMetadata<'static>, ResolverError>>
    {
        resolve_from_service_impl(self, input)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn resolve_from_identity(
        &self,
        input: &str,
    ) -> impl Future<
        Output = Result<
            (
                OAuthAuthorizationServerMetadata<'static>,
                DidDocument<'static>,
            ),
            ResolverError,
        >,
    > + Send
    where
        Self: Sync,
    {
        resolve_from_identity_impl(self, input)
    }

    #[cfg(target_arch = "wasm32")]
    fn resolve_from_identity(
        &self,
        input: &str,
    ) -> impl Future<
        Output = Result<
            (
                OAuthAuthorizationServerMetadata<'static>,
                DidDocument<'static>,
            ),
            ResolverError,
        >,
    > {
        resolve_from_identity_impl(self, input)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn get_authorization_server_metadata(
        &self,
        issuer: &Url,
    ) -> impl Future<Output = Result<OAuthAuthorizationServerMetadata<'static>, ResolverError>> + Send
    where
        Self: Sync,
    {
        get_authorization_server_metadata_impl(self, issuer)
    }

    #[cfg(target_arch = "wasm32")]
    fn get_authorization_server_metadata(
        &self,
        issuer: &Url,
    ) -> impl Future<Output = Result<OAuthAuthorizationServerMetadata<'static>, ResolverError>>
    {
        get_authorization_server_metadata_impl(self, issuer)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn get_resource_server_metadata(
        &self,
        pds: &Url,
    ) -> impl Future<Output = Result<OAuthAuthorizationServerMetadata<'static>, ResolverError>> + Send
    where
        Self: Sync,
    {
        get_resource_server_metadata_impl(self, pds)
    }

    #[cfg(target_arch = "wasm32")]
    fn get_resource_server_metadata(
        &self,
        pds: &Url,
    ) -> impl Future<Output = Result<OAuthAuthorizationServerMetadata<'static>, ResolverError>>
    {
        get_resource_server_metadata_impl(self, pds)
    }
}

pub async fn resolve_authorization_server<T: HttpClient + ?Sized>(
    client: &T,
    server: &Url,
) -> Result<OAuthAuthorizationServerMetadata<'static>, ResolverError> {
    let url = server
        .join("/.well-known/oauth-authorization-server")
        .map_err(|e| ResolverError::Transport(TransportError::Other(Box::new(e))))?;

    let req = Request::builder()
        .uri(url.to_string())
        .body(Vec::new())
        .map_err(|e| ResolverError::Transport(TransportError::InvalidRequest(e.to_string())))?;
    let res = client
        .send_http(req)
        .await
        .map_err(|e| ResolverError::Transport(TransportError::Other(Box::new(e))))?;
    if res.status() == StatusCode::OK {
        let mut metadata = serde_json::from_slice::<OAuthAuthorizationServerMetadata>(res.body())
            .map_err(ResolverError::SerdeJson)?;
        // https://datatracker.ietf.org/doc/html/rfc8414#section-3.3
        // Accept semantically equivalent issuer (normalize to the requested URL form)
        if issuer_equivalent(&metadata.issuer, server.as_str()) {
            metadata.issuer = server.as_str().into();
            Ok(metadata.into_static())
        } else {
            Err(ResolverError::AuthorizationServerMetadata(format!(
                "invalid issuer: {}",
                metadata.issuer
            )))
        }
    } else {
        Err(ResolverError::HttpStatus(res.status()))
    }
}

pub async fn resolve_protected_resource_info<T: HttpClient + ?Sized>(
    client: &T,
    server: &Url,
) -> Result<OAuthProtectedResourceMetadata<'static>, ResolverError> {
    let url = server
        .join("/.well-known/oauth-protected-resource")
        .map_err(|e| ResolverError::Transport(TransportError::Other(Box::new(e))))?;

    let req = Request::builder()
        .uri(url.to_string())
        .body(Vec::new())
        .map_err(|e| ResolverError::Transport(TransportError::InvalidRequest(e.to_string())))?;
    let res = client
        .send_http(req)
        .await
        .map_err(|e| ResolverError::Transport(TransportError::Other(Box::new(e))))?;
    if res.status() == StatusCode::OK {
        let mut metadata = serde_json::from_slice::<OAuthProtectedResourceMetadata>(res.body())
            .map_err(ResolverError::SerdeJson)?;
        // https://datatracker.ietf.org/doc/html/rfc8414#section-3.3
        // Accept semantically equivalent resource URL (normalize to the requested URL form)
        if issuer_equivalent(&metadata.resource, server.as_str()) {
            metadata.resource = server.as_str().into();
            Ok(metadata.into_static())
        } else {
            Err(ResolverError::AuthorizationServerMetadata(format!(
                "invalid resource: {}",
                metadata.resource
            )))
        }
    } else {
        Err(ResolverError::HttpStatus(res.status()))
    }
}

impl OAuthResolver for jacquard_identity::JacquardResolver {}

#[cfg(test)]
mod tests {
    use core::future::Future;
    use std::{convert::Infallible, sync::Arc};

    use super::*;
    use http::{Request as HttpRequest, Response as HttpResponse, StatusCode};
    use jacquard_common::http_client::HttpClient;
    use tokio::sync::Mutex;

    #[derive(Default, Clone)]
    struct MockHttp {
        next: Arc<Mutex<Option<HttpResponse<Vec<u8>>>>>,
    }

    impl HttpClient for MockHttp {
        type Error = Infallible;
        fn send_http(
            &self,
            _request: HttpRequest<Vec<u8>>,
        ) -> impl Future<Output = core::result::Result<HttpResponse<Vec<u8>>, Self::Error>> + Send
        {
            let next = self.next.clone();
            async move { Ok(next.lock().await.take().unwrap()) }
        }
    }

    #[tokio::test]
    async fn authorization_server_http_status() {
        let client = MockHttp::default();
        *client.next.lock().await = Some(
            HttpResponse::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Vec::new())
                .unwrap(),
        );
        let issuer = url::Url::parse("https://issuer").unwrap();
        let err = super::resolve_authorization_server(&client, &issuer)
            .await
            .unwrap_err();
        matches!(err, ResolverError::HttpStatus(StatusCode::NOT_FOUND));
    }

    #[tokio::test]
    async fn authorization_server_bad_json() {
        let client = MockHttp::default();
        *client.next.lock().await = Some(
            HttpResponse::builder()
                .status(StatusCode::OK)
                .body(b"{not json}".to_vec())
                .unwrap(),
        );
        let issuer = url::Url::parse("https://issuer").unwrap();
        let err = super::resolve_authorization_server(&client, &issuer)
            .await
            .unwrap_err();
        matches!(err, ResolverError::SerdeJson(_));
    }

    #[test]
    fn issuer_equivalence_rules() {
        assert!(super::issuer_equivalent(
            "https://issuer",
            "https://issuer/"
        ));
        assert!(super::issuer_equivalent(
            "https://issuer:443/",
            "https://issuer/"
        ));
        assert!(!super::issuer_equivalent(
            "http://issuer/",
            "https://issuer/"
        ));
        assert!(!super::issuer_equivalent(
            "https://issuer/foo",
            "https://issuer/"
        ));
        assert!(!super::issuer_equivalent(
            "https://issuer/?q=1",
            "https://issuer/"
        ));
    }
}
