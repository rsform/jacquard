use crate::types::{OAuthAuthorizationServerMetadata, OAuthProtectedResourceMetadata};
use http::{Request, StatusCode};
use jacquard_common::IntoStatic;
use jacquard_common::ident_resolver::{IdentityError, IdentityResolver};
use jacquard_common::types::did_doc::DidDocument;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::{http_client::HttpClient, types::did::Did};
use sha2::digest::const_oid::Arc;
use url::Url;

#[derive(thiserror::Error, Debug, miette::Diagnostic)]
pub enum ResolverError {
    #[error("resource not found")]
    NotFound,
    #[error("invalid at identifier: {0}")]
    AtIdentifier(String),
    #[error("invalid did: {0}")]
    Did(String),
    #[error("invalid did document: {0}")]
    DidDocument(String),
    #[error("protected resource metadata is invalid: {0}")]
    ProtectedResourceMetadata(String),
    #[error("authorization server metadata is invalid: {0}")]
    AuthorizationServerMetadata(String),
    #[error("error resolving identity: {0}")]
    IdentityResolverError(#[from] IdentityError),
    #[error("unsupported did method: {0:?}")]
    UnsupportedDidMethod(Did<'static>),
    #[error(transparent)]
    Http(#[from] http::Error),
    #[error("http client error: {0}")]
    HttpClient(Box<dyn std::error::Error + Send + Sync + 'static>),
    #[error("http status: {0:?}")]
    HttpStatus(StatusCode),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error(transparent)]
    SerdeHtmlForm(#[from] serde_html_form::ser::Error),
    #[error(transparent)]
    Uri(#[from] url::ParseError),
}

#[async_trait::async_trait]
pub trait OAuthResolver: IdentityResolver + HttpClient {
    async fn verify_issuer(
        &self,
        server_metadata: &OAuthAuthorizationServerMetadata<'_>,
        sub: &Did<'_>,
    ) -> Result<Url, ResolverError> {
        let (metadata, identity) = self.resolve_from_identity(sub).await?;
        if metadata.issuer != server_metadata.issuer {
            return Err(ResolverError::Did(format!("DIDs did not match")));
        }
        Ok(identity
            .pds_endpoint()
            .ok_or(ResolverError::DidDocument(format!("{:?}", identity).into()))?)
    }
    async fn resolve_oauth(
        &self,
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
            (self.resolve_from_service(&url).await?, None)
        } else {
            let (metadata, identity) = self.resolve_from_identity(input).await?;
            (metadata, Some(identity))
        })
    }
    async fn resolve_from_service(
        &self,
        input: &Url,
    ) -> Result<OAuthAuthorizationServerMetadata<'static>, ResolverError> {
        // Assume first that input is a PDS URL (as required by ATPROTO)
        if let Ok(metadata) = self.get_resource_server_metadata(input).await {
            return Ok(metadata);
        }
        // Fallback to trying to fetch as an issuer (Entryway)
        self.get_authorization_server_metadata(input).await
    }
    async fn resolve_from_identity(
        &self,
        input: &str,
    ) -> Result<
        (
            OAuthAuthorizationServerMetadata<'static>,
            DidDocument<'static>,
        ),
        ResolverError,
    > {
        let actor = AtIdentifier::new(input)
            .map_err(|e| ResolverError::AtIdentifier(format!("{:?}", e)))?;
        let identity = self.resolve_ident_owned(&actor).await?;
        if let Some(pds) = &identity.pds_endpoint() {
            let metadata = self.get_resource_server_metadata(pds).await?;
            Ok((metadata, identity))
        } else {
            Err(ResolverError::DidDocument(format!("Did doc lacking pds")))
        }
    }
    async fn get_authorization_server_metadata(
        &self,
        issuer: &Url,
    ) -> Result<OAuthAuthorizationServerMetadata<'static>, ResolverError> {
        Ok(resolve_authorization_server(self, issuer).await?)
    }
    async fn get_resource_server_metadata(
        &self,
        pds: &Url,
    ) -> Result<OAuthAuthorizationServerMetadata<'static>, ResolverError> {
        let rs_metadata = resolve_protected_resource_info(self, pds).await?;
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
        let as_metadata = self.get_authorization_server_metadata(issuer).await?;
        // https://datatracker.ietf.org/doc/html/draft-ietf-oauth-resource-metadata-08#name-authorization-server-metada
        if let Some(protected_resources) = &as_metadata.protected_resources {
            if !protected_resources.contains(&rs_metadata.resource) {
                return Err(ResolverError::AuthorizationServerMetadata(format!(
                    "pds {pds} does not protected by issuer: {issuer}",
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
}

#[async_trait::async_trait]
impl<T: OAuthResolver + Sync + Send> OAuthResolver for std::sync::Arc<T> {}

pub async fn resolve_authorization_server<T: HttpClient + ?Sized>(
    client: &T,
    server: &Url,
) -> Result<OAuthAuthorizationServerMetadata<'static>, ResolverError> {
    let url = server
        .join("/.well-known/oauth-authorization-server")
        .map_err(|e| ResolverError::HttpClient(e.into()))?;

    let req = Request::builder()
        .uri(url.to_string())
        .body(Vec::new())
        .map_err(|e| ResolverError::HttpClient(e.into()))?;
    let res = client
        .send_http(req)
        .await
        .map_err(|e| ResolverError::HttpClient(e.into()))?;
    if res.status() == StatusCode::OK {
        let metadata = serde_json::from_slice::<OAuthAuthorizationServerMetadata>(res.body())
            .map_err(ResolverError::SerdeJson)?;
        // https://datatracker.ietf.org/doc/html/rfc8414#section-3.3
        if metadata.issuer == server.as_str() {
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
        .map_err(|e| ResolverError::HttpClient(e.into()))?;

    let req = Request::builder()
        .uri(url.to_string())
        .body(Vec::new())
        .map_err(|e| ResolverError::HttpClient(e.into()))?;
    let res = client
        .send_http(req)
        .await
        .map_err(|e| ResolverError::HttpClient(e.into()))?;
    if res.status() == StatusCode::OK {
        let metadata = serde_json::from_slice::<OAuthProtectedResourceMetadata>(res.body())
            .map_err(ResolverError::SerdeJson)?;
        // https://datatracker.ietf.org/doc/html/rfc8414#section-3.3
        if metadata.resource == server.as_str() {
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
