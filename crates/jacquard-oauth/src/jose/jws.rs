use jacquard_common::{CowStr, IntoStatic};
use jose_jwa::Algorithm;
use jose_jwk::Jwk;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header<'a> {
    #[serde(flatten)]
    #[serde(borrow)]
    pub registered: RegisteredHeader<'a>,
}

impl<'a> From<Header<'a>> for super::super::jose::Header<'a> {
    fn from(header: Header<'a>) -> Self {
        super::super::jose::Header::Jws(header)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]

pub struct RegisteredHeader<'a> {
    pub alg: Algorithm,
    #[serde(borrow)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jku: Option<CowStr<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwk: Option<Jwk>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kid: Option<CowStr<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x5u: Option<CowStr<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x5c: Option<CowStr<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x5t: Option<CowStr<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "x5t#S256")]
    pub x5ts256: Option<CowStr<'a>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub typ: Option<CowStr<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cty: Option<CowStr<'a>>,
}

impl From<Algorithm> for RegisteredHeader<'_> {
    fn from(alg: Algorithm) -> Self {
        Self {
            alg,
            jku: None,
            jwk: None,
            kid: None,
            x5u: None,
            x5c: None,
            x5t: None,
            x5ts256: None,
            typ: None,
            cty: None,
        }
    }
}

impl<'a> From<RegisteredHeader<'a>> for super::super::jose::Header<'a> {
    fn from(registered: RegisteredHeader<'a>) -> Self {
        super::super::jose::Header::Jws(Header { registered })
    }
}

impl IntoStatic for RegisteredHeader<'_> {
    type Output = RegisteredHeader<'static>;
    fn into_static(self) -> Self::Output {
        RegisteredHeader {
            alg: self.alg,
            jku: self.jku.map(IntoStatic::into_static),
            jwk: self.jwk,
            kid: self.kid.map(IntoStatic::into_static),
            x5u: self.x5u.map(IntoStatic::into_static),
            x5c: self.x5c.map(IntoStatic::into_static),
            x5t: self.x5t.map(IntoStatic::into_static),
            x5ts256: self.x5ts256.map(IntoStatic::into_static),
            typ: self.typ.map(IntoStatic::into_static),
            cty: self.cty.map(IntoStatic::into_static),
        }
    }
}
