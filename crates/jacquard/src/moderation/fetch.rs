use super::LabelerDefs;
use crate::client::AgentSessionExt;
use jacquard_api::app_bsky::labeler::get_services::{GetServices, GetServicesOutput};
use jacquard_api::app_bsky::labeler::service::Service;
use jacquard_common::IntoStatic;
use jacquard_common::error::ClientError;
use jacquard_common::types::string::Did;
use jacquard_common::xrpc::{XrpcClient, XrpcError};

/// Fetch labeler definitions from app.bsky.labeler.getServices
///
/// This is a convenience helper for fetching labeler service records from Bluesky's
/// labeler service. You can also fetch these from other indexes or sources and
/// construct a `LabelerDefs` manually.
///
/// # Arguments
///
/// * `client` - Any XRPC client (Agent, stateless client, etc.)
/// * `dids` - List of labeler DIDs to fetch definitions for
///
/// # Example
///
/// ```no_run
/// # use jacquard::moderation::fetch_labeler_defs;
/// # use jacquard::client::BasicClient;
/// # use jacquard_common::types::string::Did;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let client = BasicClient::unauthenticated();
/// let labeler_did = Did::new_static("did:plc:ar7c4by46qjdydhdevvrndac").unwrap();
/// let defs = fetch_labeler_defs(&client, vec![labeler_did]).await?;
/// # Ok(())
/// # }
/// ```
pub async fn fetch_labeler_defs(
    client: &(impl XrpcClient + Sync),
    dids: Vec<Did<'_>>,
) -> Result<LabelerDefs<'static>, ClientError> {
    #[cfg(feature = "tracing")]
    let _span = tracing::debug_span!("fetch_labeler_defs", count = dids.len()).entered();

    let request = GetServices::new().dids(dids).detailed(true).build();

    let response = client.send(request).await?;
    let output: GetServicesOutput<'static> = response.into_output().map_err(|e| match e {
        XrpcError::Auth(auth) => ClientError::Auth(auth),
        XrpcError::Generic(g) => ClientError::Transport(
            jacquard_common::error::TransportError::Other(g.to_string().into()),
        ),
        XrpcError::Decode(e) => ClientError::Decode(e),
        XrpcError::Xrpc(typed) => ClientError::Transport(
            jacquard_common::error::TransportError::Other(format!("{:?}", typed).into()),
        ),
    })?;

    let mut defs = LabelerDefs::new();

    use jacquard_api::app_bsky::labeler::get_services::GetServicesOutputViewsItem;

    for view in output.views {
        match view {
            GetServicesOutputViewsItem::LabelerViewDetailed(detailed) => {
                if let Some(label_value_definitions) = &detailed.policies.label_value_definitions {
                    defs.insert(
                        detailed.creator.did.clone().into_static(),
                        label_value_definitions
                            .iter()
                            .map(|d| d.clone().into_static())
                            .collect(),
                    );
                }
            }
            _ => {
                // Unknown or not sufficiently detailed view type, skip
                continue;
            }
        }
    }

    Ok(defs)
}

/// Fetch labeler definitions directly from each labeler's PDS
///
/// This fetches the `app.bsky.labeler.service` record directly from the PDS where
/// the labeler is hosted.
///
/// # Arguments
///
/// * `client` - Any XRPC client with fetch_record support (Agent, etc.)
/// * `dids` - List of labeler DIDs to fetch definitions for
///
/// # Example
///
/// ```no_run
/// # use jacquard::moderation::fetch_labeler_defs_direct;
/// # use jacquard::client::BasicClient;
/// # use jacquard::prelude::*;
/// # use jacquard_common::types::string::Did;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # let client = BasicClient::unauthenticated();
/// let labeler_did = Did::new_static("did:plc:ar7c4by46qjdydhdevvrndac").unwrap();
/// let defs = fetch_labeler_defs_direct(&client, vec![labeler_did]).await?;
/// # Ok(())
/// # }
/// ```
pub async fn fetch_labeler_defs_direct(
    client: &(impl AgentSessionExt + Sync),
    dids: Vec<Did<'_>>,
) -> Result<LabelerDefs<'static>, ClientError> {
    #[cfg(feature = "tracing")]
    let _span = tracing::debug_span!("fetch_labeler_defs_direct", count = dids.len()).entered();

    let mut defs = LabelerDefs::new();

    for did in dids {
        let uri = format!("at://{}/app.bsky.labeler.service/self", did.as_str());
        let record_uri = Service::uri(uri).map_err(|e| {
            ClientError::Transport(jacquard_common::error::TransportError::Other(
                format!("Invalid URI: {}", e).into(),
            ))
        })?;

        let output = client.fetch_record(&record_uri).await?;
        let service: Service<'static> = output.value;

        if let Some(label_value_definitions) = service.policies.label_value_definitions {
            defs.insert(did.into_static(), label_value_definitions);
        }
    }

    Ok(defs)
}
