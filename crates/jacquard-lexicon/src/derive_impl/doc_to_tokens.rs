//! Serialize LexiconDoc values to TokenStream for macro codegen

use crate::lexicon::*;
use crate::schema::from_ast::{ConstraintCheck, ValidationCheck};
use jacquard_common::deps::smol_str::SmolStr;
use proc_macro2::TokenStream;
use quote::quote;
use std::collections::BTreeMap;
use syn;

/// Path prefixes for generated lexicon doc literals.
///
/// In derive macro context, all paths are fully qualified (`::jacquard_lexicon::lexicon::LexUserType`).
/// In codegen pretty mode, paths are bare (`LexUserType`) because the generated function body
/// includes scoped `use` imports.
pub(crate) struct DocPaths {
    /// Prefix for `jacquard_lexicon::lexicon::` types. Empty in short mode.
    lex: TokenStream,
    /// Path to `CowStr`. Fully qualified or bare.
    cow: TokenStream,
    /// Path to `SmolStr::new_static`. Fully qualified or bare.
    smol: TokenStream,
    /// Path to `BTreeMap`. Fully qualified or bare.
    btree: TokenStream,
    /// Path to `Vec`. Fully qualified or bare.
    vec: TokenStream,
    /// Path to `MimeType(...)`. Fully qualified or bare.
    mime: TokenStream,
}

impl DocPaths {
    /// Fully qualified paths for derive macro / proc-macro context.
    pub(crate) fn qualified() -> Self {
        Self {
            lex: quote! { ::jacquard_lexicon::lexicon:: },
            cow: quote! { ::jacquard_common::CowStr },
            smol: quote! { ::jacquard_common::deps::smol_str::SmolStr },
            btree: quote! { ::alloc::collections::BTreeMap },
            vec: quote! { ::alloc::vec::Vec },
            mime: quote! { jacquard_common::types::blob::MimeType },
        }
    }

    /// Short paths for codegen pretty mode. Requires function-scoped imports.
    pub(crate) fn short() -> Self {
        Self {
            lex: TokenStream::new(),
            cow: quote! { CowStr },
            smol: quote! { SmolStr },
            btree: quote! { BTreeMap },
            vec: quote! { Vec },
            mime: quote! { MimeType },
        }
    }

    /// Generate scoped `use` imports for the top of a generated function body.
    /// Only meaningful in short mode — returns empty in qualified mode.
    pub(crate) fn scoped_imports(&self) -> TokenStream {
        if self.lex.is_empty() {
            quote! {
                #[allow(unused_imports)]
                use jacquard_common::{CowStr, deps::smol_str::SmolStr, types::blob::MimeType};
                use jacquard_lexicon::lexicon::*;
                use alloc::collections::BTreeMap;
            }
        } else {
            TokenStream::new()
        }
    }
}

/// Convert LexiconDoc to TokenStream for compile-time codegen.
/// union_fields maps property names to type paths for runtime union access.
pub fn doc_to_tokens(doc: &LexiconDoc, union_fields: &BTreeMap<String, String>) -> TokenStream {
    doc_to_tokens_with_paths(doc, union_fields, &DocPaths::qualified())
}

/// Convert LexiconDoc to TokenStream with configurable path style.
pub(crate) fn doc_to_tokens_with_paths(
    doc: &LexiconDoc,
    union_fields: &BTreeMap<String, String>,
    p: &DocPaths,
) -> TokenStream {
    let id = doc.id.as_ref();
    let defs_tokens = defs_map_to_tokens(&doc.defs, union_fields, p);
    let lex = &p.lex;
    let cow = &p.cow;

    quote! {
        #lex LexiconDoc {
            lexicon: #lex Lexicon::Lexicon1,
            id: #cow::new_static(#id),
            defs: #defs_tokens,
            ..Default::default()
        }
    }
}

/// Convert defs BTreeMap to tokens.
fn defs_map_to_tokens(
    defs: &BTreeMap<SmolStr, LexUserType>,
    union_fields: &BTreeMap<String, String>,
    p: &DocPaths,
) -> TokenStream {
    let smol = &p.smol;
    let btree = &p.btree;
    let def_entries: Vec<_> = defs
        .iter()
        .map(|(name, def)| {
            let name_str = name.as_str();
            let def_tokens = user_type_to_tokens(def, union_fields, p);
            quote! { map.insert(#smol::new_static(#name_str), #def_tokens) }
        })
        .collect();

    quote! {
        {
            let mut map = #btree::new();
            #(#def_entries;)*
            map
        }
    }
}

/// Convert LexUserType to tokens.
fn user_type_to_tokens(
    ut: &LexUserType,
    union_fields: &BTreeMap<String, String>,
    p: &DocPaths,
) -> TokenStream {
    let lex = &p.lex;
    let cow = &p.cow;
    match ut {
        LexUserType::Record(rec) => {
            let description = field_cow_str("description", &rec.description, p);
            let key = field_cow_str("key", &rec.key, p);
            let record_tokens = match &rec.record {
                LexRecordRecord::Object(obj) => {
                    let obj_tokens = object_to_tokens(obj, union_fields, p);
                    quote! { #lex LexRecordRecord::Object(#obj_tokens) }
                }
            };
            quote! {
                #lex LexUserType::Record(
                    #lex LexRecord {
                        #description
                        #key
                        record: #record_tokens,
                        ..Default::default()
                    }
                )
            }
        }
        LexUserType::XrpcQuery(query) => {
            let params = option_to_tokens(&query.parameters, |qp| match qp {
                LexXrpcQueryParameter::Params(params) => {
                    let params_tokens = xrpc_parameters_to_tokens(params, p);
                    quote! { #lex LexXrpcQueryParameter::Params(#params_tokens) }
                }
            });
            quote! {
                #lex LexUserType::XrpcQuery(
                    #lex LexXrpcQuery {
                        parameters: #params,
                        ..Default::default()
                    }
                )
            }
        }
        LexUserType::XrpcProcedure(proc) => {
            let input = option_to_tokens(&proc.input, |body| {
                xrpc_body_to_tokens(body, union_fields, p)
            });
            quote! {
                #lex LexUserType::XrpcProcedure(
                    #lex LexXrpcProcedure {
                        input: #input,
                        ..Default::default()
                    }
                )
            }
        }
        LexUserType::XrpcSubscription(sub) => {
            let params = option_to_tokens(&sub.parameters, |sp| match sp {
                LexXrpcSubscriptionParameter::Params(params) => {
                    let params_tokens = xrpc_parameters_to_tokens(params, p);
                    quote! { #lex LexXrpcSubscriptionParameter::Params(#params_tokens) }
                }
            });
            quote! {
                #lex LexUserType::XrpcSubscription(
                    #lex LexXrpcSubscription {
                        parameters: #params,
                        ..Default::default()
                    }
                )
            }
        }
        LexUserType::Object(obj) => {
            let obj_tokens = object_to_tokens(obj, union_fields, p);
            quote! { #lex LexUserType::Object(#obj_tokens) }
        }
        LexUserType::Union(union) => {
            let refs: Vec<_> = union.refs.iter().map(|r| r.as_ref()).collect();
            let closed = field_option("closed", &union.closed, |c| quote! { #c });
            quote! {
                #lex LexUserType::Union(
                    #lex LexRefUnion {
                        refs: vec![#(#cow::new_static(#refs)),*],
                        #closed
                        ..Default::default()
                    }
                )
            }
        }
        LexUserType::Blob(blob) => {
            let accept = field_vec_mime("accept", &blob.accept, p);
            let max_size = field_option("max_size", &blob.max_size, |v| quote! { #v });
            quote! {
                #lex LexUserType::Blob(
                    #lex LexBlob {
                        #accept
                        #max_size
                        ..Default::default()
                    }
                )
            }
        }
        LexUserType::Array(arr) => {
            let items = array_item_to_tokens(&arr.items, p);
            let min = field_option("min_length", &arr.min_length, |v| quote! { #v });
            let max = field_option("max_length", &arr.max_length, |v| quote! { #v });
            quote! {
                #lex LexUserType::Array(
                    #lex LexArray {
                        items: #items,
                        #min
                        #max
                        ..Default::default()
                    }
                )
            }
        }
        LexUserType::Token(_) => quote! {
            #lex LexUserType::Token(#lex LexToken { ..Default::default() })
        },
        LexUserType::Boolean(_) => quote! {
            #lex LexUserType::Boolean(#lex LexBoolean { ..Default::default() })
        },
        LexUserType::Integer(i) => {
            let min = field_option("minimum", &i.minimum, |v| quote! { #v });
            let max = field_option("maximum", &i.maximum, |v| quote! { #v });
            quote! {
                #lex LexUserType::Integer(
                    #lex LexInteger {
                        #min
                        #max
                        ..Default::default()
                    }
                )
            }
        }
        LexUserType::String(s) => {
            let string_tokens = lex_string_to_tokens(s, p);
            quote! { #lex LexUserType::String(#string_tokens) }
        }
        LexUserType::Bytes(b) => {
            let min = field_option("min_length", &b.min_length, |v| quote! { #v });
            let max = field_option("max_length", &b.max_length, |v| quote! { #v });
            quote! {
                #lex LexUserType::Bytes(
                    #lex LexBytes {
                        #max
                        #min
                        ..Default::default()
                    }
                )
            }
        }
        LexUserType::CidLink(_) => quote! {
            #lex LexUserType::CidLink(#lex LexCidLink { ..Default::default() })
        },
        LexUserType::Unknown(_) => quote! {
            #lex LexUserType::Unknown(#lex LexUnknown { ..Default::default() })
        },
        LexUserType::PermissionSet(_) => quote! {
            #lex LexUserType::PermissionSet(#lex LexPermissionSet {
                title: None,
                title_lang: None,
                detail: None,
                detail_lang: None,
                permissions: vec![],
            })
        },
    }
}

/// Convert LexObject to tokens.
fn object_to_tokens(
    obj: &LexObject,
    union_fields: &BTreeMap<String, String>,
    p: &DocPaths,
) -> TokenStream {
    let lex = &p.lex;
    let props = properties_to_tokens(&obj.properties, union_fields, p);
    let required = field_vec_smol("required", &obj.required, p);
    let description = field_cow_str("description", &obj.description, p);

    quote! {
        #lex LexObject {
            #description
            #required
            properties: #props,
            ..Default::default()
        }
    }
}

/// Convert properties map to tokens.
fn properties_to_tokens(
    props: &BTreeMap<SmolStr, LexObjectProperty>,
    union_fields: &BTreeMap<String, String>,
    p: &DocPaths,
) -> TokenStream {
    let smol = &p.smol;
    let btree = &p.btree;
    let prop_entries: Vec<_> = props
        .iter()
        .map(|(name, prop)| {
            let name_str = name.as_str();
            let union_type_path = union_fields.get(name.as_str());
            let prop_tokens = object_property_to_tokens(prop, union_type_path, p);
            quote! { map.insert(#smol::new_static(#name_str), #prop_tokens) }
        })
        .collect();

    quote! {
        {
            #[allow(unused_mut)]
            let mut map = #btree::new();
            #(#prop_entries;)*
            map
        }
    }
}

/// Convert LexObjectProperty to tokens.
/// If union_type_path is Some, this property is a union and should access Type::LEXICON_UNION_REFS at runtime.
fn object_property_to_tokens(
    prop: &LexObjectProperty,
    union_type_path: Option<&String>,
    p: &DocPaths,
) -> TokenStream {
    let lex = &p.lex;
    let cow = &p.cow;
    match prop {
        LexObjectProperty::Boolean(_) => quote! {
            #lex LexObjectProperty::Boolean(#lex LexBoolean { ..Default::default() })
        },
        LexObjectProperty::Integer(i) => {
            let min = field_option("minimum", &i.minimum, |v| quote! { #v });
            let max = field_option("maximum", &i.maximum, |v| quote! { #v });
            quote! {
                #lex LexObjectProperty::Integer(
                    #lex LexInteger {
                        #min
                        #max
                        ..Default::default()
                    }
                )
            }
        }
        LexObjectProperty::String(s) => {
            let string_tokens = lex_string_to_tokens(s, p);
            quote! { #lex LexObjectProperty::String(#string_tokens) }
        }
        LexObjectProperty::Bytes(b) => {
            let min = field_option("min_length", &b.min_length, |v| quote! { #v });
            let max = field_option("max_length", &b.max_length, |v| quote! { #v });
            quote! {
                #lex LexObjectProperty::Bytes(
                    #lex LexBytes {
                        #max
                        #min
                        ..Default::default()
                    }
                )
            }
        }
        LexObjectProperty::CidLink(_) => quote! {
            #lex LexObjectProperty::CidLink(#lex LexCidLink { ..Default::default() })
        },
        LexObjectProperty::Blob(_) => quote! {
            #lex LexObjectProperty::Blob(#lex LexBlob { ..Default::default() })
        },
        LexObjectProperty::Unknown(_) => quote! {
            #lex LexObjectProperty::Unknown(#lex LexUnknown { ..Default::default() })
        },
        LexObjectProperty::Array(arr) => {
            let description = field_cow_str("description", &arr.description, p);
            let items = array_item_to_tokens(&arr.items, p);
            let min = field_option("min_length", &arr.min_length, |v| quote! { #v });
            let max = field_option("max_length", &arr.max_length, |v| quote! { #v });
            quote! {
                #lex LexObjectProperty::Array(
                    #lex LexArray {
                        #description
                        items: #items,
                        #min
                        #max
                        ..Default::default()
                    }
                )
            }
        }
        LexObjectProperty::Ref(r) => {
            let ref_str = r.r#ref.as_ref();
            quote! {
                #lex LexObjectProperty::Ref(
                    #lex LexRef {
                        r#ref: #cow::new_static(#ref_str),
                        ..Default::default()
                    }
                )
            }
        }
        LexObjectProperty::Union(union) => {
            let description = field_cow_str("description", &union.description, p);
            let vec_path = &p.vec;

            if let Some(type_path) = union_type_path {
                let type_ident_str = extract_type_ident_from_path(type_path);
                let type_ident = syn::Ident::new(&type_ident_str, proc_macro2::Span::call_site());
                let closed = field_option("closed", &union.closed, |c| quote! { #c });

                quote! {
                    #lex LexObjectProperty::Union(
                        #lex LexRefUnion {
                            #description
                            refs: {
                                let mut vec: #vec_path<#cow<'static>> = #vec_path::new();
                                for s in #type_ident::LEXICON_UNION_REFS.iter().copied() {
                                    vec.push(#cow::new_static(s));
                                }
                                vec
                            },
                            #closed
                            ..Default::default()
                        }
                    )
                }
            } else {
                let refs: Vec<_> = union.refs.iter().map(|r| r.as_ref()).collect();
                let closed = field_option("closed", &union.closed, |c| quote! { #c });
                quote! {
                    #lex LexObjectProperty::Union(
                        #lex LexRefUnion {
                            #description
                            refs: vec![#(#cow::new_static(#refs)),*],
                            #closed
                            ..Default::default()
                        }
                    )
                }
            }
        }
        LexObjectProperty::Object(obj) => {
            let obj_tokens = object_to_tokens(obj, &BTreeMap::new(), p);
            quote! { #lex LexObjectProperty::Object(#obj_tokens) }
        }
    }
}

/// Extract type identifier from type path string (handle Option<Type<'a>> -> Type).
fn extract_type_ident_from_path(type_path: &str) -> String {
    let tokens: proc_macro2::TokenStream = type_path
        .parse()
        .unwrap_or_else(|_| type_path.replace(" ", "").parse().unwrap());

    if let Ok(ty) = syn::parse2::<syn::Type>(tokens.clone()) {
        return extract_base_type_ident(&ty);
    }

    tokens
        .into_iter()
        .find_map(|tt| {
            if let proc_macro2::TokenTree::Ident(ident) = tt {
                Some(ident.to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| type_path.to_string())
}

/// Extract the base type identifier from a syn::Type.
fn extract_base_type_ident(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(type_path) => {
            if let Some(segment) = type_path.path.segments.last() {
                if segment.ident == "Option" {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                            return extract_base_type_ident(inner_ty);
                        }
                    }
                }
                return segment.ident.to_string();
            }
            "Unknown".to_string()
        }
        _ => "Unknown".to_string(),
    }
}

/// Convert LexArrayItem to tokens.
fn array_item_to_tokens(item: &LexArrayItem, p: &DocPaths) -> TokenStream {
    let lex = &p.lex;
    let cow = &p.cow;
    match item {
        LexArrayItem::Boolean(_) => quote! {
            #lex LexArrayItem::Boolean(#lex LexBoolean { ..Default::default() })
        },
        LexArrayItem::Integer(_) => quote! {
            #lex LexArrayItem::Integer(#lex LexInteger { ..Default::default() })
        },
        LexArrayItem::String(s) => {
            let string_tokens = lex_string_to_tokens(s, p);
            quote! { #lex LexArrayItem::String(#string_tokens) }
        }
        LexArrayItem::Bytes(_) => quote! {
            #lex LexArrayItem::Bytes(#lex LexBytes { ..Default::default() })
        },
        LexArrayItem::CidLink(_) => quote! {
            #lex LexArrayItem::CidLink(#lex LexCidLink { ..Default::default() })
        },
        LexArrayItem::Blob(_) => quote! {
            #lex LexArrayItem::Blob(#lex LexBlob { ..Default::default() })
        },
        LexArrayItem::Unknown(_) => quote! {
            #lex LexArrayItem::Unknown(#lex LexUnknown { ..Default::default() })
        },
        LexArrayItem::Ref(r) => {
            let ref_str = r.r#ref.as_ref();
            quote! {
                #lex LexArrayItem::Ref(
                    #lex LexRef {
                        r#ref: #cow::new_static(#ref_str),
                        ..Default::default()
                    }
                )
            }
        }
        LexArrayItem::Object(obj) => {
            let obj_tokens = object_to_tokens(obj, &BTreeMap::new(), p);
            quote! { #lex LexArrayItem::Object(#obj_tokens) }
        }
        LexArrayItem::Union(union) => {
            let refs: Vec<_> = union.refs.iter().map(|r| r.as_ref()).collect();
            let closed = field_option("closed", &union.closed, |c| quote! { #c });
            quote! {
                #lex LexArrayItem::Union(
                    #lex LexRefUnion {
                        refs: vec![#(#cow::new_static(#refs)),*],
                        #closed
                        ..Default::default()
                    }
                )
            }
        }
    }
}

/// Convert LexString to tokens.
fn lex_string_to_tokens(s: &LexString, p: &DocPaths) -> TokenStream {
    let lex = &p.lex;
    let description = field_cow_str("description", &s.description, p);
    let format = field_option("format", &s.format, |f| match f {
        LexStringFormat::Did => quote! { #lex LexStringFormat::Did },
        LexStringFormat::Handle => quote! { #lex LexStringFormat::Handle },
        LexStringFormat::AtUri => quote! { #lex LexStringFormat::AtUri },
        LexStringFormat::Nsid => quote! { #lex LexStringFormat::Nsid },
        LexStringFormat::Cid => quote! { #lex LexStringFormat::Cid },
        LexStringFormat::Datetime => quote! { #lex LexStringFormat::Datetime },
        LexStringFormat::Language => quote! { #lex LexStringFormat::Language },
        LexStringFormat::Tid => quote! { #lex LexStringFormat::Tid },
        LexStringFormat::RecordKey => quote! { #lex LexStringFormat::RecordKey },
        LexStringFormat::AtIdentifier => quote! { #lex LexStringFormat::AtIdentifier },
        LexStringFormat::Uri => quote! { #lex LexStringFormat::Uri },
    });
    let min_len = field_option("min_length", &s.min_length, |v| quote! { #v });
    let max_len = field_option("max_length", &s.max_length, |v| quote! { #v });
    let min_graph = field_option("min_graphemes", &s.min_graphemes, |v| quote! { #v });
    let max_graph = field_option("max_graphemes", &s.max_graphemes, |v| quote! { #v });

    quote! {
        #lex LexString {
            #description
            #format
            #min_len
            #max_len
            #min_graph
            #max_graph
            ..Default::default()
        }
    }
}

/// Convert LexXrpcParameters to tokens.
fn xrpc_parameters_to_tokens(params: &LexXrpcParameters, p: &DocPaths) -> TokenStream {
    let lex = &p.lex;
    let smol = &p.smol;
    let btree = &p.btree;
    let props: Vec<_> = params
        .properties
        .iter()
        .map(|(name, prop)| {
            let name_str = name.as_str();
            let prop_tokens = xrpc_param_property_to_tokens(prop, p);
            quote! { map.insert(#smol::new_static(#name_str), #prop_tokens) }
        })
        .collect();
    let required = field_vec_smol("required", &params.required, p);

    quote! {
        #lex LexXrpcParameters {
            #required
            properties: {
                #[allow(unused_mut)]
                let mut map = #btree::new();
                #(#props;)*
                map
            },
            ..Default::default()
        }
    }
}

/// Convert LexXrpcParametersProperty to tokens.
fn xrpc_param_property_to_tokens(prop: &LexXrpcParametersProperty, p: &DocPaths) -> TokenStream {
    let lex = &p.lex;
    match prop {
        LexXrpcParametersProperty::Boolean(_) => quote! {
            #lex LexXrpcParametersProperty::Boolean(#lex LexBoolean { ..Default::default() })
        },
        LexXrpcParametersProperty::Integer(_) => quote! {
            #lex LexXrpcParametersProperty::Integer(#lex LexInteger { ..Default::default() })
        },
        LexXrpcParametersProperty::String(s) => {
            let string_tokens = lex_string_to_tokens(s, p);
            quote! { #lex LexXrpcParametersProperty::String(#string_tokens) }
        }
        LexXrpcParametersProperty::Unknown(_) => quote! {
            #lex LexXrpcParametersProperty::Unknown(#lex LexUnknown { ..Default::default() })
        },
        LexXrpcParametersProperty::Array(arr) => {
            let items = match &arr.items {
                LexPrimitiveArrayItem::Boolean(_) => quote! {
                    #lex LexPrimitiveArrayItem::Boolean(#lex LexBoolean { ..Default::default() })
                },
                LexPrimitiveArrayItem::Integer(_) => quote! {
                    #lex LexPrimitiveArrayItem::Integer(#lex LexInteger { ..Default::default() })
                },
                LexPrimitiveArrayItem::String(s) => {
                    let string_tokens = lex_string_to_tokens(s, p);
                    quote! { #lex LexPrimitiveArrayItem::String(#string_tokens) }
                }
                LexPrimitiveArrayItem::Unknown(_) => quote! {
                    #lex LexPrimitiveArrayItem::Unknown(#lex LexUnknown { ..Default::default() })
                },
            };
            let min = field_option("min_length", &arr.min_length, |v| quote! { #v });
            let max = field_option("max_length", &arr.max_length, |v| quote! { #v });
            quote! {
                #lex LexXrpcParametersProperty::Array(
                    #lex LexPrimitiveArray {
                        items: #items,
                        #min
                        #max
                        ..Default::default()
                    }
                )
            }
        }
    }
}

/// Convert LexXrpcBody to tokens.
fn xrpc_body_to_tokens(
    body: &LexXrpcBody,
    union_fields: &BTreeMap<String, String>,
    p: &DocPaths,
) -> TokenStream {
    let lex = &p.lex;
    let cow = &p.cow;
    let encoding = body.encoding.as_ref();
    let schema = option_to_tokens(&body.schema, |s| match s {
        LexXrpcBodySchema::Object(obj) => {
            let obj_tokens = object_to_tokens(obj, union_fields, p);
            quote! { #lex LexXrpcBodySchema::Object(#obj_tokens) }
        }
        LexXrpcBodySchema::Ref(r) => {
            let ref_str = r.r#ref.as_ref();
            quote! {
                #lex LexXrpcBodySchema::Ref(
                    #lex LexRef {
                        r#ref: #cow::new_static(#ref_str),
                        ..Default::default()
                    }
                )
            }
        }
        LexXrpcBodySchema::Union(union) => {
            let refs: Vec<_> = union.refs.iter().map(|r| r.as_ref()).collect();
            let closed = field_option("closed", &union.closed, |c| quote! { #c });
            quote! {
                #lex LexXrpcBodySchema::Union(
                    #lex LexRefUnion {
                        refs: vec![#(#cow::new_static(#refs)),*],
                        #closed
                        ..Default::default()
                    }
                )
            }
        }
    });

    quote! {
        #lex LexXrpcBody {
            encoding: #cow::new_static(#encoding),
            schema: #schema,
            ..Default::default()
        }
    }
}

/// Convert validation checks to tokens using fully-qualified paths.
/// Used from derive macros where no import context is available.
pub fn validations_to_tokens(checks: &[ValidationCheck]) -> TokenStream {
    validations_to_tokens_resolved(checks, None)
}

/// Convert validation checks to tokens, optionally using short names from
/// `ResolvedImports`. When `resolved` is `None`, all paths are fully-qualified
/// (for derive macro context). When `Some`, paths are shortened via the import
/// system (for codegen Pretty mode).
pub fn validations_to_tokens_resolved(
    checks: &[ValidationCheck],
    resolved: Option<&crate::codegen::prettify::ResolvedImports>,
) -> TokenStream {
    use crate::codegen::prettify::ExternalImport;

    let constraint_error = match resolved {
        Some(r) => r.external_type_tokens(&ExternalImport::ConstraintError),
        None => quote! { ::jacquard_lexicon::validation::ConstraintError },
    };
    let validation_path = match resolved {
        Some(r) => r.external_type_tokens(&ExternalImport::ValidationPath),
        None => quote! { ::jacquard_lexicon::validation::ValidationPath },
    };
    let unicode_segmentation = match resolved {
        Some(r) => r.external_type_tokens(&ExternalImport::UnicodeSegmentation),
        None => {
            quote! { jacquard_common::deps::codegen::unicode_segmentation::UnicodeSegmentation }
        }
    };
    if checks.is_empty() {
        return quote! { Ok(()) };
    }

    let check_tokens: Vec<_> = checks
        .iter()
        .map(|check| {
            let field_ident = crate::codegen::utils::make_ident(&check.field_name);
            let field_name_literal =
                syn::LitStr::new(&check.field_name, proc_macro2::Span::call_site());

            let len_expr = if check.is_array {
                quote! { value.len() }
            } else {
                quote! { <str>::len(value.as_ref()) }
            };

            let inner_check = match &check.check {
                ConstraintCheck::MaxLength { max } => quote! {
                    #[allow(unused_comparisons)]
                    if #len_expr > #max {
                        return Err(#constraint_error::MaxLength {
                            path: #validation_path::from_field(#field_name_literal),
                            max: #max,
                            actual: #len_expr,
                        });
                    }
                },
                ConstraintCheck::MinLength { min } => quote! {
                    #[allow(unused_comparisons)]
                    if #len_expr < #min {
                        return Err(#constraint_error::MinLength {
                            path: #validation_path::from_field(#field_name_literal),
                            min: #min,
                            actual: #len_expr,
                        });
                    }
                },
                ConstraintCheck::MaxGraphemes { max } => quote! {
                    {
                        let count = #unicode_segmentation::graphemes(
                            value.as_ref(),
                            true
                        ).count();
                        if count > #max {
                            return Err(#constraint_error::MaxGraphemes {
                                path: #validation_path::from_field(#field_name_literal),
                                max: #max,
                                actual: count,
                            });
                        }
                    }
                },
                ConstraintCheck::MinGraphemes { min } => quote! {
                    {
                        let count = #unicode_segmentation::graphemes(
                            value.as_ref(),
                            true
                        ).count();
                        if count < #min {
                            return Err(#constraint_error::MinGraphemes {
                                path: #validation_path::from_field(#field_name_literal),
                                min: #min,
                                actual: count,
                            });
                        }
                    }
                },
                ConstraintCheck::Maximum { max } => quote! {
                    if *value > #max {
                        return Err(#constraint_error::Maximum {
                            path: #validation_path::from_field(#field_name_literal),
                            max: #max,
                            actual: *value,
                        });
                    }
                },
                ConstraintCheck::Minimum { min } => quote! {
                    if *value < #min {
                        return Err(#constraint_error::Minimum {
                            path: #validation_path::from_field(#field_name_literal),
                            min: #min,
                            actual: *value,
                        });
                    }
                },
                ConstraintCheck::BlobMaxSize { max } => quote! {
                    {
                        let size = value.blob().size;
                        if size > #max {
                            return Err(#constraint_error::BlobTooLarge {
                                path: #validation_path::from_field(#field_name_literal),
                                max: #max,
                                actual: size,
                            });
                        }
                    }
                },
                ConstraintCheck::BlobAccept { accept } => {
                    let accept_strs: Vec<_> = accept.iter().map(|s| s.as_str()).collect();
                    quote! {
                        {
                            let mime = value.blob().mime_type.as_str();
                            let accepted: &[&str] = &[#(#accept_strs),*];
                            let matched = accepted.iter().any(|pattern| {
                                if *pattern == "*/*" {
                                    true
                                } else if pattern.ends_with("/*") {
                                    let prefix = &pattern[..pattern.len() - 2];
                                    mime.starts_with(prefix) && mime.as_bytes().get(prefix.len()) == Some(&b'/')
                                } else {
                                    mime == *pattern
                                }
                            });
                            if !matched {
                                return Err(#constraint_error::BlobMimeTypeNotAccepted {
                                    path: #validation_path::from_field(#field_name_literal),
                                    accepted: vec![#(#accept_strs.to_string()),*],
                                    actual: mime.to_string(),
                                });
                            }
                        }
                    }
                },
            };

            if check.is_array_item && check.is_required {
                quote! {
                    for value in &self.#field_ident {
                        #inner_check
                    }
                }
            } else if check.is_array_item {
                quote! {
                    if let Some(values) = &self.#field_ident {
                        for value in values {
                            #inner_check
                        }
                    }
                }
            } else if check.is_required {
                quote! {
                    {
                        let value = &self.#field_ident;
                        #inner_check
                    }
                }
            } else {
                quote! {
                    if let Some(ref value) = self.#field_ident {
                        #inner_check
                    }
                }
            }
        })
        .collect();

    quote! {
        #(#check_tokens)*
        Ok(())
    }
}

// --- Helper functions ---

/// Emit `Some(value)` or `None` for an option. Used for enum variant wrappers
/// where the field assignment is handled by the caller.
fn option_to_tokens<T, F>(opt: &Option<T>, f: F) -> TokenStream
where
    F: FnOnce(&T) -> TokenStream,
{
    match opt {
        Some(v) => {
            let tokens = f(v);
            quote! { Some(#tokens) }
        }
        None => quote! { None },
    }
}

/// Emit `field_name: Some(value),` when present, or nothing when None.
/// Used for optional struct fields alongside `..Default::default()`.
fn field_option<T, F>(name: &str, opt: &Option<T>, f: F) -> TokenStream
where
    F: FnOnce(&T) -> TokenStream,
{
    match opt {
        Some(v) => {
            let ident = syn::Ident::new(name, proc_macro2::Span::call_site());
            let tokens = f(v);
            quote! { #ident: Some(#tokens), }
        }
        None => TokenStream::new(),
    }
}

/// Emit `field_name: Some(CowStr::new_static("...")),` when present, or nothing.
fn field_cow_str(name: &str, opt: &Option<jacquard_common::CowStr>, p: &DocPaths) -> TokenStream {
    let cow = &p.cow;
    match opt {
        Some(s) => {
            let ident = syn::Ident::new(name, proc_macro2::Span::call_site());
            let s_str = s.as_ref();
            quote! { #ident: Some(#cow::new_static(#s_str)), }
        }
        None => TokenStream::new(),
    }
}

/// Emit `field_name: Some(vec![SmolStr::new_static(...)]),` when present, or nothing.
fn field_vec_smol(name: &str, opt: &Option<Vec<SmolStr>>, p: &DocPaths) -> TokenStream {
    let smol = &p.smol;
    match opt {
        Some(v) => {
            let ident = syn::Ident::new(name, proc_macro2::Span::call_site());
            let strs: Vec<_> = v.iter().map(|s| s.as_str()).collect();
            quote! { #ident: Some(vec![#(#smol::new_static(#strs)),*]), }
        }
        None => TokenStream::new(),
    }
}

/// Emit `field_name: Some(vec![MimeType(CowStr::new_static(...))]),` when present, or nothing.
fn field_vec_mime<S: jacquard_common::Bos<str> + AsRef<str>>(
    name: &str,
    opt: &Option<Vec<jacquard_common::types::blob::MimeType<S>>>,
    p: &DocPaths,
) -> TokenStream {
    let cow = &p.cow;
    let mime = &p.mime;
    match opt {
        Some(v) => {
            let ident = syn::Ident::new(name, proc_macro2::Span::call_site());
            let mime_strs: Vec<&str> = v.iter().map(|m| m.as_str()).collect();
            quote! { #ident: Some(vec![#(#mime(#cow::new_static(#mime_strs))),*]), }
        }
        None => TokenStream::new(),
    }
}
