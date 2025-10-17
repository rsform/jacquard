//! WebSocket subscription support for XRPC
//!
//! This module defines traits and types for typed WebSocket subscriptions,
//! mirroring the request/response pattern used for HTTP XRPC endpoints.

use serde::{Deserialize, Serialize};
use std::error::Error;

use crate::IntoStatic;

/// Encoding format for subscription messages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageEncoding {
    /// JSON text frames
    Json,
    /// DAG-CBOR binary frames
    DagCbor,
}

/// XRPC subscription stream response trait
///
/// Analogous to `XrpcResp` but for WebSocket subscriptions.
/// Defines the message and error types for a subscription stream.
///
/// This trait is implemented on a marker struct to keep it lifetime-free
/// while using GATs for the message/error types.
pub trait SubscriptionResp {
    /// The NSID for this subscription
    const NSID: &'static str;

    /// Message encoding (JSON or DAG-CBOR)
    const ENCODING: MessageEncoding;

    /// Message union type
    type Message<'de>: Deserialize<'de> + IntoStatic;

    /// Error union type
    type Error<'de>: Error + Deserialize<'de> + IntoStatic;
}

/// XRPC subscription (WebSocket)
///
/// This trait is analogous to `XrpcRequest` but for WebSocket subscriptions.
/// It defines the NSID and associated stream response type.
///
/// The trait is implemented on the subscription parameters type.
pub trait XrpcSubscription: Serialize {
    /// The NSID for this XRPC subscription
    const NSID: &'static str;

    /// Message encoding (JSON or DAG-CBOR)
    const ENCODING: MessageEncoding;

    /// Stream response type (marker struct)
    type Stream: SubscriptionResp;

    /// Encode query params for WebSocket URL
    ///
    /// Default implementation uses serde_html_form to encode the struct as query parameters.
    fn query_params(&self) -> Vec<(String, String)> {
        // Default: use serde_html_form to encode self
        serde_html_form::to_string(self)
            .ok()
            .map(|s| {
                s.split('&')
                    .filter_map(|pair| {
                        let mut parts = pair.splitn(2, '=');
                        Some((parts.next()?.to_string(), parts.next()?.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// XRPC subscription endpoint trait (server-side)
///
/// Analogous to `XrpcEndpoint` but for WebSocket subscriptions.
/// Defines the fully-qualified path and associated parameter/stream types.
///
/// This exists primarily for server-side frameworks (like Axum) to extract
/// typed subscription parameters without lifetime issues.
pub trait SubscriptionEndpoint {
    /// Fully-qualified path ('/xrpc/[nsid]') where this subscription endpoint lives
    const PATH: &'static str;

    /// Message encoding (JSON or DAG-CBOR)
    const ENCODING: MessageEncoding;

    /// Subscription parameters type
    type Params<'de>: XrpcSubscription + Deserialize<'de> + IntoStatic;

    /// Stream response type
    type Stream: SubscriptionResp;
}
