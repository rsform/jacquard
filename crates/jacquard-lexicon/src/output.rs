use crate::lexicon::*;
use heck::{ToPascalCase, ToShoutySnakeCase, ToSnakeCase};
use itertools::Itertools;
use jacquard_common::CowStr;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::collections::{HashMap, HashSet};
use syn::{Path, Result};

fn string_type<'s>(string: &'s LexString<'s>) -> Result<(TokenStream, TokenStream)> {
    let description = description(&string.description);
    let typ = match string.format {
        Some(LexStringFormat::AtIdentifier) => {
            quote!(jacquard_common::types::string::AtIdentifier<'s>)
        }
        Some(LexStringFormat::Cid) => quote!(jacquard_common::types::string::Cid<'s>),
        Some(LexStringFormat::Datetime) => quote!(jacquard_common::types::string::Datetime),
        Some(LexStringFormat::Did) => quote!(jacquard_common::types::string::Did<'s>),
        Some(LexStringFormat::Handle) => quote!(jacquard_common::types::string::Handle<'s>),
        Some(LexStringFormat::Nsid) => quote!(jacquard_common::types::string::Nsid<'s>),
        Some(LexStringFormat::Language) => quote!(jacquard_common::types::string::Language),
        Some(LexStringFormat::Tid) => quote!(jacquard_common::types::string::Tid),
        Some(LexStringFormat::RecordKey) => quote!(
            jacquard_common::types::string::RecordKey<jacquard_common::types::string::Rkey<'s>>
        ),
        Some(LexStringFormat::Uri) => quote!(jacquard_common::types::string::Uri<'s>),
        Some(LexStringFormat::AtUri) => quote!(jacquard_common::types::string::AtUri<'s>),
        // TODO: other formats (uri, at-uri)
        _ => quote!(CowStr<'s>),
    };
    Ok((description, typ))
}

fn description<'s>(description: &Option<CowStr<'s>>) -> TokenStream {
    if let Some(description) = description {
        let description = description.as_ref();
        quote!(#[doc = #description])
    } else {
        quote!()
    }
}
