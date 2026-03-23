//! Streaming support for repository operations

use bytes::Bytes;
use jacquard_api::com_atproto::repo::import_repo::ImportRepo;
use jacquard_common::{
    BosStr, StreamError,
    xrpc::streaming::{XrpcProcedureStream, XrpcStreamResp},
};
use serde::{Deserialize, Serialize};

/// Streaming implementation for com.atproto.repo.importRepo
pub struct ImportRepoStream;

impl XrpcProcedureStream for ImportRepoStream {
    const NSID: &'static str = "com.atproto.repo.importRepo";
    const ENCODING: &'static str = "application/vnd.ipld.car";

    type Frame<S: BosStr> = Bytes;
    type Request = ImportRepo;
    type Response = ImportRepoStreamResponse;

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

/// Response marker for streaming importRepo
pub struct ImportRepoStreamResponse;

impl XrpcStreamResp for ImportRepoStreamResponse {
    const NSID: &'static str = "com.atproto.repo.importRepo";
    const ENCODING: &'static str = "application/json";

    type Frame<S: BosStr> = ();

    fn encode_frame<S: BosStr>(_data: Self::Frame<S>) -> Result<Bytes, StreamError>
    where
        Self::Frame<S>: Serialize,
    {
        Ok(Bytes::new())
    }

    fn decode_frame<'de, S: BosStr>(_frame: &'de [u8]) -> Result<Self::Frame<S>, StreamError>
    where
        Self::Frame<S>: Deserialize<'de>,
    {
        Ok(())
    }
}

/// Streaming implementation for com.atproto.sync.getRepo
pub struct GetRepoStream;

impl XrpcStreamResp for GetRepoStream {
    const NSID: &'static str = "com.atproto.sync.getRepo";
    const ENCODING: &'static str = "application/vnd.ipld.car";

    type Frame<S: BosStr> = Bytes;

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
