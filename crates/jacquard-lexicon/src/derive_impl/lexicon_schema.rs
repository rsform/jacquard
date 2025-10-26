//! Implementation of #[derive(LexiconSchema)] macro

use crate::lexicon::{
    LexArray, LexBlob, LexBoolean, LexBytes, LexCidLink, LexInteger, LexObject, LexObjectProperty,
    LexRef, LexRefUnion, LexString, LexStringFormat, LexUnknown, LexUserType,
};
use crate::schema::type_mapping::{LexiconPrimitiveType, StringFormat, rust_type_to_lexicon_type};
use heck::{ToKebabCase, ToLowerCamelCase, ToPascalCase, ToShoutySnakeCase, ToSnakeCase};
use jacquard_common::smol_str::{SmolStr, ToSmolStr};
use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{Attribute, Data, DeriveInput, Fields, Ident, LitStr, Type, parse2};

/// Implementation for the LexiconSchema derive macro
pub fn impl_derive_lexicon_schema(input: TokenStream) -> TokenStream {
    let input = match parse2::<DeriveInput>(input) {
        Ok(input) => input,
        Err(e) => return e.to_compile_error(),
    };

    match lexicon_schema_impl(&input) {
        Ok(tokens) => tokens,
        Err(e) => e.to_compile_error(),
    }
}

fn lexicon_schema_impl(input: &DeriveInput) -> syn::Result<TokenStream> {
    // Parse type-level attributes
    let type_attrs = parse_type_attrs(&input.attrs)?;

    // Determine NSID
    let nsid = determine_nsid(&type_attrs, input)?;

    // Generate based on data type
    match &input.data {
        Data::Struct(data_struct) => impl_for_struct(input, &type_attrs, &nsid, data_struct),
        Data::Enum(data_enum) => impl_for_enum(input, &type_attrs, &nsid, data_enum),
        Data::Union(_) => Err(syn::Error::new_spanned(
            input,
            "LexiconSchema cannot be derived for unions",
        )),
    }
}

/// Parsed lexicon attributes from type
#[derive(Debug, Default)]
struct LexiconTypeAttrs {
    /// NSID for this type (required for primary types)
    nsid: Option<String>,

    /// Fragment name (None = not a fragment, Some("") = infer from type name)
    fragment: Option<String>,

    /// Type kind
    kind: Option<LexiconTypeKind>,

    /// Record key type (for records)
    key: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum LexiconTypeKind {
    Record,
    Query,
    Procedure,
    Subscription,
    Object,
    Union,
}

/// Parse type-level lexicon attributes
fn parse_type_attrs(attrs: &[Attribute]) -> syn::Result<LexiconTypeAttrs> {
    let mut result = LexiconTypeAttrs::default();

    for attr in attrs {
        if !attr.path().is_ident("lexicon") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("nsid") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                result.nsid = Some(lit.value());
                Ok(())
            } else if meta.path.is_ident("fragment") {
                // Two forms: #[lexicon(fragment)] or #[lexicon(fragment = "name")]
                if meta.input.peek(syn::Token![=]) {
                    let value = meta.value()?;
                    let lit: LitStr = value.parse()?;
                    result.fragment = Some(lit.value());
                } else {
                    result.fragment = Some(String::new()); // Infer from type name
                }
                Ok(())
            } else if meta.path.is_ident("record") {
                result.kind = Some(LexiconTypeKind::Record);
                Ok(())
            } else if meta.path.is_ident("query") {
                result.kind = Some(LexiconTypeKind::Query);
                Ok(())
            } else if meta.path.is_ident("procedure") {
                result.kind = Some(LexiconTypeKind::Procedure);
                Ok(())
            } else if meta.path.is_ident("subscription") {
                result.kind = Some(LexiconTypeKind::Subscription);
                Ok(())
            } else if meta.path.is_ident("key") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                result.key = Some(lit.value());
                Ok(())
            } else {
                Err(meta.error("unknown lexicon attribute"))
            }
        })?;
    }

    Ok(result)
}

/// Parsed lexicon attributes from field
#[derive(Debug, Default)]
struct LexiconFieldAttrs {
    max_length: Option<usize>,
    max_graphemes: Option<usize>,
    min_length: Option<usize>,
    min_graphemes: Option<usize>,
    minimum: Option<i64>,
    maximum: Option<i64>,
    explicit_ref: Option<String>,
    format: Option<String>,
}

/// Parse field-level lexicon attributes
fn parse_field_attrs(attrs: &[Attribute]) -> syn::Result<LexiconFieldAttrs> {
    let mut result = LexiconFieldAttrs::default();

    for attr in attrs {
        if !attr.path().is_ident("lexicon") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("max_length") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.max_length = Some(lit.base10_parse()?);
                Ok(())
            } else if meta.path.is_ident("max_graphemes") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.max_graphemes = Some(lit.base10_parse()?);
                Ok(())
            } else if meta.path.is_ident("min_length") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.min_length = Some(lit.base10_parse()?);
                Ok(())
            } else if meta.path.is_ident("min_graphemes") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.min_graphemes = Some(lit.base10_parse()?);
                Ok(())
            } else if meta.path.is_ident("minimum") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.minimum = Some(lit.base10_parse()?);
                Ok(())
            } else if meta.path.is_ident("maximum") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.maximum = Some(lit.base10_parse()?);
                Ok(())
            } else if meta.path.is_ident("ref") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                result.explicit_ref = Some(lit.value());
                Ok(())
            } else if meta.path.is_ident("format") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                result.format = Some(lit.value());
                Ok(())
            } else {
                Err(meta.error("unknown lexicon field attribute"))
            }
        })?;
    }

    Ok(result)
}

/// Parsed serde attributes relevant to lexicon schema
#[derive(Debug, Default)]
struct SerdeAttrs {
    rename: Option<String>,
    skip: bool,
}

/// Parse serde attributes for a field
fn parse_serde_attrs(attrs: &[Attribute]) -> syn::Result<SerdeAttrs> {
    let mut result = SerdeAttrs::default();

    for attr in attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                result.rename = Some(lit.value());
                Ok(())
            } else if meta.path.is_ident("skip") {
                result.skip = true;
                Ok(())
            } else {
                // Ignore other serde attributes
                Ok(())
            }
        })?;
    }

    Ok(result)
}

/// Parse container-level serde rename_all
fn parse_serde_rename_all(attrs: &[Attribute]) -> syn::Result<Option<RenameRule>> {
    for attr in attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }

        let mut found_rule = None;
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename_all") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                found_rule = RenameRule::from_str(&lit.value());
                Ok(())
            } else {
                Ok(())
            }
        })?;

        if found_rule.is_some() {
            return Ok(found_rule);
        }
    }

    // Default to camelCase (lexicon standard)
    Ok(Some(RenameRule::CamelCase))
}

#[derive(Debug, Clone, Copy)]
enum RenameRule {
    CamelCase,
    SnakeCase,
    PascalCase,
    ScreamingSnakeCase,
    KebabCase,
}

impl RenameRule {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "camelCase" => Some(RenameRule::CamelCase),
            "snake_case" => Some(RenameRule::SnakeCase),
            "PascalCase" => Some(RenameRule::PascalCase),
            "SCREAMING_SNAKE_CASE" => Some(RenameRule::ScreamingSnakeCase),
            "kebab-case" => Some(RenameRule::KebabCase),
            _ => None,
        }
    }

    fn apply(&self, input: &str) -> String {
        match self {
            RenameRule::CamelCase => input.to_lower_camel_case(),
            RenameRule::SnakeCase => input.to_snake_case(),
            RenameRule::PascalCase => input.to_pascal_case(),
            RenameRule::ScreamingSnakeCase => input.to_shouty_snake_case(),
            RenameRule::KebabCase => input.to_kebab_case(),
        }
    }
}

/// Determine NSID from attributes and context
fn determine_nsid(attrs: &LexiconTypeAttrs, input: &DeriveInput) -> syn::Result<String> {
    // Explicit NSID in lexicon attribute
    if let Some(nsid) = &attrs.nsid {
        return Ok(nsid.clone());
    }

    // Fragment - need to find module NSID (not implemented yet)
    if attrs.fragment.is_some() {
        return Err(syn::Error::new_spanned(
            input,
            "fragments require explicit nsid or module-level primary type (not yet implemented)",
        ));
    }

    // Check for XrpcRequest derive with NSID
    if let Some(nsid) = extract_xrpc_nsid(&input.attrs)? {
        return Ok(nsid);
    }

    Err(syn::Error::new_spanned(
        input,
        "missing required `nsid` attribute (use #[lexicon(nsid = \"...\")] or #[xrpc(nsid = \"...\")])",
    ))
}

/// Extract NSID from XrpcRequest attributes (cross-derive coordination)
fn extract_xrpc_nsid(attrs: &[Attribute]) -> syn::Result<Option<String>> {
    for attr in attrs {
        if !attr.path().is_ident("xrpc") {
            continue;
        }

        let mut nsid = None;
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("nsid") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                nsid = Some(lit.value());
            }
            Ok(())
        })?;

        if let Some(nsid) = nsid {
            return Ok(Some(nsid));
        }
    }
    Ok(None)
}

/// Struct implementation
fn impl_for_struct(
    input: &DeriveInput,
    type_attrs: &LexiconTypeAttrs,
    nsid: &str,
    data_struct: &syn::DataStruct,
) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let generics = &input.generics;

    // Detect lifetime
    let has_lifetime = generics.lifetimes().next().is_some();
    let lifetime = if has_lifetime {
        quote! { <'_> }
    } else {
        quote! {}
    };

    // Parse fields
    let fields = match &data_struct.fields {
        Fields::Named(fields) => &fields.named,
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "LexiconSchema only supports structs with named fields",
            ));
        }
    };

    // Parse serde container attributes (defaults to camelCase)
    let rename_all = parse_serde_rename_all(&input.attrs)?;

    // Generate field definitions
    let field_defs = generate_field_definitions(fields, rename_all)?;

    // Generate validation code
    let validation_code = generate_validation(fields, rename_all)?;

    // Build lexicon_doc() implementation
    let doc_impl = generate_doc_impl(nsid, type_attrs, &field_defs)?;

    // Determine schema_id (add fragment suffix if needed)
    let schema_id = if let Some(fragment) = &type_attrs.fragment {
        let frag_name = if fragment.is_empty() {
            // Infer from type name
            name.to_string().to_lower_camel_case()
        } else {
            fragment.clone()
        };
        quote! {
            format_smolstr!("{}#{}", #nsid, #frag_name).to_string()
        }
    } else {
        quote! {
            ::jacquard_common::CowStr::new_static(#nsid)
        }
    };

    // Generate trait impl
    Ok(quote! {
        impl #generics ::jacquard_lexicon::schema::LexiconSchema for #name #lifetime {
            fn nsid() -> &'static str {
                #nsid
            }

            fn schema_id() -> ::jacquard_common::CowStr<'static> {
                #schema_id
            }

            fn lexicon_doc(
                generator: &mut ::jacquard_lexicon::schema::LexiconGenerator
            ) -> ::jacquard_lexicon::lexicon::LexiconDoc<'static> {
                #doc_impl
            }

            fn validate(&self) -> ::std::result::Result<(), ::jacquard_lexicon::schema::ValidationError> {
                #validation_code
            }
        }

        // Generate inventory submission for Phase 3 discovery
        ::inventory::submit! {
            ::jacquard_lexicon::schema::LexiconSchemaRef {
                nsid: #nsid,
                provider: || {
                    let mut generator = ::jacquard_lexicon::schema::LexiconGenerator::new(#nsid);
                    #name::lexicon_doc(&mut generator)
                },
            }
        }
    })
}

struct FieldDef {
    name: String,          // Rust field name
    schema_name: String,   // JSON field name (after serde rename)
    rust_type: Type,       // Rust type
    lex_type: TokenStream, // LexObjectProperty tokens
    required: bool,
}

fn generate_field_definitions(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::Token![,]>,
    rename_all: Option<RenameRule>,
) -> syn::Result<Vec<FieldDef>> {
    let mut defs = Vec::new();

    for field in fields {
        let field_name = field.ident.as_ref().unwrap().to_string();

        // Skip extra_data field (added by #[lexicon] attribute macro)
        if field_name == "extra_data" {
            continue;
        }

        // Parse attributes
        let serde_attrs = parse_serde_attrs(&field.attrs)?;
        let lex_attrs = parse_field_attrs(&field.attrs)?;

        // Skip if serde(skip)
        if serde_attrs.skip {
            continue;
        }

        // Determine schema name
        let schema_name = if let Some(rename) = serde_attrs.rename {
            rename
        } else if let Some(rule) = rename_all {
            rule.apply(&field_name)
        } else {
            field_name.clone()
        };

        // Determine if required (Option<T> = optional)
        let (inner_type, required) = extract_option_inner(&field.ty);
        let rust_type = inner_type.clone();

        // Generate LexObjectProperty based on type + constraints
        let lex_type = generate_lex_property(&rust_type, &lex_attrs)?;

        defs.push(FieldDef {
            name: field_name,
            schema_name,
            rust_type,
            lex_type,
            required,
        });
    }

    Ok(defs)
}

/// Extract T from Option<T>, return (type, is_required)
fn extract_option_inner(ty: &Type) -> (&Type, bool) {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return (inner, false);
                    }
                }
            }
        }
    }
    (ty, true)
}

/// Generate LexObjectProperty tokens for a field
fn generate_lex_property(
    rust_type: &Type,
    constraints: &LexiconFieldAttrs,
) -> syn::Result<TokenStream> {
    // Try to detect primitive type
    let lex_type = rust_type_to_lexicon_type(rust_type);

    match lex_type {
        Some(LexiconPrimitiveType::Boolean) => Ok(quote! {
            ::jacquard_lexicon::lexicon::LexObjectProperty::Boolean(
                ::jacquard_lexicon::lexicon::LexBoolean {
                    description: None,
                    default: None,
                    r#const: None,
                }
            )
        }),
        Some(LexiconPrimitiveType::Integer) => {
            let minimum = constraints
                .minimum
                .map(|v| quote! { Some(#v) })
                .unwrap_or(quote! { None });
            let maximum = constraints
                .maximum
                .map(|v| quote! { Some(#v) })
                .unwrap_or(quote! { None });

            Ok(quote! {
                ::jacquard_lexicon::lexicon::LexObjectProperty::Integer(
                    ::jacquard_lexicon::lexicon::LexInteger {
                        description: None,
                        default: None,
                        minimum: #minimum,
                        maximum: #maximum,
                        r#enum: None,
                        r#const: None,
                    }
                )
            })
        }
        Some(LexiconPrimitiveType::String(format)) => generate_string_property(format, constraints),
        Some(LexiconPrimitiveType::Bytes) => {
            let max_length = constraints
                .max_length
                .map(|v| quote! { Some(#v) })
                .unwrap_or(quote! { None });
            let min_length = constraints
                .min_length
                .map(|v| quote! { Some(#v) })
                .unwrap_or(quote! { None });

            Ok(quote! {
                ::jacquard_lexicon::lexicon::LexObjectProperty::Bytes(
                    ::jacquard_lexicon::lexicon::LexBytes {
                        description: None,
                        max_length: #max_length,
                        min_length: #min_length,
                    }
                )
            })
        }
        Some(LexiconPrimitiveType::CidLink) => Ok(quote! {
            ::jacquard_lexicon::lexicon::LexObjectProperty::CidLink(
                ::jacquard_lexicon::lexicon::LexCidLink {
                    description: None,
                }
            )
        }),
        Some(LexiconPrimitiveType::Blob) => Ok(quote! {
            ::jacquard_lexicon::lexicon::LexObjectProperty::Blob(
                ::jacquard_lexicon::lexicon::LexBlob {
                    description: None,
                    accept: None,
                    max_size: None,
                }
            )
        }),
        Some(LexiconPrimitiveType::Unknown) => Ok(quote! {
            ::jacquard_lexicon::lexicon::LexObjectProperty::Unknown(
                ::jacquard_lexicon::lexicon::LexUnknown {
                    description: None,
                }
            )
        }),
        Some(LexiconPrimitiveType::Array(item_type)) => {
            let item_prop = generate_array_item(*item_type, constraints)?;
            let max_length = constraints
                .max_length
                .map(|v| quote! { Some(#v) })
                .unwrap_or(quote! { None });
            let min_length = constraints
                .min_length
                .map(|v| quote! { Some(#v) })
                .unwrap_or(quote! { None });

            Ok(quote! {
                ::jacquard_lexicon::lexicon::LexObjectProperty::Array(
                    ::jacquard_lexicon::lexicon::LexArray {
                        description: None,
                        items: #item_prop,
                        min_length: #min_length,
                        max_length: #max_length,
                    }
                )
            })
        }
        None => {
            // Not a recognized primitive - check for explicit ref or trait bound
            if let Some(ref_nsid) = &constraints.explicit_ref {
                Ok(quote! {
                    ::jacquard_lexicon::lexicon::LexObjectProperty::Ref(
                        ::jacquard_lexicon::lexicon::LexRef {
                            description: None,
                            r#ref: #ref_nsid.into(),
                        }
                    )
                })
            } else {
                // Try to use type's LexiconSchema impl
                Ok(quote! {
                    {
                        // Use the type's schema_id method
                        let ref_nsid = <#rust_type as ::jacquard_lexicon::schema::LexiconSchema>::schema_id();
                        ::jacquard_lexicon::lexicon::LexObjectProperty::Ref(
                            ::jacquard_lexicon::lexicon::LexRef {
                                description: None,
                                r#ref: ref_nsid.to_string().into(),
                            }
                        )
                    }
                })
            }
        }
        _ => Err(syn::Error::new_spanned(
            rust_type,
            "unsupported type for lexicon schema generation",
        )),
    }
}

fn generate_array_item(
    item_type: LexiconPrimitiveType,
    _constraints: &LexiconFieldAttrs,
) -> syn::Result<TokenStream> {
    match item_type {
        LexiconPrimitiveType::String(format) => {
            let format_token = string_format_token(format);
            Ok(quote! {
                ::jacquard_lexicon::lexicon::LexArrayItem::String(
                    ::jacquard_lexicon::lexicon::LexString {
                        description: None,
                        format: #format_token,
                        default: None,
                        min_length: None,
                        max_length: None,
                        min_graphemes: None,
                        max_graphemes: None,
                        r#enum: None,
                        r#const: None,
                        known_values: None,
                    }
                )
            })
        }
        LexiconPrimitiveType::Integer => Ok(quote! {
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
        }),
        _ => Ok(quote! {
            ::jacquard_lexicon::lexicon::LexArrayItem::Unknown(
                ::jacquard_lexicon::lexicon::LexUnknown {
                    description: None,
                }
            )
        }),
    }
}

fn generate_string_property(
    format: StringFormat,
    constraints: &LexiconFieldAttrs,
) -> syn::Result<TokenStream> {
    let format_token = string_format_token(format);

    let max_length = constraints
        .max_length
        .map(|v| quote! { Some(#v) })
        .unwrap_or(quote! { None });
    let max_graphemes = constraints
        .max_graphemes
        .map(|v| quote! { Some(#v) })
        .unwrap_or(quote! { None });
    let min_length = constraints
        .min_length
        .map(|v| quote! { Some(#v) })
        .unwrap_or(quote! { None });
    let min_graphemes = constraints
        .min_graphemes
        .map(|v| quote! { Some(#v) })
        .unwrap_or(quote! { None });

    Ok(quote! {
        ::jacquard_lexicon::lexicon::LexObjectProperty::String(
            ::jacquard_lexicon::lexicon::LexString {
                description: None,
                format: #format_token,
                default: None,
                min_length: #min_length,
                max_length: #max_length,
                min_graphemes: #min_graphemes,
                max_graphemes: #max_graphemes,
                r#enum: None,
                r#const: None,
                known_values: None,
            }
        )
    })
}

fn string_format_token(format: StringFormat) -> TokenStream {
    match format {
        StringFormat::Plain => quote! { None },
        StringFormat::Did => {
            quote! { Some(::jacquard_lexicon::lexicon::LexStringFormat::Did) }
        }
        StringFormat::Handle => {
            quote! { Some(::jacquard_lexicon::lexicon::LexStringFormat::Handle) }
        }
        StringFormat::AtUri => {
            quote! { Some(::jacquard_lexicon::lexicon::LexStringFormat::AtUri) }
        }
        StringFormat::Nsid => {
            quote! { Some(::jacquard_lexicon::lexicon::LexStringFormat::Nsid) }
        }
        StringFormat::Cid => {
            quote! { Some(::jacquard_lexicon::lexicon::LexStringFormat::Cid) }
        }
        StringFormat::Datetime => {
            quote! { Some(::jacquard_lexicon::lexicon::LexStringFormat::Datetime) }
        }
        StringFormat::Language => {
            quote! { Some(::jacquard_lexicon::lexicon::LexStringFormat::Language) }
        }
        StringFormat::Tid => {
            quote! { Some(::jacquard_lexicon::lexicon::LexStringFormat::Tid) }
        }
        StringFormat::RecordKey => {
            quote! { Some(::jacquard_lexicon::lexicon::LexStringFormat::RecordKey) }
        }
        StringFormat::AtIdentifier => {
            quote! { Some(::jacquard_lexicon::lexicon::LexStringFormat::AtIdentifier) }
        }
        StringFormat::Uri => {
            quote! { Some(::jacquard_lexicon::lexicon::LexStringFormat::Uri) }
        }
    }
}

fn generate_doc_impl(
    nsid: &str,
    type_attrs: &LexiconTypeAttrs,
    field_defs: &[FieldDef],
) -> syn::Result<TokenStream> {
    // Build properties map
    let properties: Vec<_> = field_defs
        .iter()
        .map(|def| {
            let name = &def.schema_name;
            let lex_type = &def.lex_type;
            quote! {
                (#name.into(), #lex_type)
            }
        })
        .collect();

    // Build required array
    let required: Vec<_> = field_defs
        .iter()
        .filter(|def| def.required)
        .map(|def| {
            let name = &def.schema_name;
            quote! { #name.into() }
        })
        .collect();

    let required_field = if required.is_empty() {
        quote! { None }
    } else {
        quote! { Some(vec![#(#required),*]) }
    };

    // Determine user type based on kind
    let user_type = match type_attrs.kind {
        Some(LexiconTypeKind::Record) => {
            let key = type_attrs
                .key
                .as_ref()
                .map(|k| quote! { Some(#k.into()) })
                .unwrap_or(quote! { None });

            quote! {
                ::jacquard_lexicon::lexicon::LexUserType::Record(
                    ::jacquard_lexicon::lexicon::LexRecord {
                        description: None,
                        key: #key,
                        record: ::jacquard_lexicon::lexicon::LexRecordRecord::Object(
                            ::jacquard_lexicon::lexicon::LexObject {
                                description: None,
                                required: #required_field,
                                nullable: None,
                                properties: [#(#properties),*].into(),
                            }
                        ),
                    }
                )
            }
        }
        Some(LexiconTypeKind::Query) => {
            quote! {
                ::jacquard_lexicon::lexicon::LexUserType::Query(
                    ::jacquard_lexicon::lexicon::LexQuery {
                        description: None,
                        parameters: Some(::jacquard_lexicon::lexicon::LexObject {
                            description: None,
                            required: #required_field,
                            nullable: None,
                            properties: [#(#properties),*].into(),
                        }),
                        output: None,
                        errors: None,
                    }
                )
            }
        }
        Some(LexiconTypeKind::Procedure) => {
            quote! {
                ::jacquard_lexicon::lexicon::LexUserType::Procedure(
                    ::jacquard_lexicon::lexicon::LexProcedure {
                        description: None,
                        input: Some(::jacquard_lexicon::lexicon::LexProcedureIO {
                            description: None,
                            encoding: "application/json".into(),
                            schema: Some(::jacquard_lexicon::lexicon::LexProcedureSchema::Object(
                                ::jacquard_lexicon::lexicon::LexObject {
                                    description: None,
                                    required: #required_field,
                                    nullable: None,
                                    properties: [#(#properties),*].into(),
                                }
                            )),
                        }),
                        output: None,
                        errors: None,
                    }
                )
            }
        }
        _ => {
            // Default: Object type
            quote! {
                ::jacquard_lexicon::lexicon::LexUserType::Object(
                    ::jacquard_lexicon::lexicon::LexObject {
                        description: None,
                        required: #required_field,
                        nullable: None,
                        properties: [#(#properties),*].into(),
                    }
                )
            }
        }
    };

    Ok(quote! {
        {
            let mut defs = ::std::collections::BTreeMap::new();
            defs.insert("main".into(), #user_type);

            ::jacquard_lexicon::lexicon::LexiconDoc {
                lexicon: ::jacquard_lexicon::lexicon::Lexicon::Lexicon1,
                id: #nsid.into(),
                revision: None,
                description: None,
                defs,
            }
        }
    })
}

fn generate_validation(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::Token![,]>,
    rename_all: Option<RenameRule>,
) -> syn::Result<TokenStream> {
    let mut checks = Vec::new();

    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        let field_name_str = field_name.to_string();

        // Skip extra_data
        if field_name_str == "extra_data" {
            continue;
        }

        let lex_attrs = parse_field_attrs(&field.attrs)?;
        let serde_attrs = parse_serde_attrs(&field.attrs)?;

        if serde_attrs.skip {
            continue;
        }

        // Get actual field name for errors
        let display_name = if let Some(rename) = serde_attrs.rename {
            rename
        } else if let Some(rule) = rename_all {
            rule.apply(&field_name_str)
        } else {
            field_name_str.clone()
        };

        // Extract inner type if Option
        let (inner_type, is_required) = extract_option_inner(&field.ty);

        // Generate checks based on type and constraints
        let field_checks = generate_field_validation(
            field_name,
            &display_name,
            inner_type,
            is_required,
            &lex_attrs,
        )?;

        checks.extend(field_checks);
    }

    if checks.is_empty() {
        Ok(quote! { Ok(()) })
    } else {
        Ok(quote! {
            let mut errors = Vec::new();

            #(#checks)*

            if errors.is_empty() {
                Ok(())
            } else if errors.len() == 1 {
                Err(errors.into_iter().next().unwrap())
            } else {
                Err(::jacquard_lexicon::schema::ValidationError::Multiple(errors))
            }
        })
    }
}

fn generate_field_validation(
    field_ident: &Ident,
    display_name: &str,
    field_type: &Type,
    is_required: bool,
    constraints: &LexiconFieldAttrs,
) -> syn::Result<Vec<TokenStream>> {
    let mut checks = Vec::new();

    // Determine base type
    let lex_type = rust_type_to_lexicon_type(field_type);

    // Build accessor for the field value
    let (value_binding, value_expr) = if is_required {
        (quote! { let value = &self.#field_ident; }, quote! { value })
    } else {
        (
            quote! {},
            quote! {
                match &self.#field_ident {
                    Some(v) => v,
                    None => continue,
                }
            },
        )
    };

    match lex_type {
        Some(LexiconPrimitiveType::String(_)) => {
            // String constraints
            if let Some(max_len) = constraints.max_length {
                checks.push(quote! {
                    #value_binding
                    if #value_expr.len() > #max_len {
                        errors.push(::jacquard_lexicon::schema::ValidationError::MaxLength {
                            field: #display_name,
                            max: #max_len,
                            actual: #value_expr.len(),
                        });
                    }
                });
            }

            if let Some(max_graphemes) = constraints.max_graphemes {
                checks.push(quote! {
                    #value_binding
                    let count = ::unicode_segmentation::UnicodeSegmentation::graphemes(
                        #value_expr.as_ref(),
                        true
                    ).count();
                    if count > #max_graphemes {
                        errors.push(::jacquard_lexicon::schema::ValidationError::MaxGraphemes {
                            field: #display_name,
                            max: #max_graphemes,
                            actual: count,
                        });
                    }
                });
            }

            if let Some(min_len) = constraints.min_length {
                checks.push(quote! {
                    #value_binding
                    if #value_expr.len() < #min_len {
                        errors.push(::jacquard_lexicon::schema::ValidationError::MinLength {
                            field: #display_name,
                            min: #min_len,
                            actual: #value_expr.len(),
                        });
                    }
                });
            }

            if let Some(min_graphemes) = constraints.min_graphemes {
                checks.push(quote! {
                    #value_binding
                    let count = ::unicode_segmentation::UnicodeSegmentation::graphemes(
                        #value_expr.as_ref(),
                        true
                    ).count();
                    if count < #min_graphemes {
                        errors.push(::jacquard_lexicon::schema::ValidationError::MinGraphemes {
                            field: #display_name,
                            min: #min_graphemes,
                            actual: count,
                        });
                    }
                });
            }
        }
        Some(LexiconPrimitiveType::Integer) => {
            if let Some(maximum) = constraints.maximum {
                checks.push(quote! {
                    #value_binding
                    if *#value_expr > #maximum {
                        errors.push(::jacquard_lexicon::schema::ValidationError::Maximum {
                            field: #display_name,
                            max: #maximum,
                            actual: *#value_expr,
                        });
                    }
                });
            }

            if let Some(minimum) = constraints.minimum {
                checks.push(quote! {
                    #value_binding
                    if *#value_expr < #minimum {
                        errors.push(::jacquard_lexicon::schema::ValidationError::Minimum {
                            field: #display_name,
                            min: #minimum,
                            actual: *#value_expr,
                        });
                    }
                });
            }
        }
        Some(LexiconPrimitiveType::Array(_)) => {
            if let Some(max_len) = constraints.max_length {
                checks.push(quote! {
                    #value_binding
                    if #value_expr.len() > #max_len {
                        errors.push(::jacquard_lexicon::schema::ValidationError::MaxLength {
                            field: #display_name,
                            max: #max_len,
                            actual: #value_expr.len(),
                        });
                    }
                });
            }

            if let Some(min_len) = constraints.min_length {
                checks.push(quote! {
                    #value_binding
                    if #value_expr.len() < #min_len {
                        errors.push(::jacquard_lexicon::schema::ValidationError::MinLength {
                            field: #display_name,
                            min: #min_len,
                            actual: #value_expr.len(),
                        });
                    }
                });
            }
        }
        _ => {
            // No built-in validation for this type
        }
    }

    Ok(checks)
}

/// Enum implementation (union support)
fn impl_for_enum(
    input: &DeriveInput,
    type_attrs: &LexiconTypeAttrs,
    nsid: &str,
    data_enum: &syn::DataEnum,
) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let generics = &input.generics;

    // Detect lifetime
    let has_lifetime = generics.lifetimes().next().is_some();
    let lifetime = if has_lifetime {
        quote! { <'_> }
    } else {
        quote! {}
    };

    // Check if this is an open union (has #[open_union] attribute)
    let is_open = has_open_union_attr(&input.attrs);

    // Extract variant refs
    let mut refs = Vec::new();
    for variant in &data_enum.variants {
        // Skip Unknown variant (added by #[open_union] macro)
        if variant.ident == "Unknown" {
            continue;
        }

        // Get NSID for this variant
        let variant_ref = extract_variant_ref(variant, nsid)?;
        refs.push(variant_ref);
    }

    // Generate union def
    // Only set closed: true for explicitly closed unions (no #[open_union])
    // Open unions omit the field (defaults to open per spec)
    let closed_field = if !is_open {
        quote! { Some(true) }
    } else {
        quote! { None }
    };

    let user_type = quote! {
        ::jacquard_lexicon::lexicon::LexUserType::Union(
            ::jacquard_lexicon::lexicon::LexRefUnion {
                description: None,
                refs: vec![#(#refs.into()),*],
                closed: #closed_field,
            }
        )
    };

    Ok(quote! {
        impl #generics ::jacquard_lexicon::schema::LexiconSchema for #name #lifetime {
            fn nsid() -> &'static str {
                #nsid
            }

            fn schema_id() -> ::jacquard_common::CowStr<'static> {
                ::jacquard_common::CowStr::new_static(#nsid)
            }

            fn lexicon_doc(
                _generator: &mut ::jacquard_lexicon::schema::LexiconGenerator
            ) -> ::jacquard_lexicon::lexicon::LexiconDoc<'static> {
                let mut defs = ::std::collections::BTreeMap::new();
                defs.insert("main".into(), #user_type);

                ::jacquard_lexicon::lexicon::LexiconDoc {
                    lexicon: ::jacquard_lexicon::lexicon::Lexicon::Lexicon1,
                    id: #nsid.into(),
                    revision: None,
                    description: None,
                    defs,
                }
            }
        }

        ::inventory::submit! {
            ::jacquard_lexicon::schema::LexiconSchemaRef {
                nsid: #nsid,
                provider: || {
                    let mut generator = ::jacquard_lexicon::schema::LexiconGenerator::new(#nsid);
                    #name::lexicon_doc(&mut generator)
                },
            }
        }
    })
}

/// Check if type has #[open_union] attribute
fn has_open_union_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("open_union"))
}

/// Extract NSID ref for a variant
fn extract_variant_ref(variant: &syn::Variant, base_nsid: &str) -> syn::Result<String> {
    // Priority 1: Check for #[nsid = "..."] attribute
    for attr in &variant.attrs {
        if attr.path().is_ident("nsid") {
            if let syn::Meta::NameValue(meta) = &attr.meta {
                if let syn::Expr::Lit(expr_lit) = &meta.value {
                    if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                        return Ok(lit_str.value());
                    }
                }
            }
        }
    }

    // Priority 2: Check for #[serde(rename = "...")] attribute
    for attr in &variant.attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }

        let mut rename = None;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                rename = Some(lit.value());
            }
            Ok(())
        });

        if let Some(rename) = rename {
            return Ok(rename);
        }
    }

    // Priority 3: For variants with non-primitive inner types, error
    // (caller should use #[nsid] or type must impl LexiconSchema)
    match &variant.fields {
        Fields::Unit => {
            // Unit variant - generate fragment ref: baseNsid#variantName
            let variant_name = variant.ident.to_string().to_lower_camel_case();
            Ok(format!("{}#{}", base_nsid, variant_name))
        }
        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
            let ty = &fields.unnamed.first().unwrap().ty;

            // Check if primitive - if so, error (unions need refs)
            if let Some(prim) = rust_type_to_lexicon_type(ty) {
                if is_primitive(&prim) {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "union variants with primitive inner types must use #[nsid] or #[serde(rename)] attribute",
                    ));
                }
            }

            // Non-primitive - error, must have explicit attribute
            // (we can't call schema_id() at compile time)
            Err(syn::Error::new_spanned(
                variant,
                "union variants with non-primitive types must use #[nsid] or #[serde(rename)] attribute to specify the ref",
            ))
        }
        _ => Err(syn::Error::new_spanned(
            variant,
            "union variants must be unit variants or have single unnamed field",
        )),
    }
}

/// Check if a lexicon primitive type is actually a primitive (not a ref-able type)
fn is_primitive(prim: &LexiconPrimitiveType) -> bool {
    matches!(
        prim,
        LexiconPrimitiveType::Boolean
            | LexiconPrimitiveType::Integer
            | LexiconPrimitiveType::String(_)
            | LexiconPrimitiveType::Bytes
            | LexiconPrimitiveType::Unknown
    )
}
