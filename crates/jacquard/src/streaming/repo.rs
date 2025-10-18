//! Streaming support for repository operations

use bytes::Bytes;
use jacquard_api::com_atproto::repo::import_repo::ImportRepo;
use jacquard_common::{
    xrpc::streaming::{XrpcProcedureStream, XrpcStreamResp},
    StreamError,
};
use serde::{Deserialize, Serialize};

/// Streaming implementation for com.atproto.repo.importRepo
pub struct ImportRepoStream;

impl XrpcProcedureStream for ImportRepoStream {
    const NSID: &'static str = "com.atproto.repo.importRepo";
    const ENCODING: &'static str = "application/vnd.ipld.car";

    type Frame<'de> = Bytes;
    type Request = ImportRepo;
    type Response = ImportRepoStreamResponse;

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

/// Response marker for streaming importRepo
pub struct ImportRepoStreamResponse;

impl XrpcStreamResp for ImportRepoStreamResponse {
    const NSID: &'static str = "com.atproto.repo.importRepo";
    const ENCODING: &'static str = "application/json";

    type Frame<'de> = ();

    fn encode_frame<'de>(_data: Self::Frame<'de>) -> Result<Bytes, StreamError>
    where
        Self::Frame<'de>: Serialize,
    {
        Ok(Bytes::new())
    }

    fn decode_frame<'de>(_frame: &'de [u8]) -> Result<Self::Frame<'de>, StreamError>
    where
        Self::Frame<'de>: Deserialize<'de>,
    {
        Ok(())
    }
}

/// Streaming implementation for com.atproto.sync.getRepo
pub struct GetRepoStream;

impl XrpcStreamResp for GetRepoStream {
    const NSID: &'static str = "com.atproto.sync.getRepo";
    const ENCODING: &'static str = "application/vnd.ipld.car";

    type Frame<'de> = Bytes;

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
