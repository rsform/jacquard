//! Streaming support for video uploads

use bytes::Bytes;
use jacquard_api::app_bsky::video::upload_video::{UploadVideo, UploadVideoOutput};
use jacquard_common::{
    BosStr, StreamError,
    xrpc::streaming::{XrpcProcedureStream, XrpcStreamResp},
};
use serde::{Deserialize, Serialize};

/// Streaming implementation for app.bsky.video.uploadVideo
pub struct UploadVideoStream;

impl XrpcProcedureStream for UploadVideoStream {
    const NSID: &'static str = "app.bsky.video.uploadVideo";
    const ENCODING: &'static str = "video/mp4";

    type Frame<S: BosStr> = Bytes;
    type Request = UploadVideo;
    type Response = UploadVideoStreamResponse;

    fn encode_frame<S: BosStr>(data: Self::Frame<S>) -> Result<Bytes, StreamError>
    where
        Self::Frame<S>: Serialize,
    {
        Ok(data)
    }

    fn decode_frame<'de, S: BosStr>(frame: &'de [u8]) -> Result<Self::Frame<S>, StreamError>
    where
        Self::Frame<S>: Deserialize<'de>,
    {
        Ok(Bytes::copy_from_slice(frame))
    }
}

/// Response marker for streaming uploadVideo
pub struct UploadVideoStreamResponse;

impl XrpcStreamResp for UploadVideoStreamResponse {
    const NSID: &'static str = "app.bsky.video.uploadVideo";
    const ENCODING: &'static str = "application/json";

    type Frame<S: BosStr> = UploadVideoOutput<S>;

    fn encode_frame<S: BosStr>(data: Self::Frame<S>) -> Result<Bytes, StreamError>
    where
        Self::Frame<S>: Serialize,
    {
        Ok(Bytes::from_owner(
            serde_json::to_vec(&data).map_err(StreamError::encode)?,
        ))
    }

    fn decode_frame<'de, S: BosStr>(frame: &'de [u8]) -> Result<Self::Frame<S>, StreamError>
    where
        Self::Frame<S>: Deserialize<'de>,
    {
        Ok(serde_json::from_slice(frame).map_err(StreamError::decode)?)
    }
}
