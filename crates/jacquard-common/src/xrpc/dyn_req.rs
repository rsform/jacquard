pub trait DynXrpcRequest {
    fn nsid(&self) -> Nsid<'static>;
    fn method(&self) -> XrpcMethod;
    fn response_type(&self) -> &'static str;
    fn encode_body(&self) -> Result<Vec<u8>, EncodeError>;
}

pub trait DynXrpcResp {
    fn nsid(&self) -> Nsid<'static>;
    fn encoding(&self) -> &'static str;
    fn decode_output(&self, body: &[u8]) -> Result<Data<'_>, DecodeError>;
}

impl<XRPC> DynXrpcRequest for XRPC
where
    XRPC: XrpcRequest,
{
    fn nsid(&self) -> Nsid<'static> {
        unsafe { Nsid::new_static(XRPC::NSID).unwrap_unchecked() }
    }

    fn method(&self) -> XrpcMethod {
        XRPC::METHOD
    }

    fn response_type(&self) -> &'static str {
        <XRPC::Response as XrpcResp>::ENCODING
    }

    fn encode_body(&self) -> Result<Vec<u8>, EncodeError> {
        XRPC::encode_body(self)
    }
}

impl<XRPC> DynXrpcResp for XRPC
where
    XRPC: XrpcResp,
{
    fn nsid(&self) -> Nsid<'static> {
        unsafe { Nsid::new_static(XRPC::NSID).unwrap_unchecked() }
    }

    fn encoding(&self) -> &'static str {
        XRPC::ENCODING
    }

    fn decode_output(&self, body: &[u8]) -> Result<Data<'_>, DecodeError> {
        if self.encoding() == "application/json" {
            Ok(serde_json::from_slice::<Data>(body)?.into_static())
        } else if self.encoding() == "application/vnd.ipld.car" {
            Ok(serde_ipld_dagcbor::from_slice::<Data>(body)?.into_static())
        } else {
            Ok(Data::Bytes(Bytes::copy_from_slice(body)))
        }
    }
}
