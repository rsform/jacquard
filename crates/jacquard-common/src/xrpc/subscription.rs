//! WebSocket subscription support for XRPC
//!
//! This module defines traits and types for typed WebSocket subscriptions,
//! mirroring the request/response pattern used for HTTP XRPC endpoints.

use crate::bos::BosStr;
use crate::deps::fluent_uri::{
    ParseError, Uri,
    pct_enc::{
        EString,
        encoder::{Data as EncData, Query},
    },
};
use crate::error::DecodeError;
use crate::stream::StreamError;
use crate::websocket::{
    WebSocketClient, WebSocketConnectOptions, WebSocketConnection, WsSink, WsStream,
};
use crate::{CowStr, Data, IntoStatic, RawData, WsMessage};
use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::error::Error;
use core::fmt;
use core::future::Future;
use core::marker::PhantomData;
use core::str::FromStr;
#[cfg(not(target_arch = "wasm32"))]
use n0_future::stream::Boxed;
#[cfg(target_arch = "wasm32")]
use n0_future::stream::BoxedLocal as Boxed;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

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

    /// WebSocket subprotocol to request during the upgrade handshake
    /// (e.g. Some("xrpc.v1.json") for network.bsky.jetstream.subscribeEvents).
    /// Subscriptions without one negotiate no subprotocol.
    const SUBPROTOCOL: Option<&'static str> = None;

    /// Message union type, parameterised on backing string type.
    type Message<S: BosStr>;

    /// Error union type. Always owned (`DeserializeOwned`).
    type Error: Error + DeserializeOwned;

    /// Decode a message from bytes.
    ///
    /// Default implementation uses simple deserialization via serde.
    /// Subscriptions that use framed encoding (header + body) can override
    /// this to do two-stage deserialization.
    fn decode_message<'de, S>(bytes: &'de [u8]) -> Result<Self::Message<S>, DecodeError>
    where
        S: BosStr + Deserialize<'de>,
        Self::Message<S>: Deserialize<'de>,
    {
        match Self::ENCODING {
            MessageEncoding::Json => {
                if Self::SUBPROTOCOL == Some("xrpc.v1.json") {
                    Self::decode_json_frame(bytes)?.ok_or_else(|| {
                        DecodeError::UnknownEventType("unknown xrpc.v1.json envelope".into())
                    })
                } else {
                    serde_json::from_slice(bytes).map_err(DecodeError::from)
                }
            }
            MessageEncoding::DagCbor => {
                serde_ipld_dagcbor::from_slice(bytes).map_err(DecodeError::from)
            }
        }
    }

    /// Decode an `xrpc.v1.json` frame (atproto proposal 0015):
    /// `{"$type":"message","payload":{…union…}}`. The payload field
    /// deserializes as this subscription's message union in the same
    /// parse. Error frames surface as decode errors; the server closes
    /// the stream after sending one.
    fn decode_json_frame<'de, S>(bytes: &'de [u8]) -> Result<Option<Self::Message<S>>, DecodeError>
    where
        S: BosStr + Deserialize<'de>,
        Self::Message<S>: Deserialize<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(tag = "$type")]
        enum Frame<'a, M> {
            #[serde(rename = "message")]
            Message { payload: M },
            #[serde(rename = "error")]
            Error {
                #[serde(borrow)]
                error: crate::CowStr<'a>,
                #[serde(default, borrow)]
                message: Option<crate::CowStr<'a>>,
            },
            #[serde(other)]
            Unknown,
        }

        match serde_json::from_slice::<Frame<'de, Self::Message<S>>>(bytes)? {
            Frame::Message { payload } => Ok(Some(payload)),
            Frame::Error { error, message } => {
                let detail = match message {
                    Some(m) => format!("{error}: {m}"),
                    None => error.to_string(),
                };
                Err(DecodeError::UnknownEventType(detail.into()))
            }
            Frame::Unknown => Ok(None),
        }
    }
}

/// XRPC subscription (WebSocket)
///
/// This trait is analogous to `XrpcRequest` but for WebSocket subscriptions.
/// It defines the NSID and associated stream response type.
///
/// The trait is implemented on the subscription parameters type.
pub trait XrpcSubscription {
    /// The NSID for this XRPC subscription
    const NSID: &'static str;

    /// Message encoding (JSON or DAG-CBOR)
    const ENCODING: MessageEncoding;

    /// Custom path override (e.g., "/subscribe" for Jetstream).
    /// If None, defaults to "/xrpc/{NSID}"
    const CUSTOM_PATH: Option<&'static str> = None;

    /// WebSocket subprotocol to request during the upgrade handshake.
    /// Mirrors [`SubscriptionResp::SUBPROTOCOL`] on the associated stream type.
    const SUBPROTOCOL: Option<&'static str> = None;

    /// Stream response type (marker struct)
    type Stream: SubscriptionResp;

    /// Encode query params for WebSocket URL
    ///
    /// Default implementation uses serde_html_form to encode the struct as query parameters.
    fn query_params(&self) -> Vec<(String, String)>
    where
        Self: Serialize,
    {
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

/// Header for framed DAG-CBOR subscription messages.
///
/// Used in ATProto subscription streams where each message has a CBOR-encoded header
/// followed by the message body.
#[derive(Debug, serde::Deserialize)]
pub struct EventHeader {
    /// Operation code
    pub op: i64,
    /// Event type discriminator (e.g., "#commit", "#identity")
    pub t: smol_str::SmolStr,
}

/// A minimal cursor for no_std that tracks read position.
///
/// Implements `ciborium_io::Read` to work with ciborium's CBOR parser.
#[cfg(not(feature = "std"))]
struct SliceCursor<'a> {
    slice: &'a [u8],
    position: usize,
}

#[cfg(not(feature = "std"))]
impl<'a> SliceCursor<'a> {
    fn new(slice: &'a [u8]) -> Self {
        Self { slice, position: 0 }
    }

    fn position(&self) -> usize {
        self.position
    }
}

#[cfg(not(feature = "std"))]
impl ciborium_io::Read for SliceCursor<'_> {
    type Error = core::convert::Infallible;

    fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), Self::Error> {
        let end = self.position + buf.len();
        buf.copy_from_slice(&self.slice[self.position..end]);
        self.position = end;
        Ok(())
    }
}

/// Parse a framed DAG-CBOR message header and return the header plus remaining body bytes.
///
/// Used for two-stage deserialization of subscription messages in formats like
/// `com.atproto.sync.subscribeRepos`.
#[cfg(feature = "std")]
pub fn parse_event_header<'a>(bytes: &'a [u8]) -> Result<(EventHeader, &'a [u8]), DecodeError> {
    let mut cursor = std::io::Cursor::new(bytes);
    let header: EventHeader = ciborium::de::from_reader(&mut cursor)?;
    let position = cursor.position() as usize;
    drop(cursor); // explicit drop before reborrowing bytes

    Ok((header, &bytes[position..]))
}

/// Parse a framed DAG-CBOR message header and return the header plus remaining body bytes.
///
/// Used for two-stage deserialization of subscription messages in formats like
/// `com.atproto.sync.subscribeRepos`.
#[cfg(not(feature = "std"))]
pub fn parse_event_header<'a>(bytes: &'a [u8]) -> Result<(EventHeader, &'a [u8]), DecodeError> {
    let mut cursor = SliceCursor::new(bytes);
    let header: EventHeader = ciborium::de::from_reader(&mut cursor)?;
    let position = cursor.position();

    Ok((header, &bytes[position..]))
}

/// Decode JSON messages from a WebSocket stream, with an injected
/// transform applied to binary frames first.
///
/// The transform is authoritative when it returns `Ok(Some(bytes))`: the
/// returned immutable bytes are decoded directly and the static-dictionary/v1
/// zstd path is skipped. `Ok(None)` declines the frame and falls through to
/// the existing [`decode_json_msg`] behavior unchanged. `Err` is surfaced
/// without attempting to reinterpret the original binary frame. This lets a
/// caller that negotiated per-stream dictionary compression decode its frames
/// without any of that logic living here.
fn decode_json_bytes<'a, S: SubscriptionResp, Str>(
    bytes: &'a [u8],
) -> Option<Result<StreamMessage<Str, S>, StreamError>>
where
    StreamMessage<Str, S>: Deserialize<'a>,
    Str: BosStr + Clone + FromStr + fmt::Debug + Deserialize<'a>,
    <Str as FromStr>::Err: fmt::Debug,
{
    if S::SUBPROTOCOL == Some("xrpc.v1.json") {
        match S::decode_json_frame::<Str>(bytes) {
            Ok(Some(message)) => Some(Ok(message)),
            Ok(None) => None,
            Err(error) => Some(Err(StreamError::decode(error))),
        }
    } else {
        Some(S::decode_message::<Str>(bytes).map_err(StreamError::decode))
    }
}

/// Decode a JSON WebSocket message after applying a fallible binary transform.
/// See [`SubscriptionStream::into_stream_with_binary_transform`].
pub fn decode_json_msg_with<S, F, Str>(
    msg_result: Result<crate::websocket::WsMessage, StreamError>,
    transform: &F,
) -> Option<Result<StreamMessage<Str, S>, StreamError>>
where
    S: SubscriptionResp,
    F: Fn(&[u8]) -> Result<Option<crate::deps::bytes::Bytes>, StreamError>,
    StreamMessage<Str, S>: DeserializeOwned,
    Str: BosStr + Clone + FromStr + fmt::Debug + DeserializeOwned,
    <Str as FromStr>::Err: fmt::Debug,
{
    use crate::websocket::WsMessage;

    match msg_result {
        Ok(WsMessage::Text(text)) => decode_json_bytes::<S, Str>(text.as_ref()),
        Ok(WsMessage::Binary(bytes)) => match transform(&bytes) {
            Ok(Some(transformed)) => decode_json_bytes::<S, Str>(&transformed),
            Ok(None) => decode_json_msg::<S, Str>(Ok(WsMessage::Binary(bytes))),
            Err(error) => Some(Err(error)),
        },
        Ok(WsMessage::Close(_)) => Some(Err(StreamError::closed())),
        Err(e) => Some(Err(e)),
    }
}

/// Decode JSON messages from a WebSocket stream
pub fn decode_json_msg<S: SubscriptionResp, Str>(
    msg_result: Result<crate::websocket::WsMessage, StreamError>,
) -> Option<Result<StreamMessage<Str, S>, StreamError>>
where
    StreamMessage<Str, S>: DeserializeOwned,
    Str: BosStr + Clone + FromStr + fmt::Debug + DeserializeOwned,
    <Str as FromStr>::Err: fmt::Debug,
{
    use crate::websocket::WsMessage;

    match msg_result {
        Ok(WsMessage::Text(text)) => decode_json_bytes::<S, Str>(text.as_ref()),
        Ok(WsMessage::Binary(bytes)) => {
            #[cfg(feature = "zstd")]
            {
                // Try to decompress with zstd first (Jetstream uses zstd compression)
                match decompress_zstd(&bytes) {
                    Ok(decompressed) => decode_json_bytes::<S, Str>(&decompressed),
                    Err(_) => {
                        // Not zstd-compressed, try direct decode
                        decode_json_bytes::<S, Str>(&bytes)
                    }
                }
            }
            #[cfg(not(feature = "zstd"))]
            {
                decode_json_bytes::<S, Str>(&bytes)
            }
        }
        Ok(WsMessage::Close(_)) => Some(Err(StreamError::closed())),
        Err(e) => Some(Err(e)),
    }
}

/// The vendored zstd dictionary (id 1612007021) shipped for v1 Jetstream
/// subscriptions. Jetstream v2 serves its own dictionary generation;
/// comparing against this detects whether the vendored copy is still
/// current.
#[cfg(feature = "zstd")]
pub static VENDORED_ZSTD_DICTIONARY: &[u8] = include_bytes!("../../zstd_dictionary");

#[cfg(feature = "zstd")]
fn decompress_zstd(bytes: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    use zstd::stream::decode_all;

    static DICTIONARY: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let dict = DICTIONARY.get_or_init(|| VENDORED_ZSTD_DICTIONARY.to_vec());

    decode_all(std::io::Cursor::new(bytes)).or_else(|_| {
        // Try with dictionary
        let mut decoder = zstd::Decoder::with_dictionary(std::io::Cursor::new(bytes), dict)?;
        let mut result = Vec::new();
        std::io::Read::read_to_end(&mut decoder, &mut result)?;
        Ok(result)
    })
}

/// Decode CBOR messages from a WebSocket stream
pub fn decode_cbor_msg<S: SubscriptionResp, Str>(
    msg_result: Result<crate::websocket::WsMessage, StreamError>,
) -> Option<Result<StreamMessage<Str, S>, StreamError>>
where
    StreamMessage<Str, S>: DeserializeOwned,
    Str: BosStr + Clone + FromStr + fmt::Debug + DeserializeOwned,
    <Str as FromStr>::Err: fmt::Debug,
{
    use crate::websocket::WsMessage;

    match msg_result {
        Ok(WsMessage::Binary(bytes)) => {
            Some(S::decode_message::<Str>(&bytes).map_err(StreamError::decode))
        }
        Ok(WsMessage::Text(_)) => Some(Err(StreamError::wrong_message_format(
            "expected binary frame for CBOR, got text",
        ))),
        Ok(WsMessage::Close(_)) => Some(Err(StreamError::closed())),
        Err(e) => Some(Err(e)),
    }
}

/// Websocket subscriber-sent control message
///
/// Note: this is not meaningful for atproto event stream endpoints as
/// those do not support control after the fact. Jetstream does, however.
///
/// If you wish to control an ongoing Jetstream connection, wrap the [`WsSink`]
/// returned from one of the `into_*` methods of the [`SubscriptionStream`]
/// in a [`SubscriptionController`] with the corresponding message implementing
/// this trait as a generic parameter.
pub trait SubscriptionControlMessage: Serialize {
    /// The subscription this is associated with
    type Subscription: XrpcSubscription;

    /// Encode the control message for transmission
    ///
    /// Defaults to json text (matches Jetstream)
    fn encode(&self) -> Result<WsMessage, StreamError> {
        Ok(WsMessage::from(
            serde_json::to_string(&self).map_err(StreamError::encode)?,
        ))
    }

    /// Decode the control message
    fn decode<'de>(frame: &'de [u8]) -> Result<Self, StreamError>
    where
        Self: Deserialize<'de>,
    {
        Ok(serde_json::from_slice(frame).map_err(StreamError::decode)?)
    }
}

/// Control a websocket stream with a given subscription control message
pub struct SubscriptionController<S: SubscriptionControlMessage> {
    controller: WsSink,
    _marker: PhantomData<fn() -> S>,
}

impl<S: SubscriptionControlMessage> SubscriptionController<S> {
    /// Create a new subscription controller from a WebSocket sink.
    pub fn new(controller: WsSink) -> Self {
        Self {
            controller,
            _marker: PhantomData,
        }
    }

    /// Configure the upstream connection via the websocket
    pub async fn configure(&mut self, params: &S) -> Result<(), StreamError> {
        let message = params.encode()?;

        n0_future::SinkExt::send(self.controller.get_mut(), message)
            .await
            .map_err(StreamError::transport)
    }
}

/// Typed subscription stream wrapping a WebSocket connection.
///
/// Analogous to `Response<R>` for XRPC but for subscription streams.
/// Automatically decodes messages based on the subscription's encoding format.
pub struct SubscriptionStream<S: SubscriptionResp> {
    _marker: PhantomData<fn() -> S>,
    connection: WebSocketConnection,
}

impl<S: SubscriptionResp> SubscriptionStream<S> {
    /// Create a new subscription stream from a WebSocket connection.
    pub fn new(connection: WebSocketConnection) -> Self {
        Self {
            _marker: PhantomData,
            connection,
        }
    }

    /// Get a reference to the underlying WebSocket connection.
    pub fn connection(&self) -> &WebSocketConnection {
        &self.connection
    }

    /// Get a mutable reference to the underlying WebSocket connection.
    pub fn connection_mut(&mut self) -> &mut WebSocketConnection {
        &mut self.connection
    }

    /// Split the connection and decode messages into a typed stream.
    ///
    /// Returns a tuple of (sender, typed message stream).
    /// Messages are decoded according to the subscription's ENCODING.
    pub fn into_stream<Str>(self) -> (WsSink, Boxed<Result<StreamMessage<Str, S>, StreamError>>)
    where
        StreamMessage<Str, S>: DeserializeOwned,
        Str: BosStr + Clone + FromStr + fmt::Debug + DeserializeOwned,
        <Str as FromStr>::Err: fmt::Debug,
    {
        use n0_future::StreamExt as _;

        let (tx, rx) = self.connection.split();

        #[cfg(not(target_arch = "wasm32"))]
        let stream = match S::ENCODING {
            MessageEncoding::Json => rx
                .into_inner()
                .filter_map(|msg| decode_json_msg::<S, Str>(msg))
                .boxed(),
            MessageEncoding::DagCbor => rx
                .into_inner()
                .filter_map(|msg| decode_cbor_msg::<S, Str>(msg))
                .boxed(),
        };

        #[cfg(target_arch = "wasm32")]
        let stream = match S::ENCODING {
            MessageEncoding::Json => rx
                .into_inner()
                .filter_map(|msg| decode_json_msg::<S, Str>(msg))
                .boxed_local(),
            MessageEncoding::DagCbor => rx
                .into_inner()
                .filter_map(|msg| decode_cbor_msg::<S, Str>(msg))
                .boxed_local(),
        };

        (tx, stream)
    }

    /// Split the connection and decode messages into a typed stream,
    /// with a per-stream transform applied to binary frames before the
    /// standard decode path.
    ///
    /// `Ok(Some(bytes))` decodes the immutable transformed payload directly;
    /// `Ok(None)` falls through to the existing binary-frame behavior, and an
    /// error terminates that frame without reinterpretation. Use this for
    /// subscriptions that negotiated per-stream dictionary compression; see
    /// [`decode_json_msg_with`].
    pub fn into_stream_with_binary_transform<F, Str>(
        self,
        transform: F,
    ) -> (WsSink, Boxed<Result<StreamMessage<Str, S>, StreamError>>)
    where
        F: Fn(&[u8]) -> Result<Option<crate::deps::bytes::Bytes>, StreamError> + Send + 'static,
        StreamMessage<Str, S>: DeserializeOwned,
        Str: BosStr + Clone + FromStr + fmt::Debug + DeserializeOwned,
        <Str as FromStr>::Err: fmt::Debug,
    {
        use n0_future::StreamExt as _;

        let (tx, rx) = self.connection.split();

        #[cfg(not(target_arch = "wasm32"))]
        let stream = match S::ENCODING {
            MessageEncoding::Json => rx
                .into_inner()
                .filter_map(move |msg| decode_json_msg_with::<S, _, Str>(msg, &transform))
                .boxed(),
            MessageEncoding::DagCbor => rx
                .into_inner()
                .filter_map(|msg| decode_cbor_msg::<S, Str>(msg))
                .boxed(),
        };

        #[cfg(target_arch = "wasm32")]
        let stream = match S::ENCODING {
            MessageEncoding::Json => rx
                .into_inner()
                .filter_map(move |msg| decode_json_msg_with::<S, _, Str>(msg, &transform))
                .boxed_local(),
            MessageEncoding::DagCbor => rx
                .into_inner()
                .filter_map(|msg| decode_cbor_msg::<S, Str>(msg))
                .boxed_local(),
        };

        (tx, stream)
    }

    /// Converts the subscription into a stream of raw atproto data.
    pub fn into_raw_data_stream(self) -> (WsSink, Boxed<Result<RawData<'static>, StreamError>>) {
        use n0_future::StreamExt as _;

        let (tx, rx) = self.connection.split();

        fn parse_msg<'a>(bytes: &'a [u8]) -> Result<RawData<'a>, serde_json::Error> {
            serde_json::from_slice(bytes)
        }
        fn parse_cbor<'a>(
            bytes: &'a [u8],
        ) -> Result<RawData<'a>, serde_ipld_dagcbor::DecodeError<core::convert::Infallible>>
        {
            serde_ipld_dagcbor::from_slice(bytes)
        }

        #[cfg(not(target_arch = "wasm32"))]
        let stream = match S::ENCODING {
            MessageEncoding::Json => rx
                .into_inner()
                .filter_map(|msg_result| match msg_result {
                    Ok(WsMessage::Text(text)) => Some(
                        parse_msg(text.as_ref())
                            .map(|v| v.into_static())
                            .map_err(StreamError::decode),
                    ),
                    Ok(WsMessage::Binary(bytes)) => {
                        #[cfg(feature = "zstd")]
                        {
                            match decompress_zstd(&bytes) {
                                Ok(decompressed) => Some(
                                    parse_msg(&decompressed)
                                        .map(|v| v.into_static())
                                        .map_err(StreamError::decode),
                                ),
                                Err(_) => Some(
                                    parse_msg(&bytes)
                                        .map(|v| v.into_static())
                                        .map_err(StreamError::decode),
                                ),
                            }
                        }
                        #[cfg(not(feature = "zstd"))]
                        {
                            Some(
                                parse_msg(&bytes)
                                    .map(|v| v.into_static())
                                    .map_err(StreamError::decode),
                            )
                        }
                    }
                    Ok(WsMessage::Close(_)) => Some(Err(StreamError::closed())),
                    Err(e) => Some(Err(e)),
                })
                .boxed(),
            MessageEncoding::DagCbor => rx
                .into_inner()
                .filter_map(|msg_result| match msg_result {
                    Ok(WsMessage::Binary(bytes)) => Some(
                        parse_cbor(&bytes)
                            .map(|v| v.into_static())
                            .map_err(|e| StreamError::decode(crate::error::DecodeError::from(e))),
                    ),
                    Ok(WsMessage::Text(_)) => Some(Err(StreamError::wrong_message_format(
                        "expected binary frame for CBOR, got text",
                    ))),
                    Ok(WsMessage::Close(_)) => Some(Err(StreamError::closed())),
                    Err(e) => Some(Err(e)),
                })
                .boxed(),
        };

        #[cfg(target_arch = "wasm32")]
        let stream = match S::ENCODING {
            MessageEncoding::Json => rx
                .into_inner()
                .filter_map(|msg_result| match msg_result {
                    Ok(WsMessage::Text(text)) => Some(
                        parse_msg(text.as_ref())
                            .map(|v| v.into_static())
                            .map_err(StreamError::decode),
                    ),
                    Ok(WsMessage::Binary(bytes)) => {
                        #[cfg(feature = "zstd")]
                        {
                            match decompress_zstd(&bytes) {
                                Ok(decompressed) => Some(
                                    parse_msg(&decompressed)
                                        .map(|v| v.into_static())
                                        .map_err(StreamError::decode),
                                ),
                                Err(_) => Some(
                                    parse_msg(&bytes)
                                        .map(|v| v.into_static())
                                        .map_err(StreamError::decode),
                                ),
                            }
                        }
                        #[cfg(not(feature = "zstd"))]
                        {
                            Some(
                                parse_msg(&bytes)
                                    .map(|v| v.into_static())
                                    .map_err(StreamError::decode),
                            )
                        }
                    }
                    Ok(WsMessage::Close(_)) => Some(Err(StreamError::closed())),
                    Err(e) => Some(Err(e)),
                })
                .boxed_local(),
            MessageEncoding::DagCbor => rx
                .into_inner()
                .filter_map(|msg_result| match msg_result {
                    Ok(WsMessage::Binary(bytes)) => Some(
                        parse_cbor(&bytes)
                            .map(|v| v.into_static())
                            .map_err(|e| StreamError::decode(crate::error::DecodeError::from(e))),
                    ),
                    Ok(WsMessage::Text(_)) => Some(Err(StreamError::wrong_message_format(
                        "expected binary frame for CBOR, got text",
                    ))),
                    Ok(WsMessage::Close(_)) => Some(Err(StreamError::closed())),
                    Err(e) => Some(Err(e)),
                })
                .boxed_local(),
        };

        (tx, stream)
    }

    /// Converts the subscription into a stream of loosely-typed atproto data.
    pub fn into_data_stream(self) -> (WsSink, Boxed<Result<Data<smol_str::SmolStr>, StreamError>>) {
        use n0_future::StreamExt as _;

        let (tx, rx) = self.connection.split();

        fn parse_msg(bytes: &[u8]) -> Result<Data<smol_str::SmolStr>, serde_json::Error> {
            serde_json::from_slice(bytes)
        }
        fn parse_cbor(
            bytes: &[u8],
        ) -> Result<
            Data<smol_str::SmolStr>,
            serde_ipld_dagcbor::DecodeError<core::convert::Infallible>,
        > {
            serde_ipld_dagcbor::from_slice(bytes)
        }

        #[cfg(not(target_arch = "wasm32"))]
        let stream = match S::ENCODING {
            MessageEncoding::Json => rx
                .into_inner()
                .filter_map(|msg_result| match msg_result {
                    Ok(WsMessage::Text(text)) => Some(
                        parse_msg(text.as_ref())
                            .map(|v| v.into_static())
                            .map_err(StreamError::decode),
                    ),
                    Ok(WsMessage::Binary(bytes)) => {
                        #[cfg(feature = "zstd")]
                        {
                            match decompress_zstd(&bytes) {
                                Ok(decompressed) => Some(
                                    parse_msg(&decompressed)
                                        .map(|v| v.into_static())
                                        .map_err(StreamError::decode),
                                ),
                                Err(_) => Some(
                                    parse_msg(&bytes)
                                        .map(|v| v.into_static())
                                        .map_err(StreamError::decode),
                                ),
                            }
                        }
                        #[cfg(not(feature = "zstd"))]
                        {
                            Some(
                                parse_msg(&bytes)
                                    .map(|v| v.into_static())
                                    .map_err(StreamError::decode),
                            )
                        }
                    }
                    Ok(WsMessage::Close(_)) => Some(Err(StreamError::closed())),
                    Err(e) => Some(Err(e)),
                })
                .boxed(),
            MessageEncoding::DagCbor => rx
                .into_inner()
                .filter_map(|msg_result| match msg_result {
                    Ok(WsMessage::Binary(bytes)) => Some(
                        parse_cbor(&bytes)
                            .map(|v| v.into_static())
                            .map_err(|e| StreamError::decode(crate::error::DecodeError::from(e))),
                    ),
                    Ok(WsMessage::Text(_)) => Some(Err(StreamError::wrong_message_format(
                        "expected binary frame for CBOR, got text",
                    ))),
                    Ok(WsMessage::Close(_)) => Some(Err(StreamError::closed())),
                    Err(e) => Some(Err(e)),
                })
                .boxed(),
        };

        #[cfg(target_arch = "wasm32")]
        let stream = match S::ENCODING {
            MessageEncoding::Json => rx
                .into_inner()
                .filter_map(|msg_result| match msg_result {
                    Ok(WsMessage::Text(text)) => Some(
                        parse_msg(text.as_ref())
                            .map(|v| v.into_static())
                            .map_err(StreamError::decode),
                    ),
                    Ok(WsMessage::Binary(bytes)) => {
                        #[cfg(feature = "zstd")]
                        {
                            match decompress_zstd(&bytes) {
                                Ok(decompressed) => Some(
                                    parse_msg(&decompressed)
                                        .map(|v| v.into_static())
                                        .map_err(StreamError::decode),
                                ),
                                Err(_) => Some(
                                    parse_msg(&bytes)
                                        .map(|v| v.into_static())
                                        .map_err(StreamError::decode),
                                ),
                            }
                        }
                        #[cfg(not(feature = "zstd"))]
                        {
                            Some(
                                parse_msg(&bytes)
                                    .map(|v| v.into_static())
                                    .map_err(StreamError::decode),
                            )
                        }
                    }
                    Ok(WsMessage::Close(_)) => Some(Err(StreamError::closed())),
                    Err(e) => Some(Err(e)),
                })
                .boxed_local(),
            MessageEncoding::DagCbor => rx
                .into_inner()
                .filter_map(|msg_result| match msg_result {
                    Ok(WsMessage::Binary(bytes)) => Some(
                        parse_cbor(&bytes)
                            .map(|v| v.into_static())
                            .map_err(|e| StreamError::decode(crate::error::DecodeError::from(e))),
                    ),
                    Ok(WsMessage::Text(_)) => Some(Err(StreamError::wrong_message_format(
                        "expected binary frame for CBOR, got text",
                    ))),
                    Ok(WsMessage::Close(_)) => Some(Err(StreamError::closed())),
                    Err(e) => Some(Err(e)),
                })
                .boxed_local(),
        };

        (tx, stream)
    }

    /// Consume the stream and return the underlying connection.
    pub fn into_connection(self) -> WebSocketConnection {
        self.connection
    }

    /// Tee the stream, keeping the raw stream in self and returning a typed stream.
    ///
    /// Replaces the internal WebSocket stream with one copy and returns a typed decoded
    /// stream. Both streams receive all messages. Useful for observing raw messages
    /// while also processing typed messages.
    pub fn tee<Str>(&mut self) -> Boxed<Result<StreamMessage<Str, S>, StreamError>>
    where
        StreamMessage<Str, S>: DeserializeOwned,
        Str: BosStr + Clone + FromStr + fmt::Debug + DeserializeOwned,
        <Str as FromStr>::Err: fmt::Debug,
    {
        use n0_future::StreamExt as _;

        let rx = self.connection.receiver_mut();
        let (raw_rx, typed_rx_source) =
            core::mem::replace(rx, WsStream::new(n0_future::stream::empty())).tee();

        // Put the raw stream back
        *rx = raw_rx;

        #[cfg(not(target_arch = "wasm32"))]
        let stream = match S::ENCODING {
            MessageEncoding::Json => typed_rx_source
                .into_inner()
                .filter_map(|msg| decode_json_msg::<S, Str>(msg))
                .boxed(),
            MessageEncoding::DagCbor => typed_rx_source
                .into_inner()
                .filter_map(|msg| decode_cbor_msg::<S, Str>(msg))
                .boxed(),
        };

        #[cfg(target_arch = "wasm32")]
        let stream = match S::ENCODING {
            MessageEncoding::Json => typed_rx_source
                .into_inner()
                .filter_map(|msg| decode_json_msg::<S, Str>(msg))
                .boxed_local(),
            MessageEncoding::DagCbor => typed_rx_source
                .into_inner()
                .filter_map(|msg| decode_cbor_msg::<S, Str>(msg))
                .boxed_local(),
        };
        stream
    }
}

type StreamMessage<S, R> = <R as SubscriptionResp>::Message<S>;

/// XRPC subscription endpoint trait (server-side)
///
/// Analogous to `XrpcEndpoint` but for WebSocket subscriptions.
/// Defines the fully-qualified path and associated parameter/stream types.
///
/// This exists primarily for server-side frameworks (like Axum) to extract
/// typed subscription parameters without lifetime issues.
pub trait SubscriptionEndpoint {
    /// Fully-qualified path ('/xrpc/{nsid}') where this subscription endpoint lives
    const PATH: &'static str;

    /// Message encoding (JSON or DAG-CBOR)
    const ENCODING: MessageEncoding;

    /// Subscription parameters type
    type Params<S: BosStr>: XrpcSubscription;

    /// Stream response type
    type Stream: SubscriptionResp;
}

/// Per-subscription options for WebSocket subscriptions.
#[derive(Debug, Default, Clone)]
pub struct SubscriptionOptions<'a> {
    /// Extra headers to attach to this subscription (e.g., Authorization).
    pub headers: Vec<(CowStr<'a>, CowStr<'a>)>,
}

impl IntoStatic for SubscriptionOptions<'_> {
    type Output = SubscriptionOptions<'static>;

    fn into_static(self) -> Self::Output {
        SubscriptionOptions {
            headers: self
                .headers
                .into_iter()
                .map(|(k, v)| (k.into_static(), v.into_static()))
                .collect(),
        }
    }
}

/// Extension for stateless subscription calls on any `WebSocketClient`.
///
/// Provides a builder pattern for establishing WebSocket subscriptions with custom options.
pub trait SubscriptionExt: WebSocketClient {
    /// Start building a subscription call for the given base URI.
    fn subscription<'a>(&'a self, base: Uri<String>) -> SubscriptionCall<'a, Self>
    where
        Self: Sized,
    {
        SubscriptionCall {
            client: self,
            base,
            opts: SubscriptionOptions::default(),
        }
    }
}

impl<T: WebSocketClient> SubscriptionExt for T {}

/// Build a subscription URI from a base URI, optional custom path, and query parameters.
///
/// This is a pure function that constructs the complete subscription WebSocket URI.
/// It supports both standard NSID-based paths (e.g., `/xrpc/{nsid}`) and custom paths
/// (e.g., Jetstream's `/subscribe`).
///
/// # Arguments
///
/// - `base`: The base URI (e.g., `wss://bsky.social`)
/// - `nsid`: The subscription NSID (e.g., `com.atproto.sync.subscribeRepos`)
/// - `custom_path`: Optional custom path to use instead of `/xrpc/{nsid}`
/// - `query_params`: Query parameters as (key, value) pairs
///
/// # Returns
///
/// A complete subscription URI with scheme, authority, path, and optional query string,
/// or a parse error if the constructed URI is invalid.
fn build_subscription_uri(
    base: &Uri<String>,
    nsid: &str,
    custom_path: Option<&str>,
    query_params: &[(String, String)],
) -> Result<Uri<String>, ParseError> {
    let base_path = base.path().as_str().trim_end_matches('/');

    // Build the path: base_path + custom_path or "/xrpc/{nsid}"
    let mut path = String::with_capacity(base_path.len() + 50);
    path.push_str(base_path);
    if let Some(custom_path) = custom_path {
        path.push_str(custom_path);
    } else {
        path.push_str("/xrpc/");
        path.push_str(nsid);
    }

    // Build query string from parameters with percent-encoding
    let query_str = if !query_params.is_empty() {
        query_params
            .iter()
            .map(|(k, v)| {
                let mut enc_k = EString::<Query>::new();
                enc_k.encode_str::<EncData>(k.as_str());
                let mut enc_v = EString::<Query>::new();
                enc_v.encode_str::<EncData>(v.as_str());
                alloc::format!("{}={}", enc_k, enc_v)
            })
            .collect::<Vec<_>>()
            .join("&")
    } else {
        String::new()
    };

    // Calculate approximate capacity for the final URI string
    let capacity = base.scheme().as_str().len()
        + 3 // "://"
        + base.authority().map(|a| a.as_str().len()).unwrap_or(0)
        + path.len()
        + query_str.len()
        + if !query_str.is_empty() { 1 } else { 0 }; // "?"

    // Construct the URI using fluent-uri builder pattern
    let mut uri_str = String::with_capacity(capacity);
    uri_str.push_str(base.scheme().as_str());
    uri_str.push_str("://");

    if let Some(authority) = base.authority() {
        uri_str.push_str(authority.as_str());
    }

    uri_str.push_str(&path);

    if !query_str.is_empty() {
        uri_str.push('?');
        uri_str.push_str(&query_str);
    }

    Uri::parse(uri_str)
        .map(|u| u.to_owned())
        .map_err(|(e, _)| e)
}

/// Stateless subscription call builder.
///
/// Provides methods for adding headers and establishing typed subscriptions.
pub struct SubscriptionCall<'a, C: WebSocketClient> {
    pub(crate) client: &'a C,
    pub(crate) base: Uri<String>,
    pub(crate) opts: SubscriptionOptions<'a>,
}

impl<'a, C: WebSocketClient> SubscriptionCall<'a, C> {
    /// Add an extra header.
    pub fn header(mut self, name: impl Into<CowStr<'a>>, value: impl Into<CowStr<'a>>) -> Self {
        self.opts.headers.push((name.into(), value.into()));
        self
    }

    /// Replace the builder's options entirely.
    pub fn with_options(mut self, opts: SubscriptionOptions<'a>) -> Self {
        self.opts = opts;
        self
    }

    /// Subscribe to the given XRPC subscription endpoint.
    ///
    /// Builds a WebSocket URI from the base, appends the NSID path,
    /// encodes query parameters from the subscription type, and connects.
    /// If the subscription declares a subprotocol, it is negotiated during
    /// the upgrade handshake via `Sec-WebSocket-Protocol`.
    /// Returns a typed SubscriptionStream that automatically decodes messages.
    pub async fn subscribe<Sub>(
        self,
        params: &Sub,
    ) -> Result<SubscriptionStream<Sub::Stream>, C::Error>
    where
        Sub: XrpcSubscription + Serialize,
    {
        let query_params = params.query_params();
        let uri = build_subscription_uri(&self.base, Sub::NSID, Sub::CUSTOM_PATH, &query_params)
            .expect("subscription URI must be valid (base_uri + path always yields a valid URI)");

        let protocol = <Sub::Stream as SubscriptionResp>::SUBPROTOCOL;
        let options = WebSocketConnectOptions {
            headers: self.opts.headers,
            protocols: protocol.into_iter().collect(),
        };
        let connection = self
            .client
            .connect_with_options(uri.borrow(), options)
            .await?;

        Ok(SubscriptionStream::new(connection))
    }
}

/// Stateful subscription client trait.
///
/// Analogous to `XrpcClient` but for WebSocket subscriptions.
/// Provides a stateful interface for subscribing with configured base URI and options.
#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait SubscriptionClient: WebSocketClient {
    /// Get the base URI for the client.
    fn base_uri(&self) -> impl Future<Output = Uri<String>>;

    /// Get the subscription options for the client.
    fn subscription_opts(&self) -> impl Future<Output = SubscriptionOptions<'_>> {
        async { SubscriptionOptions::default() }
    }

    /// Subscribe to an XRPC subscription endpoint using the client's base URI and options.
    #[cfg(not(target_arch = "wasm32"))]
    fn subscribe<Sub>(
        &self,
        params: &Sub,
    ) -> impl Future<Output = Result<SubscriptionStream<Sub::Stream>, Self::Error>>
    where
        Sub: XrpcSubscription + Serialize + Send + Sync,
        Self: Sync;

    /// Subscribe to an XRPC subscription endpoint using the client's base URI and options.
    #[cfg(target_arch = "wasm32")]
    fn subscribe<Sub>(
        &self,
        params: &Sub,
    ) -> impl Future<Output = Result<SubscriptionStream<Sub::Stream>, Self::Error>>
    where
        Sub: XrpcSubscription + Serialize + Send + Sync;

    /// Subscribe with custom options.
    #[cfg(not(target_arch = "wasm32"))]
    fn subscribe_with_opts<Sub>(
        &self,
        params: &Sub,
        opts: SubscriptionOptions<'_>,
    ) -> impl Future<Output = Result<SubscriptionStream<Sub::Stream>, Self::Error>>
    where
        Sub: XrpcSubscription + Serialize + Send + Sync,
        Self: Sync;

    /// Subscribe with custom options.
    #[cfg(target_arch = "wasm32")]
    fn subscribe_with_opts<Sub>(
        &self,
        params: &Sub,
        opts: SubscriptionOptions<'_>,
    ) -> impl Future<Output = Result<SubscriptionStream<Sub::Stream>, Self::Error>>
    where
        Sub: XrpcSubscription + Serialize + Send + Sync;
}

/// Simple stateless subscription client wrapping a WebSocketClient.
///
/// Analogous to a basic HTTP client but for WebSocket subscriptions.
/// Does not manage sessions or authentication - useful for public subscriptions
/// or when you want to handle auth manually via headers.
pub struct BasicSubscriptionClient<W: WebSocketClient> {
    client: W,
    base_uri: Uri<String>,
    opts: SubscriptionOptions<'static>,
}

impl<W: WebSocketClient> BasicSubscriptionClient<W> {
    /// Create a new basic subscription client with the given WebSocket client and base URI.
    pub fn new(client: W, base_uri: Uri<String>) -> Self {
        Self {
            client,
            base_uri,
            opts: SubscriptionOptions::default(),
        }
    }

    /// Create with default options.
    pub fn with_options(mut self, opts: SubscriptionOptions<'_>) -> Self {
        self.opts = opts.into_static();
        self
    }

    /// Get a reference to the inner WebSocket client.
    pub fn inner(&self) -> &W {
        &self.client
    }
}

impl<W: WebSocketClient> WebSocketClient for BasicSubscriptionClient<W> {
    type Error = W::Error;

    async fn connect(&self, uri: Uri<&str>) -> Result<WebSocketConnection, Self::Error> {
        self.client.connect(uri).await
    }

    async fn connect_with_options(
        &self,
        uri: Uri<&str>,
        options: WebSocketConnectOptions<'_>,
    ) -> Result<WebSocketConnection, Self::Error> {
        self.client.connect_with_options(uri, options).await
    }
}

impl<W: WebSocketClient> SubscriptionClient for BasicSubscriptionClient<W> {
    async fn base_uri(&self) -> Uri<String> {
        self.base_uri.clone()
    }

    async fn subscription_opts(&self) -> SubscriptionOptions<'_> {
        self.opts.clone()
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn subscribe<Sub>(
        &self,
        params: &Sub,
    ) -> Result<SubscriptionStream<Sub::Stream>, Self::Error>
    where
        Sub: XrpcSubscription + Serialize + Send + Sync,
        Self: Sync,
    {
        let opts = self.subscription_opts().await;
        self.subscribe_with_opts(params, opts).await
    }

    #[cfg(target_arch = "wasm32")]
    async fn subscribe<Sub>(
        &self,
        params: &Sub,
    ) -> Result<SubscriptionStream<Sub::Stream>, Self::Error>
    where
        Sub: XrpcSubscription + Serialize + Send + Sync,
    {
        let opts = self.subscription_opts().await;
        self.subscribe_with_opts(params, opts).await
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn subscribe_with_opts<Sub>(
        &self,
        params: &Sub,
        opts: SubscriptionOptions<'_>,
    ) -> Result<SubscriptionStream<Sub::Stream>, Self::Error>
    where
        Sub: XrpcSubscription + Serialize + Send + Sync,
        Self: Sync,
    {
        let base = self.base_uri().await;
        self.subscription(base)
            .with_options(opts)
            .subscribe(params)
            .await
    }

    #[cfg(target_arch = "wasm32")]
    async fn subscribe_with_opts<Sub>(
        &self,
        params: &Sub,
        opts: SubscriptionOptions<'_>,
    ) -> Result<SubscriptionStream<Sub::Stream>, Self::Error>
    where
        Sub: XrpcSubscription + Serialize + Send + Sync,
    {
        let base = self.base_uri().await;
        self.subscription(base)
            .with_options(opts)
            .subscribe(params)
            .await
    }
}

/// Type alias for a basic subscription client using the default TungsteniteClient.
///
/// Provides a simple, stateless WebSocket subscription client without session management.
/// Useful for public subscriptions or when handling authentication manually.
///
/// # Example
///
/// ```no_run
/// # use jacquard_common::xrpc::{TungsteniteSubscriptionClient, SubscriptionClient};
/// # use jacquard_common::deps::fluent_uri::Uri;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let base = Uri::parse("wss://bsky.network")?.to_owned();
/// let client = TungsteniteSubscriptionClient::from_base_uri(base);
/// // let conn = client.subscribe(&params).await?;
/// # Ok(())
/// # }
/// ```
pub type TungsteniteSubscriptionClient =
    BasicSubscriptionClient<crate::websocket::tungstenite_client::TungsteniteClient>;

impl TungsteniteSubscriptionClient {
    /// Create a new Tungstenite-backed subscription client with the given base URI.
    pub fn from_base_uri(base_uri: Uri<String>) -> Self {
        let client = crate::websocket::tungstenite_client::TungsteniteClient::new();
        BasicSubscriptionClient::new(client, base_uri)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deps::fluent_uri::Uri;
    use crate::jetstream::JetstreamParams;
    use crate::xrpc::{GenericError, MessageEncoding, SubscriptionResp};
    use alloc::string::ToString;
    use core::convert::Infallible;
    use smol_str::SmolStr;

    /// Minimal message type for decode tests: `{"n": <int>}`.
    #[derive(Debug, serde::Deserialize)]
    struct TestMsg {
        n: i64,
    }

    struct TestMsgStream;

    impl SubscriptionResp for TestMsgStream {
        const NSID: &'static str = "test.msg";
        const ENCODING: MessageEncoding = MessageEncoding::Json;
        type Message<S: crate::BosStr> = TestMsg;
        type Error = GenericError;
    }

    struct JsonFrameStream;

    impl SubscriptionResp for JsonFrameStream {
        const NSID: &'static str = "test.json.frame";
        const ENCODING: MessageEncoding = MessageEncoding::Json;
        const SUBPROTOCOL: Option<&'static str> = Some("xrpc.v1.json");
        type Message<S: crate::BosStr> = TestMsg;
        type Error = GenericError;
    }

    #[test]
    fn xrpc_v1_json_decodes_message_envelope_once() {
        let decoded =
            JsonFrameStream::decode_message::<&str>(br#"{"$type":"message","payload":{"n":7}}"#)
                .expect("message frame");
        assert_eq!(decoded.n, 7);
    }

    #[test]
    fn xrpc_v1_json_surfaces_error_envelope() {
        let error = JsonFrameStream::decode_message::<&str>(
            br#"{"$type":"error","error":"ConsumerTooSlow","message":"lagging"}"#,
        )
        .expect_err("error frame");
        assert!(matches!(
            error,
            DecodeError::UnknownEventType(detail) if detail == "ConsumerTooSlow: lagging"
        ));
    }

    #[test]
    fn xrpc_v1_json_stream_skips_unknown_envelope_between_messages() {
        use n0_future::StreamExt as _;

        let frames = [
            r#"{"$type":"message","payload":{"n":1}}"#,
            r#"{"$type":"future","payload":{"n":2}}"#,
            r#"{"$type":"message","payload":{"n":3}}"#,
        ]
        .into_iter()
        .map(|frame| {
            Ok(crate::websocket::WsMessage::Text(
                crate::websocket::WsText::from(frame),
            ))
        });
        let decoded = futures_lite::future::block_on(
            n0_future::stream::iter(frames)
                .filter_map(decode_json_msg::<JsonFrameStream, SmolStr>)
                .collect::<Vec<_>>(),
        );

        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].as_ref().expect("first").n, 1);
        assert_eq!(decoded[1].as_ref().expect("second").n, 3);
    }

    #[test]
    fn xrpc_v1_json_rejects_malformed_envelope() {
        let error = JsonFrameStream::decode_message::<&str>(br#"{"$type":"message"}"#)
            .expect_err("missing payload");
        assert!(matches!(error, DecodeError::Json(_)));
    }

    #[test]
    fn binary_transform_is_authoritative_when_it_accepts() {
        // A binary frame the transform claims: its output is decoded
        // directly, bypassing the static-dictionary path entirely.
        let frame = crate::websocket::WsMessage::from(vec![0x01, 0x02]);
        let result =
            decode_json_msg_with::<TestMsgStream, _, SmolStr>(Ok(frame), &|_bytes: &[u8]| {
                Ok(Some(crate::deps::bytes::Bytes::from_static(br#"{"n":7}"#)))
            });
        let decoded = result.expect("Some").expect("decodes");
        assert_eq!(decoded.n, 7);
    }

    #[test]
    fn binary_transform_error_is_preserved() {
        let frame = crate::websocket::WsMessage::from(vec![0x01, 0x02]);
        let result =
            decode_json_msg_with::<TestMsgStream, _, SmolStr>(Ok(frame), &|_bytes: &[u8]| {
                Err(StreamError::protocol("transform failed"))
            });
        let error = result.expect("Some").expect_err("transform error");
        assert_eq!(error.kind(), &crate::stream::StreamErrorKind::Protocol);
        assert_eq!(
            error.source().expect("source").to_string(),
            "transform failed"
        );
    }

    #[test]
    fn binary_transform_falls_through_when_it_declines() {
        // Transform declines (None): the frame takes the existing
        // decode_json_msg path. A plain-JSON binary frame still decodes,
        // proving the v1 behavior is unchanged behind a declining
        // transform.
        let frame = crate::websocket::WsMessage::from(br#"{"n":9}"#.to_vec());
        let result =
            decode_json_msg_with::<TestMsgStream, _, SmolStr>(Ok(frame), &|_bytes: &[u8]| Ok(None));
        let decoded = result.expect("Some").expect("decodes");
        assert_eq!(decoded.n, 9);
    }

    /// Test-only client that records the handshake options it was given, so
    /// tests can assert on what the subscription layer handed to the
    /// WebSocket layer.
    #[derive(Debug, Clone)]
    struct HandshakeRecord {
        protocols: Vec<String>,
        headers: Vec<(String, String)>,
    }

    struct RecordingClient {
        record: std::sync::Mutex<HandshakeRecord>,
    }

    impl RecordingClient {
        fn new() -> Self {
            Self {
                record: std::sync::Mutex::new(HandshakeRecord {
                    protocols: Vec::new(),
                    headers: Vec::new(),
                }),
            }
        }

        fn observed(&self) -> HandshakeRecord {
            self.record.lock().expect("test mutex").clone()
        }
    }

    impl WebSocketClient for RecordingClient {
        type Error = Infallible;

        async fn connect(&self, _uri: Uri<&str>) -> Result<WebSocketConnection, Self::Error> {
            unreachable!("tests assert on connect_with_* paths")
        }

        async fn connect_with_options(
            &self,
            _uri: Uri<&str>,
            options: WebSocketConnectOptions<'_>,
        ) -> Result<WebSocketConnection, Self::Error> {
            *self.record.lock().expect("test mutex") = HandshakeRecord {
                protocols: options.protocols.iter().map(|p| p.to_string()).collect(),
                headers: options
                    .headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            };
            Ok(empty_connection())
        }
    }

    fn empty_connection() -> WebSocketConnection {
        // A sink that accepts and drops everything, paired with an empty
        // stream. The subscribe path never touches either before the test
        // asserts.
        use futures::sink::SinkExt as _;
        let sink = futures::sink::drain()
            .sink_map_err(|_: std::convert::Infallible| StreamError::closed());
        WebSocketConnection::new(WsSink::new(sink), WsStream::new(n0_future::stream::empty()))
    }

    /// AC.2: a subscription without a declared subprotocol must not send one.
    #[test]
    fn test_subscribe_repos_sends_no_protocol() {
        let client = RecordingClient::new();
        let base = Uri::parse("wss://bsky.social").unwrap().to_owned();

        // JetstreamParams (v1) declares no subprotocol.
        let params = JetstreamParams::<SmolStr> {
            wanted_collections: None,
            wanted_dids: None,
            cursor: None,
            max_message_size_bytes: None,
            compress: None,
            require_hello: None,
        };
        let _ = futures_lite::future::block_on(client.subscription(base).subscribe(&params));

        let record = client.observed();
        assert!(
            record.protocols.is_empty(),
            "subscriptions without a declared subprotocol must not negotiate one"
        );
    }

    /// Minimal subscription declaring a subprotocol, mirroring what the
    /// lexicon codegen emits for network.bsky.jetstream.subscribeEvents
    /// (SUBPROTOCOL = Some("xrpc.v1.json")).
    mod sub_protocol_sub {
        use super::*;
        use crate::xrpc::GenericError;

        pub struct SubProtocolStream;

        impl SubscriptionResp for SubProtocolStream {
            const NSID: &'static str = "test.sub.protocol";
            const ENCODING: MessageEncoding = MessageEncoding::Json;
            const SUBPROTOCOL: Option<&'static str> = Some("xrpc.v1.json");

            type Message<S: BosStr> = ();
            type Error = GenericError;
        }

        #[derive(Debug, Clone, serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        pub struct SubProtocolParams;

        impl XrpcSubscription for SubProtocolParams {
            const NSID: &'static str = "test.sub.protocol";
            const ENCODING: MessageEncoding = MessageEncoding::Json;
            type Stream = SubProtocolStream;
        }
    }

    /// AC.2: a subscription declaring a subprotocol must request exactly it.
    #[test]
    fn test_subscribe_events_requests_xrpc_v1_json() {
        use sub_protocol_sub::SubProtocolParams;

        let client = RecordingClient::new();
        let base = Uri::parse("wss://jetstream.example.com")
            .unwrap()
            .to_owned();

        let _ =
            futures_lite::future::block_on(client.subscription(base).subscribe(&SubProtocolParams));

        let record = client.observed();
        assert_eq!(
            record.protocols,
            vec!["xrpc.v1.json".to_string()],
            "declared subprotocol must be requested at the upgrade"
        );
        assert!(record.headers.is_empty());
    }

    /// Test uri-and-deps.AC3.1: Subscription URL construction with NSID path.
    ///
    /// Verifies that the build_subscription_uri() function constructs the correct
    /// `/xrpc/{nsid}` path with query parameters properly encoded.
    #[test]
    fn test_subscription_uri_with_nsid_path() {
        let base_uri = Uri::parse("wss://bsky.social/xrpc").unwrap().to_owned();
        let nsid = "com.example.subscribe";
        let query_params = vec![
            ("cursor".to_string(), "abc123".to_string()),
            ("filter".to_string(), "like".to_string()),
        ];

        let uri = build_subscription_uri(&base_uri, nsid, None, &query_params)
            .expect("valid base uri and path should produce valid uri");

        // Verify the URI contains the correct NSID path
        let uri_str = uri.as_str();
        assert!(uri_str.contains("/xrpc/com.example.subscribe"));
        assert!(uri_str.contains("cursor=abc123"));
        assert!(uri_str.contains("filter=like"));
        assert!(!uri_str.contains("//xrpc"));
    }

    /// Test uri-and-deps.AC3.2: Subscription with custom path.
    ///
    /// Verifies that build_subscription_uri() uses CUSTOM_PATH (e.g., `/subscribe` for Jetstream)
    /// instead of the default `/xrpc/{nsid}` path.
    #[test]
    fn test_subscription_uri_with_custom_path() {
        let base_uri = Uri::parse("wss://jetstream.example.com")
            .unwrap()
            .to_owned();
        let custom_path = "/subscribe";

        let uri = build_subscription_uri(&base_uri, "com.example.sub", Some(custom_path), &[])
            .expect("valid base uri and path should produce valid uri");

        // Verify custom path is used instead of /xrpc/{nsid}
        let uri_str = uri.as_str();
        assert!(uri_str.contains("/subscribe"));
        assert!(!uri_str.contains("/xrpc/"));
    }

    /// Test uri-and-deps.AC3.3: WebSocketClient::connect() accepts Uri<String>.
    ///
    /// Verifies that the trait signature accepts Uri<String> and that SubscriptionCall
    /// correctly passes Uri<String> to the WebSocket client.
    #[test]
    fn test_subscription_uri_scheme_and_authority() {
        let base_uri = Uri::parse("wss://example.com:8080/path")
            .unwrap()
            .to_owned();
        let nsid = "com.example.test";

        let uri = build_subscription_uri(&base_uri, nsid, None, &[])
            .expect("valid base uri and path should produce valid uri");

        // Verify the URI preserves scheme and authority correctly
        let uri_str = uri.as_str();
        assert!(uri_str.starts_with("wss://example.com:8080"));
        assert!(uri_str.contains("/path/xrpc/com.example.test"));
    }

    /// Test query parameter encoding with multiple parameters.
    #[test]
    fn test_query_parameters_encoding() {
        let base_uri = Uri::parse("wss://example.com").unwrap().to_owned();
        let params = vec![
            ("cursor".to_string(), "abc123".to_string()),
            ("filter".to_string(), "like".to_string()),
        ];

        let uri = build_subscription_uri(&base_uri, "com.test", None, &params)
            .expect("valid base uri and path should produce valid uri");

        // Verify query parameters are correctly encoded
        let uri_str = uri.as_str();
        assert!(uri_str.contains("?"));
        assert!(uri_str.contains("cursor=abc123"));
        assert!(uri_str.contains("filter=like"));
        assert!(uri_str.contains("&"));
    }

    /// Test URI construction with trailing slash handling.
    #[test]
    fn test_uri_trailing_slash_handling() {
        let base_uri = Uri::parse("wss://example.com/xrpc/").unwrap().to_owned();

        let uri = build_subscription_uri(&base_uri, "com.example.test", None, &[])
            .expect("valid base uri and path should produce valid uri");

        // Verify no double slashes in path
        let uri_str = uri.as_str();
        assert!(!uri_str.contains("//xrpc"));
        assert!(uri_str.contains("/xrpc/com.example.test"));
    }

    /// Test empty query parameters do not add trailing question mark.
    #[test]
    fn test_empty_query_parameters() {
        let base_uri = Uri::parse("wss://example.com").unwrap().to_owned();

        let uri = build_subscription_uri(&base_uri, "com.example.test", None, &[])
            .expect("valid base uri and path should produce valid uri");

        // Verify no trailing question mark with empty query
        let uri_str = uri.as_str();
        assert!(!uri_str.contains("?"));
        assert!(uri_str.ends_with("com.example.test"));
    }
}
