// Forked from atrium-codegen
// https://github.com/sugyan/atrium/blob/main/lexicon/atrium-codegen/src/schema.rs

use crate::lexicon::*;
use heck::ToPascalCase;
use jacquard_common::{
    CowStr, IntoStatic,
    smol_str::{self, SmolStr, ToSmolStr},
};
use std::collections::BTreeMap;

pub(crate) fn find_ref_unions<'s>(
    defs: &'s BTreeMap<SmolStr, LexUserType<'s>>,
) -> Vec<(SmolStr, LexRefUnion<'s>)> {
    let mut unions = Vec::new();
    for (key, def) in defs {
        match def {
            LexUserType::Record(record) => {
                let LexRecordRecord::Object(object) = &record.record;
                find_ref_unions_in_object(object, SmolStr::new_static("Record"), &mut unions);
            }
            LexUserType::XrpcQuery(query) => {
                if let Some(output) = &query.output {
                    if let Some(schema) = &output.schema {
                        find_ref_unions_in_body_schema(
                            schema,
                            SmolStr::new_static("Output"),
                            &mut unions,
                        );
                    }
                }
            }
            LexUserType::XrpcProcedure(procedure) => {
                if let Some(input) = &procedure.input {
                    if let Some(schema) = &input.schema {
                        find_ref_unions_in_body_schema(
                            schema,
                            SmolStr::new_static("Input"),
                            &mut unions,
                        );
                    }
                }
                if let Some(output) = &procedure.output {
                    if let Some(schema) = &output.schema {
                        find_ref_unions_in_body_schema(
                            schema,
                            SmolStr::new_static("Output"),
                            &mut unions,
                        );
                    }
                }
            }
            LexUserType::XrpcSubscription(subscription) => {
                if let Some(message) = &subscription.message {
                    if let Some(schema) = &message.schema {
                        find_ref_unions_in_subscription_message_schema(
                            schema,
                            SmolStr::new_static("Message"),
                            &mut unions,
                        );
                    }
                }
            }
            LexUserType::Array(array) => {
                find_ref_unions_in_array(
                    array,
                    CowStr::Borrowed(&key.to_pascal_case()).into_static(),
                    &mut unions,
                );
            }
            LexUserType::Object(object) => {
                find_ref_unions_in_object(object, key.to_pascal_case().to_smolstr(), &mut unions);
            }
            _ => {}
        }
    }
    unions.sort_by_cached_key(|(name, _)| name.clone());
    unions
}

fn find_ref_unions_in_body_schema<'s>(
    schema: &'s LexXrpcBodySchema,
    name: SmolStr,
    unions: &mut Vec<(SmolStr, LexRefUnion<'s>)>,
) {
    match schema {
        LexXrpcBodySchema::Union(_) => unimplemented!(),
        LexXrpcBodySchema::Object(object) => find_ref_unions_in_object(object, name, unions),
        _ => {}
    }
}

fn find_ref_unions_in_subscription_message_schema<'s>(
    schema: &'s LexXrpcSubscriptionMessageSchema,
    name: SmolStr,
    unions: &mut Vec<(SmolStr, LexRefUnion<'s>)>,
) {
    match schema {
        LexXrpcSubscriptionMessageSchema::Union(union) => {
            unions.push((name.into(), union.clone()));
        }
        LexXrpcSubscriptionMessageSchema::Object(object) => {
            find_ref_unions_in_object(object, name, unions)
        }
        _ => {}
    }
}

fn find_ref_unions_in_array<'s>(
    array: &'s LexArray,
    name: CowStr<'s>,
    unions: &mut Vec<(SmolStr, LexRefUnion<'s>)>,
) {
    if let LexArrayItem::Union(union) = &array.items {
        unions.push((smol_str::format_smolstr!("{}", name), union.clone()));
    }
}

fn find_ref_unions_in_object<'s>(
    object: &'s LexObject,
    name: SmolStr,
    unions: &mut Vec<(SmolStr, LexRefUnion<'s>)>,
) {
    for (k, property) in &object.properties {
        match property {
            LexObjectProperty::Union(union) => {
                unions.push((
                    smol_str::format_smolstr!("{name}{}", k.to_pascal_case()),
                    union.clone(),
                ));
            }
            LexObjectProperty::Array(array) => {
                find_ref_unions_in_array(
                    array,
                    CowStr::Borrowed(&(name.to_string() + &k.to_pascal_case())).into_static(),
                    unions,
                );
            }
            _ => {}
        }
    }
}
