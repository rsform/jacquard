use super::LabelerDefs;
use crate::client::{AgentError, AgentSessionExt, CollectionErr, CollectionOutput};
use crate::moderation::labeled::LabeledRecord;

#[cfg(feature = "api_bluesky")]
use jacquard_api::app_bsky::labeler::{
    get_services::{GetServices, GetServicesOutput},
    service::Service,
};
use jacquard_api::com_atproto::label::{Label, query_labels::QueryLabels};
use jacquard_common::cowstr::ToCowStr;
use jacquard_common::error::{ClientError, TransportError};
use jacquard_common::types::collection::Collection;
use jacquard_common::types::string::Did;
use jacquard_common::types::uri::RecordUri;
use jacquard_common::xrpc::{XrpcClient, XrpcError};
use jacquard_common::{CowStr, IntoStatic};
use std::convert::From;

/// Fetch labeler definitions from Bluesky's AppView (or a compatible one)
#[cfg(feature = "api_bluesky")]
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
        XrpcError::Generic(g) => {
            ClientError::Transport(TransportError::Other(g.to_string().into()))
        }
        XrpcError::Decode(e) => ClientError::Decode(e),
        XrpcError::Xrpc(typed) => {
            ClientError::Transport(TransportError::Other(format!("{:?}", typed).into()))
        }
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
/// This is much less efficient for the client than querying the AppView, but has
/// the virtue of working without the Bluesky AppView or a compatible one. Other
/// alternatives include querying <https://ufos.microcosm.blue> for definitions
/// created relatively recently, or doing your own scraping and indexing beforehand.
///
#[cfg(feature = "api_bluesky")]
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
            ClientError::Transport(TransportError::Other(format!("Invalid URI: {}", e).into()))
        })?;

        let output = client.fetch_record(&record_uri).await?;
        let service: Service<'static> = output.value;

        if let Some(label_value_definitions) = service.policies.label_value_definitions {
            defs.insert(did.into_static(), label_value_definitions);
        }
    }

    Ok(defs)
}

/// Convenient wrapper for com.atproto.label.queryLabels
///
/// Avoids depending on the Bluesky namespace, though it may call out to the
/// Bluesky AppView (or a compatible one configured via atproto-proxy header).
///
/// Fetches labels directly for a given set of URI patterns.
/// This one defaults to the max number, assuming that you will be fetching
/// in bulk. This is not especially efficient and mostly exists as a demonstration.
///
/// In practice if you are running an app server, you should call [`subscribeLabels`](https://tangled.org/@nonbinary.computer/jacquard/blob/main/crates/jacquard-api/src/com_atproto/label/subscribe_labels.rs)
/// on labelers to tail their output, and index them alongside the data your app cares about.
pub async fn fetch_labels(
    client: &impl AgentSessionExt,
    uri_patterns: Vec<CowStr<'_>>,
    sources: Vec<Did<'_>>,
    cursor: Option<CowStr<'_>>,
) -> Result<(Vec<Label<'static>>, Option<CowStr<'static>>), AgentError> {
    #[cfg(feature = "tracing")]
    let _span = tracing::debug_span!("fetch_labels", count = sources.len()).entered();

    let request = QueryLabels::new()
        .maybe_cursor(cursor)
        .limit(250)
        .uri_patterns(uri_patterns)
        .sources(sources)
        .build();
    let labels = client
        .send(request)
        .await?
        .into_output()
        .map_err(|e| match e {
            XrpcError::Generic(e) => AgentError::Generic(e),
            _ => unimplemented!(), // We know the error at this point is always GenericXrpcError
        })?;
    Ok((labels.labels, labels.cursor))
}

/// Minimal helper to fetch a URI and any labels.
///
/// This is *extremely* inefficient and should not be used except in experimentation.
/// It primarily exists as a demonstration that you can hydrate labels without
/// using any Bluesky appview methods.
///
/// In practice if you are running an app server, you should call [`subscribeLabels`](https://tangled.org/@nonbinary.computer/jacquard/blob/main/crates/jacquard-api/src/com_atproto/label/subscribe_labels.rs)
/// on labelers to tail their output, and index them alongside the data your app cares about.
pub async fn fetch_labeled_record<R>(
    client: &impl AgentSessionExt,
    record_uri: &RecordUri<'_, R>,
    sources: Vec<Did<'_>>,
) -> Result<LabeledRecord<'static, R>, AgentError>
where
    R: Collection + From<CollectionOutput<'static, R>>,
    for<'a> CollectionOutput<'a, R>: IntoStatic<Output = CollectionOutput<'static, R>>,
    for<'a> CollectionErr<'a, R>: IntoStatic<Output = CollectionErr<'static, R>>,
{
    let record: R = client.fetch_record(record_uri).await?.into();
    let (labels, _) =
        fetch_labels(client, vec![record_uri.as_uri().to_cowstr()], sources, None).await?;

    Ok(LabeledRecord { record, labels })
}
