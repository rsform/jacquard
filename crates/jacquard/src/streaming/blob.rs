//! Streaming support for blob uploads

use bytes::Bytes;
use jacquard_api::com_atproto::repo::upload_blob::{UploadBlob, UploadBlobOutput};
use jacquard_common::{
    StreamError,
    xrpc::streaming::{XrpcProcedureStream, XrpcStreamResp},
};
use serde::{Deserialize, Serialize};

/// Streaming implementation for com.atproto.repo.uploadBlob
pub struct UploadBlobStream;

impl XrpcProcedureStream for UploadBlobStream {
    const NSID: &'static str = "com.atproto.repo.uploadBlob";
    const ENCODING: &'static str = "*/*";

    type Frame<'de> = Bytes;
    type Request = UploadBlob;
    type Response = UploadBlobStreamResponse;

    fn encode_frame<'de>(data: Self::Frame<'de>) -> Result<Bytes, StreamError>
    where
        Self::Frame<'de>: Serialize,
    {
        Ok(data)
    }

    fn decode_frame<'de>(frame: &'de [u8]) -> Result<Self::Frame<'de>, StreamError>
    where
        Self::Frame<'de>: Deserialize<'de>,
    {
        Ok(Bytes::copy_from_slice(frame))
    }
}

/// Response marker for streaming uploadBlob
pub struct UploadBlobStreamResponse;

impl XrpcStreamResp for UploadBlobStreamResponse {
    const NSID: &'static str = "com.atproto.repo.uploadBlob";
    const ENCODING: &'static str = "application/json";

    type Frame<'de> = UploadBlobOutput<'de>;

    fn encode_frame<'de>(data: Self::Frame<'de>) -> Result<Bytes, StreamError>
    where
        Self::Frame<'de>: Serialize,
    {
        Ok(Bytes::from_owner(
            serde_json::to_vec(&data).map_err(StreamError::encode)?,
        ))
    }

    fn decode_frame<'de>(frame: &'de [u8]) -> Result<Self::Frame<'de>, StreamError>
    where
        Self::Frame<'de>: Deserialize<'de>,
    {
        Ok(serde_json::from_slice(frame).map_err(StreamError::decode)?)
    }
}
