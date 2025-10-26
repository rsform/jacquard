//! Serialize LexiconDoc values to TokenStream for macro codegen

use crate::lexicon::*;
use crate::schema::from_ast::{ConstraintCheck, ValidationCheck};
use jacquard_common::smol_str::SmolStr;
use proc_macro2::TokenStream;
use quote::quote;
use std::collections::BTreeMap;

/// Convert LexiconDoc to TokenStream for compile-time codegen
pub fn doc_to_tokens(doc: &LexiconDoc) -> TokenStream {
    let id = doc.id.as_ref();
    let defs_tokens = defs_map_to_tokens(&doc.defs);

    quote! {
        ::jacquard_lexicon::lexicon::LexiconDoc {
            lexicon: ::jacquard_lexicon::lexicon::Lexicon::Lexicon1,
            id: #id.into(),
            revision: None,
            description: None,
            defs: #defs_tokens,
        }
    }
}

/// Convert defs BTreeMap to tokens
fn defs_map_to_tokens(defs: &BTreeMap<SmolStr, LexUserType>) -> TokenStream {
    let def_entries: Vec<_> = defs
        .iter()
        .map(|(name, def)| {
            let name_str = name.as_str();
            let def_tokens = user_type_to_tokens(def);
            quote! { map.insert(#name_str.into(), #def_tokens) }
        })
        .collect();

    quote! {
        {
            let mut map = ::std::collections::BTreeMap::new();
            #(#def_entries;)*
            map
        }
    }
}

/// Convert LexUserType to tokens
fn user_type_to_tokens(ut: &LexUserType) -> TokenStream {
    match ut {
        LexUserType::Record(rec) => {
            let key = option_cow_str_to_tokens(&rec.key);
            let record_tokens = match &rec.record {
                LexRecordRecord::Object(obj) => {
                    let obj_tokens = object_to_tokens(obj);
                    quote! {
                        ::jacquard_lexicon::lexicon::LexRecordRecord::Object(#obj_tokens)
                    }
                }
            };
            quote! {
                ::jacquard_lexicon::lexicon::LexUserType::Record(
                    ::jacquard_lexicon::lexicon::LexRecord {
                        description: None,
                        key: #key,
                        record: #record_tokens,
                    }
                )
            }
        }
        LexUserType::XrpcQuery(query) => {
            let params = option_to_tokens(&query.parameters, |p| match p {
                LexXrpcQueryParameter::Params(params) => {
                    let params_tokens = xrpc_parameters_to_tokens(params);
                    quote! {
                        ::jacquard_lexicon::lexicon::LexXrpcQueryParameter::Params(#params_tokens)
                    }
                }
            });
            quote! {
                ::jacquard_lexicon::lexicon::LexUserType::XrpcQuery(
                    ::jacquard_lexicon::lexicon::LexXrpcQuery {
                        description: None,
                        parameters: #params,
                        output: None,
                        errors: None,
                    }
                )
            }
        }
        LexUserType::XrpcProcedure(proc) => {
            let input = option_to_tokens(&proc.input, xrpc_body_to_tokens);
            quote! {
                ::jacquard_lexicon::lexicon::LexUserType::XrpcProcedure(
                    ::jacquard_lexicon::lexicon::LexXrpcProcedure {
                        description: None,
                        parameters: None,
                        input: #input,
                        output: None,
                        errors: None,
                    }
                )
            }
        }
        LexUserType::XrpcSubscription(sub) => {
            let params = option_to_tokens(&sub.parameters, |p| match p {
                LexXrpcSubscriptionParameter::Params(params) => {
                    let params_tokens = xrpc_parameters_to_tokens(params);
                    quote! {
                        ::jacquard_lexicon::lexicon::LexXrpcSubscriptionParameter::Params(#params_tokens)
                    }
                }
            });
            quote! {
                ::jacquard_lexicon::lexicon::LexUserType::XrpcSubscription(
                    ::jacquard_lexicon::lexicon::LexXrpcSubscription {
                        description: None,
                        parameters: #params,
                        message: None,
                        infos: None,
                        errors: None,
                    }
                )
            }
        }
        LexUserType::Object(obj) => {
            let obj_tokens = object_to_tokens(obj);
            quote! {
                ::jacquard_lexicon::lexicon::LexUserType::Object(#obj_tokens)
            }
        }
        LexUserType::Union(union) => {
            let refs: Vec<_> = union.refs.iter().map(|r| r.as_ref()).collect();
            let closed = option_to_tokens(&union.closed, |c| quote! { #c });
            quote! {
                ::jacquard_lexicon::lexicon::LexUserType::Union(
                    ::jacquard_lexicon::lexicon::LexRefUnion {
                        description: None,
                        refs: vec![#(#refs.into()),*],
                        closed: #closed,
                    }
                )
            }
        }
        LexUserType::Blob(blob) => {
            let accept = option_vec_mime_type_to_tokens(&blob.accept);
            let max_size = option_to_tokens(&blob.max_size, |v| quote! { #v });
            quote! {
                ::jacquard_lexicon::lexicon::LexUserType::Blob(
                    ::jacquard_lexicon::lexicon::LexBlob {
                        description: None,
                        accept: #accept,
                        max_size: #max_size,
                    }
                )
            }
        }
        LexUserType::Array(arr) => {
            let items = array_item_to_tokens(&arr.items);
            let min = option_to_tokens(&arr.min_length, |v| quote! { #v });
            let max = option_to_tokens(&arr.max_length, |v| quote! { #v });
            quote! {
                ::jacquard_lexicon::lexicon::LexUserType::Array(
                    ::jacquard_lexicon::lexicon::LexArray {
                        description: None,
                        items: #items,
                        min_length: #min,
                        max_length: #max,
                    }
                )
            }
        }
        LexUserType::Token(_) => quote! {
            ::jacquard_lexicon::lexicon::LexUserType::Token(
                ::jacquard_lexicon::lexicon::LexToken { description: None }
            )
        },
        LexUserType::Boolean(_) => quote! {
            ::jacquard_lexicon::lexicon::LexUserType::Boolean(
                ::jacquard_lexicon::lexicon::LexBoolean {
                    description: None,
                    default: None,
                    r#const: None,
                }
            )
        },
        LexUserType::Integer(i) => {
            let min = option_to_tokens(&i.minimum, |v| quote! { #v });
            let max = option_to_tokens(&i.maximum, |v| quote! { #v });
            quote! {
                ::jacquard_lexicon::lexicon::LexUserType::Integer(
                    ::jacquard_lexicon::lexicon::LexInteger {
                        description: None,
                        default: None,
                        minimum: #min,
                        maximum: #max,
                        r#enum: None,
                        r#const: None,
                    }
                )
            }
        }
        LexUserType::String(s) => {
            let string_tokens = lex_string_to_tokens(s);
            quote! {
                ::jacquard_lexicon::lexicon::LexUserType::String(#string_tokens)
            }
        }
        LexUserType::Bytes(b) => {
            let min = option_to_tokens(&b.min_length, |v| quote! { #v });
            let max = option_to_tokens(&b.max_length, |v| quote! { #v });
            quote! {
                ::jacquard_lexicon::lexicon::LexUserType::Bytes(
                    ::jacquard_lexicon::lexicon::LexBytes {
                        description: None,
                        max_length: #max,
                        min_length: #min,
                    }
                )
            }
        }
        LexUserType::CidLink(_) => quote! {
            ::jacquard_lexicon::lexicon::LexUserType::CidLink(
                ::jacquard_lexicon::lexicon::LexCidLink { description: None }
            )
        },
        LexUserType::Unknown(_) => quote! {
            ::jacquard_lexicon::lexicon::LexUserType::Unknown(
                ::jacquard_lexicon::lexicon::LexUnknown { description: None }
            )
        },
    }
}

/// Convert LexObject to tokens
fn object_to_tokens(obj: &LexObject) -> TokenStream {
    let props = properties_to_tokens(&obj.properties);
    let required = option_vec_smol_str_to_tokens(&obj.required);

    quote! {
        ::jacquard_lexicon::lexicon::LexObject {
            description: None,
            required: #required,
            nullable: None,
            properties: #props,
        }
    }
}

/// Convert properties map to tokens
fn properties_to_tokens(props: &BTreeMap<SmolStr, LexObjectProperty>) -> TokenStream {
    let prop_entries: Vec<_> = props
        .iter()
        .map(|(name, prop)| {
            let name_str = name.as_str();
            let prop_tokens = object_property_to_tokens(prop);
            quote! { map.insert(#name_str.into(), #prop_tokens) }
        })
        .collect();

    quote! {
        {
            #[allow(unused_mut)]
            let mut map = ::std::collections::BTreeMap::new();
            #(#prop_entries;)*
            map
        }
    }
}

/// Convert LexObjectProperty to tokens
fn object_property_to_tokens(prop: &LexObjectProperty) -> TokenStream {
    match prop {
        LexObjectProperty::Boolean(_) => quote! {
            ::jacquard_lexicon::lexicon::LexObjectProperty::Boolean(
                ::jacquard_lexicon::lexicon::LexBoolean {
                    description: None,
                    default: None,
                    r#const: None,
                }
            )
        },
        LexObjectProperty::Integer(i) => {
            let min = option_to_tokens(&i.minimum, |v| quote! { #v });
            let max = option_to_tokens(&i.maximum, |v| quote! { #v });
            quote! {
                ::jacquard_lexicon::lexicon::LexObjectProperty::Integer(
                    ::jacquard_lexicon::lexicon::LexInteger {
                        description: None,
                        default: None,
                        minimum: #min,
                        maximum: #max,
                        r#enum: None,
                        r#const: None,
                    }
                )
            }
        }
        LexObjectProperty::String(s) => {
            let string_tokens = lex_string_to_tokens(s);
            quote! {
                ::jacquard_lexicon::lexicon::LexObjectProperty::String(#string_tokens)
            }
        }
        LexObjectProperty::Bytes(b) => {
            let min = option_to_tokens(&b.min_length, |v| quote! { #v });
            let max = option_to_tokens(&b.max_length, |v| quote! { #v });
            quote! {
                ::jacquard_lexicon::lexicon::LexObjectProperty::Bytes(
                    ::jacquard_lexicon::lexicon::LexBytes {
                        description: None,
                        max_length: #max,
                        min_length: #min,
                    }
                )
            }
        }
        LexObjectProperty::CidLink(_) => quote! {
            ::jacquard_lexicon::lexicon::LexObjectProperty::CidLink(
                ::jacquard_lexicon::lexicon::LexCidLink { description: None }
            )
        },
        LexObjectProperty::Blob(_) => quote! {
            ::jacquard_lexicon::lexicon::LexObjectProperty::Blob(
                ::jacquard_lexicon::lexicon::LexBlob {
                    description: None,
                    accept: None,
                    max_size: None,
                }
            )
        },
        LexObjectProperty::Unknown(_) => quote! {
            ::jacquard_lexicon::lexicon::LexObjectProperty::Unknown(
                ::jacquard_lexicon::lexicon::LexUnknown { description: None }
            )
        },
        LexObjectProperty::Array(arr) => {
            let items = array_item_to_tokens(&arr.items);
            let min = option_to_tokens(&arr.min_length, |v| quote! { #v });
            let max = option_to_tokens(&arr.max_length, |v| quote! { #v });
            quote! {
                ::jacquard_lexicon::lexicon::LexObjectProperty::Array(
                    ::jacquard_lexicon::lexicon::LexArray {
                        description: None,
                        items: #items,
                        min_length: #min,
                        max_length: #max,
                    }
                )
            }
        }
        LexObjectProperty::Ref(r) => {
            let ref_str = r.r#ref.as_ref();
            quote! {
                ::jacquard_lexicon::lexicon::LexObjectProperty::Ref(
                    ::jacquard_lexicon::lexicon::LexRef {
                        description: None,
                        r#ref: #ref_str.into(),
                    }
                )
            }
        }
        LexObjectProperty::Union(union) => {
            let refs: Vec<_> = union.refs.iter().map(|r| r.as_ref()).collect();
            let closed = option_to_tokens(&union.closed, |c| quote! { #c });
            quote! {
                ::jacquard_lexicon::lexicon::LexObjectProperty::Union(
                    ::jacquard_lexicon::lexicon::LexRefUnion {
                        description: None,
                        refs: vec![#(#refs.into()),*],
                        closed: #closed,
                    }
                )
            }
        }
        LexObjectProperty::Object(obj) => {
            let obj_tokens = object_to_tokens(obj);
            quote! {
                ::jacquard_lexicon::lexicon::LexObjectProperty::Object(#obj_tokens)
            }
        }
    }
}

/// Convert LexArrayItem to tokens
fn array_item_to_tokens(item: &LexArrayItem) -> TokenStream {
    match item {
        LexArrayItem::Boolean(_) => quote! {
            ::jacquard_lexicon::lexicon::LexArrayItem::Boolean(
                ::jacquard_lexicon::lexicon::LexBoolean {
                    description: None,
                    default: None,
                    r#const: None,
                }
            )
        },
        LexArrayItem::Integer(_) => quote! {
            ::jacquard_lexicon::lexicon::LexArrayItem::Integer(
                ::jacquard_lexicon::lexicon::LexInteger {
                    description: None,
                    default: None,
                    minimum: None,
                    maximum: None,
                    r#enum: None,
                    r#const: None,
                }
            )
        },
        LexArrayItem::String(s) => {
            let string_tokens = lex_string_to_tokens(s);
            quote! {
                ::jacquard_lexicon::lexicon::LexArrayItem::String(#string_tokens)
            }
        }
        LexArrayItem::Bytes(_) => quote! {
            ::jacquard_lexicon::lexicon::LexArrayItem::Bytes(
                ::jacquard_lexicon::lexicon::LexBytes {
                    description: None,
                    max_length: None,
                    min_length: None,
                }
            )
        },
        LexArrayItem::CidLink(_) => quote! {
            ::jacquard_lexicon::lexicon::LexArrayItem::CidLink(
                ::jacquard_lexicon::lexicon::LexCidLink { description: None }
            )
        },
        LexArrayItem::Blob(_) => quote! {
            ::jacquard_lexicon::lexicon::LexArrayItem::Blob(
                ::jacquard_lexicon::lexicon::LexBlob {
                    description: None,
                    accept: None,
                    max_size: None,
                }
            )
        },
        LexArrayItem::Unknown(_) => quote! {
            ::jacquard_lexicon::lexicon::LexArrayItem::Unknown(
                ::jacquard_lexicon::lexicon::LexUnknown { description: None }
            )
        },
        LexArrayItem::Ref(r) => {
            let ref_str = r.r#ref.as_ref();
            quote! {
                ::jacquard_lexicon::lexicon::LexArrayItem::Ref(
                    ::jacquard_lexicon::lexicon::LexRef {
                        description: None,
                        r#ref: #ref_str.into(),
                    }
                )
            }
        }
        LexArrayItem::Object(obj) => {
            let obj_tokens = object_to_tokens(obj);
            quote! {
                ::jacquard_lexicon::lexicon::LexArrayItem::Object(#obj_tokens)
            }
        }
        LexArrayItem::Union(union) => {
            let refs: Vec<_> = union.refs.iter().map(|r| r.as_ref()).collect();
            let closed = option_to_tokens(&union.closed, |c| quote! { #c });
            quote! {
                ::jacquard_lexicon::lexicon::LexArrayItem::Union(
                    ::jacquard_lexicon::lexicon::LexRefUnion {
                        description: None,
                        refs: vec![#(#refs.into()),*],
                        closed: #closed,
                    }
                )
            }
        }
    }
}

/// Convert LexString to tokens
fn lex_string_to_tokens(s: &LexString) -> TokenStream {
    let format = option_to_tokens(&s.format, |f| match f {
        LexStringFormat::Did => quote! { ::jacquard_lexicon::lexicon::LexStringFormat::Did },
        LexStringFormat::Handle => quote! { ::jacquard_lexicon::lexicon::LexStringFormat::Handle },
        LexStringFormat::AtUri => quote! { ::jacquard_lexicon::lexicon::LexStringFormat::AtUri },
        LexStringFormat::Nsid => quote! { ::jacquard_lexicon::lexicon::LexStringFormat::Nsid },
        LexStringFormat::Cid => quote! { ::jacquard_lexicon::lexicon::LexStringFormat::Cid },
        LexStringFormat::Datetime => {
            quote! { ::jacquard_lexicon::lexicon::LexStringFormat::Datetime }
        }
        LexStringFormat::Language => {
            quote! { ::jacquard_lexicon::lexicon::LexStringFormat::Language }
        }
        LexStringFormat::Tid => quote! { ::jacquard_lexicon::lexicon::LexStringFormat::Tid },
        LexStringFormat::RecordKey => {
            quote! { ::jacquard_lexicon::lexicon::LexStringFormat::RecordKey }
        }
        LexStringFormat::AtIdentifier => {
            quote! { ::jacquard_lexicon::lexicon::LexStringFormat::AtIdentifier }
        }
        LexStringFormat::Uri => quote! { ::jacquard_lexicon::lexicon::LexStringFormat::Uri },
    });
    let min_len = option_to_tokens(&s.min_length, |v| quote! { #v });
    let max_len = option_to_tokens(&s.max_length, |v| quote! { #v });
    let min_graph = option_to_tokens(&s.min_graphemes, |v| quote! { #v });
    let max_graph = option_to_tokens(&s.max_graphemes, |v| quote! { #v });

    quote! {
        ::jacquard_lexicon::lexicon::LexString {
            description: None,
            format: #format,
            default: None,
            min_length: #min_len,
            max_length: #max_len,
            min_graphemes: #min_graph,
            max_graphemes: #max_graph,
            r#enum: None,
            r#const: None,
            known_values: None,
        }
    }
}

/// Convert LexXrpcParameters to tokens
fn xrpc_parameters_to_tokens(params: &LexXrpcParameters) -> TokenStream {
    let props: Vec<_> = params
        .properties
        .iter()
        .map(|(name, prop)| {
            let name_str = name.as_str();
            let prop_tokens = xrpc_param_property_to_tokens(prop);
            quote! { map.insert(#name_str.into(), #prop_tokens) }
        })
        .collect();
    let required = option_vec_smol_str_to_tokens(&params.required);

    quote! {
        ::jacquard_lexicon::lexicon::LexXrpcParameters {
            description: None,
            required: #required,
            properties: {
                #[allow(unused_mut)]
                let mut map = ::std::collections::BTreeMap::new();
                #(#props;)*
                map
            },
        }
    }
}

/// Convert LexXrpcParametersProperty to tokens
fn xrpc_param_property_to_tokens(prop: &LexXrpcParametersProperty) -> TokenStream {
    match prop {
        LexXrpcParametersProperty::Boolean(_) => quote! {
            ::jacquard_lexicon::lexicon::LexXrpcParametersProperty::Boolean(
                ::jacquard_lexicon::lexicon::LexBoolean {
                    description: None,
                    default: None,
                    r#const: None,
                }
            )
        },
        LexXrpcParametersProperty::Integer(_) => quote! {
            ::jacquard_lexicon::lexicon::LexXrpcParametersProperty::Integer(
                ::jacquard_lexicon::lexicon::LexInteger {
                    description: None,
                    default: None,
                    minimum: None,
                    maximum: None,
                    r#enum: None,
                    r#const: None,
                }
            )
        },
        LexXrpcParametersProperty::String(s) => {
            let string_tokens = lex_string_to_tokens(s);
            quote! {
                ::jacquard_lexicon::lexicon::LexXrpcParametersProperty::String(#string_tokens)
            }
        }
        LexXrpcParametersProperty::Unknown(_) => quote! {
            ::jacquard_lexicon::lexicon::LexXrpcParametersProperty::Unknown(
                ::jacquard_lexicon::lexicon::LexUnknown { description: None }
            )
        },
        LexXrpcParametersProperty::Array(arr) => {
            let items = match &arr.items {
                LexPrimitiveArrayItem::Boolean(_) => quote! {
                    ::jacquard_lexicon::lexicon::LexPrimitiveArrayItem::Boolean(
                        ::jacquard_lexicon::lexicon::LexBoolean {
                            description: None,
                            default: None,
                            r#const: None,
                        }
                    )
                },
                LexPrimitiveArrayItem::Integer(_) => quote! {
                    ::jacquard_lexicon::lexicon::LexPrimitiveArrayItem::Integer(
                        ::jacquard_lexicon::lexicon::LexInteger {
                            description: None,
                            default: None,
                            minimum: None,
                            maximum: None,
                            r#enum: None,
                            r#const: None,
                        }
                    )
                },
                LexPrimitiveArrayItem::String(s) => {
                    let string_tokens = lex_string_to_tokens(s);
                    quote! {
                        ::jacquard_lexicon::lexicon::LexPrimitiveArrayItem::String(#string_tokens)
                    }
                }
                LexPrimitiveArrayItem::Unknown(_) => quote! {
                    ::jacquard_lexicon::lexicon::LexPrimitiveArrayItem::Unknown(
                        ::jacquard_lexicon::lexicon::LexUnknown { description: None }
                    )
                },
            };
            let min = option_to_tokens(&arr.min_length, |v| quote! { #v });
            let max = option_to_tokens(&arr.max_length, |v| quote! { #v });
            quote! {
                ::jacquard_lexicon::lexicon::LexXrpcParametersProperty::Array(
                    ::jacquard_lexicon::lexicon::LexPrimitiveArray {
                        description: None,
                        items: #items,
                        min_length: #min,
                        max_length: #max,
                    }
                )
            }
        }
    }
}

/// Convert LexXrpcBody to tokens
fn xrpc_body_to_tokens(body: &LexXrpcBody) -> TokenStream {
    let encoding = body.encoding.as_ref();
    let schema = option_to_tokens(&body.schema, |s| match s {
        LexXrpcBodySchema::Object(obj) => {
            let obj_tokens = object_to_tokens(obj);
            quote! {
                ::jacquard_lexicon::lexicon::LexXrpcBodySchema::Object(#obj_tokens)
            }
        }
        LexXrpcBodySchema::Ref(r) => {
            let ref_str = r.r#ref.as_ref();
            quote! {
                ::jacquard_lexicon::lexicon::LexXrpcBodySchema::Ref(
                    ::jacquard_lexicon::lexicon::LexRef {
                        description: None,
                        r#ref: #ref_str.into(),
                    }
                )
            }
        }
        LexXrpcBodySchema::Union(union) => {
            let refs: Vec<_> = union.refs.iter().map(|r| r.as_ref()).collect();
            let closed = option_to_tokens(&union.closed, |c| quote! { #c });
            quote! {
                ::jacquard_lexicon::lexicon::LexXrpcBodySchema::Union(
                    ::jacquard_lexicon::lexicon::LexRefUnion {
                        description: None,
                        refs: vec![#(#refs.into()),*],
                        closed: #closed,
                    }
                )
            }
        }
    });

    quote! {
        ::jacquard_lexicon::lexicon::LexXrpcBody {
            description: None,
            encoding: #encoding.into(),
            schema: #schema,
        }
    }
}

/// Convert validation checks to tokens
pub fn validations_to_tokens(checks: &[ValidationCheck]) -> TokenStream {
    if checks.is_empty() {
        return quote! { Ok(()) };
    }

    let check_tokens: Vec<_> = checks
        .iter()
        .map(|check| {
            // Use make_ident to handle keywords properly (adds r# prefix if needed)
            let field_ident = crate::codegen::utils::make_ident(&check.field_name);
            let field_name_static = &check.field_name;

            // Generate the inner validation check
            // For Vec types, use .len() directly; for strings/newtypes, use .as_ref().len()
            let is_vec = check.field_type.starts_with("Vec<");
            let len_expr = if is_vec {
                quote! { value.len() }
            } else {
                quote! { value.as_ref().len() }
            };

            let inner_check = match &check.check {
                ConstraintCheck::MaxLength { max } => quote! {
                    if #len_expr > #max {
                        return Err(::jacquard_lexicon::schema::ValidationError::MaxLength {
                            field: #field_name_static,
                            max: #max,
                            actual: #len_expr,
                        });
                    }
                },
                ConstraintCheck::MinLength { min } => quote! {
                    if #len_expr < #min {
                        return Err(::jacquard_lexicon::schema::ValidationError::MinLength {
                            field: #field_name_static,
                            min: #min,
                            actual: #len_expr,
                        });
                    }
                },
                ConstraintCheck::MaxGraphemes { max } => quote! {
                    {
                        let count = ::unicode_segmentation::UnicodeSegmentation::graphemes(
                            value.as_ref(),
                            true
                        ).count();
                        if count > #max {
                            return Err(::jacquard_lexicon::schema::ValidationError::MaxGraphemes {
                                field: #field_name_static,
                                max: #max,
                                actual: count,
                            });
                        }
                    }
                },
                ConstraintCheck::MinGraphemes { min } => quote! {
                    {
                        let count = ::unicode_segmentation::UnicodeSegmentation::graphemes(
                            value.as_ref(),
                            true
                        ).count();
                        if count < #min {
                            return Err(::jacquard_lexicon::schema::ValidationError::MinGraphemes {
                                field: #field_name_static,
                                min: #min,
                                actual: count,
                            });
                        }
                    }
                },
                ConstraintCheck::Maximum { max } => quote! {
                    if *value > #max {
                        return Err(::jacquard_lexicon::schema::ValidationError::Maximum {
                            field: #field_name_static,
                            max: #max,
                            actual: *value,
                        });
                    }
                },
                ConstraintCheck::Minimum { min } => quote! {
                    if *value < #min {
                        return Err(::jacquard_lexicon::schema::ValidationError::Minimum {
                            field: #field_name_static,
                            min: #min,
                            actual: *value,
                        });
                    }
                },
            };

            // Wrap in Option check if field is optional
            if check.is_required {
                // Required field: access directly
                quote! {
                    {
                        let value = &self.#field_ident;
                        #inner_check
                    }
                }
            } else {
                // Optional field: check if Some first
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

// Helper functions

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

fn option_cow_str_to_tokens(opt: &Option<jacquard_common::CowStr>) -> TokenStream {
    match opt {
        Some(s) => {
            let s_str = s.as_ref();
            quote! { Some(#s_str.into()) }
        }
        None => quote! { None },
    }
}

fn option_vec_smol_str_to_tokens(opt: &Option<Vec<SmolStr>>) -> TokenStream {
    match opt {
        Some(v) => {
            let strs: Vec<_> = v.iter().map(|s| s.as_str()).collect();
            quote! { Some(vec![#(#strs.into()),*]) }
        }
        None => quote! { None },
    }
}

fn option_vec_mime_type_to_tokens(
    opt: &Option<Vec<jacquard_common::types::blob::MimeType>>,
) -> TokenStream {
    match opt {
        Some(v) => {
            let mime_strs: Vec<_> = v.iter().map(|m| m.0.as_ref()).collect();
            quote! { Some(vec![#(jacquard_common::types::blob::MimeType(#mime_strs.into())),*]) }
        }
        None => quote! { None },
    }
}
