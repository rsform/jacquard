pub trait DynXrpcRequest {
    fn nsid(&self) -> Nsid;
    fn method(&self) -> XrpcMethod;
    fn response_type(&self) -> &'static str;
    fn encode_body(&self) -> Result<Vec<u8>, EncodeError>;
}

pub trait DynXrpcResp {
    fn nsid(&self) -> Nsid;
    fn encoding(&self) -> &'static str;
    fn decode_output(&self, body: &[u8]) -> Result<Data<'_>, DecodeError>;
}

impl<XRPC> DynXrpcRequest for XRPC
where
    XRPC: XrpcRequest,
{
    fn nsid(&self) -> Nsid {
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
    fn nsid(&self) -> Nsid {
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

pub struct DynResponse {
    buffer: Bytes,
    status: StatusCode,
}

impl DynResponse {
    pub fn new(buffer: Bytes, status: StatusCode) -> Self {
        Self { buffer, status }
    }

    /// Parse the response into an owned output
    pub fn into_output<R>(self) -> Result<RespOutput<'static, R>, XrpcError<RespErr<'static, R>>>
    where
        R: XrpcResp,
        for<'a> RespOutput<'a, R>: IntoStatic<Output = RespOutput<'static, R>>,
        for<'a> RespErr<'a, R>: IntoStatic<Output = RespErr<'static, R>>,
    {
        fn parse_error<'b, R: XrpcResp>(buffer: &'b [u8]) -> Result<R::Err<'b>, serde_json::Error> {
            serde_json::from_slice(buffer)
        }

        // 200: parse as output
        if self.status.is_success() {
            match R::decode_output(&self.buffer) {
                Ok(output) => Ok(output.into_static()),
                Err(e) => Err(XrpcError::Decode(e)),
            }
        // 400: try typed XRPC error, fallback to generic error
        } else if self.status.as_u16() == 400 {
            let error = match parse_error::<R>(&self.buffer) {
                Ok(error) => XrpcError::Xrpc(error),
                Err(_) => {
                    // Fallback to generic error (InvalidRequest, ExpiredToken, etc.)
                    match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                        Ok(mut generic) => {
                            generic.nsid = R::NSID;
                            generic.method = ""; // method info only available on request
                            generic.http_status = self.status;
                            // Map auth-related errors to AuthError
                            match generic.error.as_ref() {
                                "ExpiredToken" => XrpcError::Auth(AuthError::TokenExpired),
                                "InvalidToken" => XrpcError::Auth(AuthError::InvalidToken),
                                _ => XrpcError::Generic(generic),
                            }
                        }
                        Err(e) => XrpcError::Decode(DecodeError::Json(e)),
                    }
                }
            };
            Err(error.into_static())
        // 401: always auth error
        } else {
            let error: XrpcError<<R as XrpcResp>::Err<'_>> =
                match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                    Ok(mut generic) => {
                        let status = self.status;
                        generic.nsid = R::NSID;
                        generic.method = ""; // method info only available on request
                        generic.http_status = status;
                        match generic.error.as_ref() {
                            "ExpiredToken" => XrpcError::Auth(AuthError::TokenExpired),
                            "InvalidToken" => XrpcError::Auth(AuthError::InvalidToken),
                            _ => XrpcError::Auth(AuthError::NotAuthenticated),
                        }
                    }
                    Err(e) => XrpcError::Decode(DecodeError::Json(e)),
                };

            Err(error.into_static())
        }
    }
}
