//! Streaming support for video uploads

use bytes::Bytes;
use jacquard_api::app_bsky::video::upload_video::{UploadVideo, UploadVideoOutput};
use jacquard_common::{
    xrpc::streaming::{XrpcProcedureStream, XrpcStreamResp},
    StreamError,
};
use serde::{Deserialize, Serialize};

/// Streaming implementation for app.bsky.video.uploadVideo
pub struct UploadVideoStream;

impl XrpcProcedureStream for UploadVideoStream {
    const NSID: &'static str = "app.bsky.video.uploadVideo";
    const ENCODING: &'static str = "video/mp4";

    type Frame<'de> = Bytes;
    type Request = UploadVideo;
    type Response = UploadVideoStreamResponse;

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

/// Response marker for streaming uploadVideo
pub struct UploadVideoStreamResponse;

impl XrpcStreamResp for UploadVideoStreamResponse {
    const NSID: &'static str = "app.bsky.video.uploadVideo";
    const ENCODING: &'static str = "application/json";

    type Frame<'de> = UploadVideoOutput<'de>;

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
