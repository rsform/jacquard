use crate::corpus::LexiconCorpus;
use crate::error::{CodegenError, Result};
use crate::lexicon::{
    LexArrayItem, LexInteger, LexObject, LexObjectProperty, LexRecord, LexString, LexStringFormat,
    LexUserType, LexXrpcBody, LexXrpcBodySchema, LexXrpcError, LexXrpcProcedure, LexXrpcQuery,
    LexXrpcSubscription, LexXrpcSubscriptionMessageSchema,
};
use heck::{ToPascalCase, ToSnakeCase};
use proc_macro2::TokenStream;
use quote::quote;

/// Convert a value string to a valid Rust variant name
fn value_to_variant_name(value: &str) -> String {
    // Remove leading special chars and convert to pascal case
    let clean = value.trim_start_matches(|c: char| !c.is_alphanumeric());
    let variant = clean.replace('-', "_").to_pascal_case();

    // Prefix with underscore if starts with digit
    if variant.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        format!("_{}", variant)
    } else if variant.is_empty() {
        "Unknown".to_string()
    } else {
        variant
    }
}

/// Sanitize a string to be safe for identifiers and filenames
fn sanitize_name(s: &str) -> String {
    if s.is_empty() {
        return "unknown".to_string();
    }

    // Replace invalid characters with underscores
    let mut sanitized: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect();

    // Ensure it doesn't start with a digit
    if sanitized.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        sanitized = format!("_{}", sanitized);
    }

    sanitized
}

/// Create an identifier, using raw identifier if necessary for keywords
fn make_ident(s: &str) -> syn::Ident {
    if s.is_empty() {
        eprintln!("Warning: Empty identifier encountered, using 'unknown' as fallback");
        return syn::Ident::new("unknown", proc_macro2::Span::call_site());
    }

    let sanitized = sanitize_name(s);

    // Try to parse as ident, fall back to raw ident if needed
    syn::parse_str::<syn::Ident>(&sanitized)
        .unwrap_or_else(|_| {
            eprintln!("Warning: Invalid identifier '{}' sanitized to '{}'", s, sanitized);
            syn::Ident::new_raw(&sanitized, proc_macro2::Span::call_site())
        })
}

/// Code generator for lexicon types
pub struct CodeGenerator<'c> {
    corpus: &'c LexiconCorpus,
    root_module: String,
}

impl<'c> CodeGenerator<'c> {
    /// Create a new code generator
    pub fn new(corpus: &'c LexiconCorpus, root_module: impl Into<String>) -> Self {
        Self {
            corpus,
            root_module: root_module.into(),
        }
    }

    /// Generate code for a lexicon def
    pub fn generate_def(
        &self,
        nsid: &str,
        def_name: &str,
        def: &LexUserType<'static>,
    ) -> Result<TokenStream> {
        match def {
            LexUserType::Record(record) => self.generate_record(nsid, def_name, record),
            LexUserType::Object(obj) => self.generate_object(nsid, def_name, obj),
            LexUserType::XrpcQuery(query) => self.generate_query(nsid, def_name, query),
            LexUserType::XrpcProcedure(proc) => self.generate_procedure(nsid, def_name, proc),
            LexUserType::Token(_) => {
                // Token types are marker types used in knownValues enums.
                // We don't generate anything for them - the knownValues enum
                // is the actual type that gets used.
                Ok(quote! {})
            }
            LexUserType::String(s) if s.known_values.is_some() => {
                self.generate_known_values_enum(nsid, def_name, s)
            }
            LexUserType::String(s) => {
                // Plain string type alias
                let type_name = self.def_to_type_name(nsid, def_name);
                let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());
                let rust_type = self.string_to_rust_type(s);
                let doc = self.generate_doc_comment(s.description.as_ref());
                Ok(quote! {
                    #doc
                    pub type #ident<'a> = #rust_type;
                })
            }
            LexUserType::Integer(i) if i.r#enum.is_some() => {
                self.generate_integer_enum(nsid, def_name, i)
            }
            LexUserType::Array(array) => {
                // Top-level array becomes type alias to Vec<ItemType>
                let type_name = self.def_to_type_name(nsid, def_name);
                let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());
                let item_type = self.array_item_to_rust_type(nsid, &array.items)?;
                let doc = self.generate_doc_comment(array.description.as_ref());
                let needs_lifetime = self.array_item_needs_lifetime(&array.items);
                if needs_lifetime {
                    Ok(quote! {
                        #doc
                        pub type #ident<'a> = Vec<#item_type>;
                    })
                } else {
                    Ok(quote! {
                        #doc
                        pub type #ident = Vec<#item_type>;
                    })
                }
            }
            LexUserType::Boolean(_)
            | LexUserType::Integer(_)
            | LexUserType::Bytes(_)
            | LexUserType::CidLink(_)
            | LexUserType::Unknown(_) => {
                // These are rarely top-level defs, but if they are, make type aliases
                let type_name = self.def_to_type_name(nsid, def_name);
                let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());
                let (rust_type, needs_lifetime) = match def {
                    LexUserType::Boolean(_) => (quote! { bool }, false),
                    LexUserType::Integer(_) => (quote! { i64 }, false),
                    LexUserType::Bytes(_) => (quote! { bytes::Bytes }, false),
                    LexUserType::CidLink(_) => {
                        (quote! { jacquard_common::types::cid::CidLink<'a> }, true)
                    }
                    LexUserType::Unknown(_) => {
                        (quote! { jacquard_common::types::value::Data<'a> }, true)
                    }
                    _ => unreachable!(),
                };
                if needs_lifetime {
                    Ok(quote! {
                        pub type #ident<'a> = #rust_type;
                    })
                } else {
                    Ok(quote! {
                        pub type #ident = #rust_type;
                    })
                }
            }
            LexUserType::Blob(_) => Err(CodegenError::unsupported(
                format!("top-level def type {:?}", def),
                nsid,
                None::<String>,
            )),
            LexUserType::XrpcSubscription(sub) => self.generate_subscription(nsid, def_name, sub),
        }
    }

    /// Generate a record type
    fn generate_record(
        &self,
        nsid: &str,
        def_name: &str,
        record: &LexRecord<'static>,
    ) -> Result<TokenStream> {
        match &record.record {
            crate::lexicon::LexRecordRecord::Object(obj) => {
                let type_name = self.def_to_type_name(nsid, def_name);
                let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());

                // Generate main struct fields
                let fields = self.generate_object_fields(nsid, &type_name, obj, false)?;
                let doc = self.generate_doc_comment(record.description.as_ref());

                // Records always get a lifetime since they have the #[lexicon] attribute
                // which adds extra_data: BTreeMap<..., Data<'a>>
                let struct_def = quote! {
                    #doc
                    #[jacquard_derive::lexicon]
                    #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic)]
                    #[serde(rename_all = "camelCase")]
                    pub struct #ident<'a> {
                        #fields
                    }
                };

                // Generate union types and nested object types for this record
                let mut unions = Vec::new();
                for (field_name, field_type) in &obj.properties {
                    match field_type {
                        LexObjectProperty::Union(union) => {
                            let union_name =
                                format!("{}Record{}", type_name, field_name.to_pascal_case());
                            // Clone refs to avoid lifetime issues
                            let refs: Vec<_> = union.refs.iter().cloned().collect();
                            let union_def =
                                self.generate_union(&union_name, &refs, None, union.closed)?;
                            unions.push(union_def);
                        }
                        LexObjectProperty::Object(nested_obj) => {
                            let object_name =
                                format!("{}Record{}", type_name, field_name.to_pascal_case());
                            let obj_def = self.generate_object(nsid, &object_name, nested_obj)?;
                            unions.push(obj_def);
                        }
                        _ => {}
                    }
                }

                // Generate Collection trait impl
                let collection_impl = quote! {
                    impl jacquard_common::types::collection::Collection for #ident<'_> {
                        const NSID: &'static str = #nsid;
                    }
                };

                // Generate IntoStatic impl
                // let field_names: Vec<&str> = obj.properties.keys().map(|k| k.as_str()).collect();
                // let into_static_impl =
                //     self.generate_into_static_for_struct(&type_name, &field_names, true, true);

                Ok(quote! {
                    #struct_def
                    #(#unions)*
                    #collection_impl
                    //#into_static_impl
                })
            }
        }
    }

    /// Generate an object type
    fn generate_object(
        &self,
        nsid: &str,
        def_name: &str,
        obj: &LexObject<'static>,
    ) -> Result<TokenStream> {
        let type_name = self.def_to_type_name(nsid, def_name);
        let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());

        let fields = self.generate_object_fields(nsid, &type_name, obj, false)?;
        let doc = self.generate_doc_comment(obj.description.as_ref());

        // Objects always get a lifetime since they have the #[lexicon] attribute
        // which adds extra_data: BTreeMap<..., Data<'a>>
        let struct_def = quote! {
            #doc
            #[jacquard_derive::lexicon]
            #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic)]
            #[serde(rename_all = "camelCase")]
            pub struct #ident<'a> {
                #fields
            }
        };

        // Generate union types and nested object types for this object
        let mut unions = Vec::new();
        for (field_name, field_type) in &obj.properties {
            match field_type {
                LexObjectProperty::Union(union) => {
                    let union_name = format!("{}Record{}", type_name, field_name.to_pascal_case());
                    let refs: Vec<_> = union.refs.iter().cloned().collect();
                    let union_def = self.generate_union(&union_name, &refs, None, union.closed)?;
                    unions.push(union_def);
                }
                LexObjectProperty::Object(nested_obj) => {
                    let object_name = format!("{}Record{}", type_name, field_name.to_pascal_case());
                    let obj_def = self.generate_object(nsid, &object_name, nested_obj)?;
                    unions.push(obj_def);
                }
                _ => {}
            }
        }

        // Generate IntoStatic impl
        // let field_names: Vec<&str> = obj.properties.keys().map(|k| k.as_str()).collect();
        // let into_static_impl =
        //     self.generate_into_static_for_struct(&type_name, &field_names, true, true);

        Ok(quote! {
            #struct_def
            #(#unions)*
            //#into_static_impl
        })
    }

    /// Generate fields for an object
    fn generate_object_fields(
        &self,
        nsid: &str,
        parent_type_name: &str,
        obj: &LexObject<'static>,
        is_builder: bool,
    ) -> Result<TokenStream> {
        let required = obj.required.as_ref().map(|r| r.as_slice()).unwrap_or(&[]);

        let mut fields = Vec::new();
        for (field_name, field_type) in &obj.properties {
            let is_required = required.contains(field_name);
            let field_tokens = self.generate_field(
                nsid,
                parent_type_name,
                field_name,
                field_type,
                is_required,
                is_builder,
            )?;
            fields.push(field_tokens);
        }

        Ok(quote! { #(#fields)* })
    }

    /// Generate a single field
    fn generate_field(
        &self,
        nsid: &str,
        parent_type_name: &str,
        field_name: &str,
        field_type: &LexObjectProperty<'static>,
        is_required: bool,
        is_builder: bool,
    ) -> Result<TokenStream> {
        if field_name.is_empty() {
            eprintln!(
                "Warning: Empty field name in lexicon '{}' type '{}', using 'unknown' as fallback",
                nsid, parent_type_name
            );
        }
        let field_ident = make_ident(&field_name.to_snake_case());

        let rust_type =
            self.property_to_rust_type(nsid, parent_type_name, field_name, field_type)?;
        let needs_lifetime = self.property_needs_lifetime(field_type);

        // Check if this is a CowStr field for builder(into) attribute
        let is_cowstr = matches!(field_type, LexObjectProperty::String(s) if s.format.is_none());

        let rust_type = if is_required {
            rust_type
        } else {
            quote! { std::option::Option<#rust_type> }
        };

        // Extract description from field type
        let description = match field_type {
            LexObjectProperty::Ref(r) => r.description.as_ref(),
            LexObjectProperty::Union(u) => u.description.as_ref(),
            LexObjectProperty::Bytes(b) => b.description.as_ref(),
            LexObjectProperty::CidLink(c) => c.description.as_ref(),
            LexObjectProperty::Array(a) => a.description.as_ref(),
            LexObjectProperty::Blob(b) => b.description.as_ref(),
            LexObjectProperty::Object(o) => o.description.as_ref(),
            LexObjectProperty::Boolean(b) => b.description.as_ref(),
            LexObjectProperty::Integer(i) => i.description.as_ref(),
            LexObjectProperty::String(s) => s.description.as_ref(),
            LexObjectProperty::Unknown(u) => u.description.as_ref(),
        };
        let doc = self.generate_doc_comment(description);

        let mut attrs = Vec::new();

        if !is_required {
            attrs.push(quote! { #[serde(skip_serializing_if = "std::option::Option::is_none")] });
        }

        // Add serde(borrow) to all fields with lifetimes
        if needs_lifetime {
            attrs.push(quote! { #[serde(borrow)] });
        }

        // Add builder(into) for CowStr fields to allow String, &str, etc., but only for builder structs
        if is_builder && is_cowstr {
            attrs.push(quote! { #[builder(into)] });
        }

        Ok(quote! {
            #doc
            #(#attrs)*
            pub #field_ident: #rust_type,
        })
    }

    /// Check if a property type needs a lifetime parameter
    fn property_needs_lifetime(&self, prop: &LexObjectProperty<'static>) -> bool {
        match prop {
            LexObjectProperty::Boolean(_) | LexObjectProperty::Integer(_) => false,
            LexObjectProperty::String(s) => self.string_needs_lifetime(s),
            LexObjectProperty::Bytes(_) => false, // Bytes is owned
            LexObjectProperty::CidLink(_)
            | LexObjectProperty::Blob(_)
            | LexObjectProperty::Unknown(_) => true,
            LexObjectProperty::Array(array) => self.array_item_needs_lifetime(&array.items),
            LexObjectProperty::Object(_) => true, // Nested objects have lifetimes
            LexObjectProperty::Ref(ref_type) => {
                // Check if the ref target actually needs a lifetime
                self.ref_needs_lifetime(&ref_type.r#ref)
            }
            LexObjectProperty::Union(_) => true, // Unions generally have lifetimes
        }
    }

    /// Check if an array item type needs a lifetime parameter
    fn array_item_needs_lifetime(&self, item: &LexArrayItem) -> bool {
        match item {
            LexArrayItem::Boolean(_) | LexArrayItem::Integer(_) => false,
            LexArrayItem::String(s) => self.string_needs_lifetime(s),
            LexArrayItem::Bytes(_) => false,
            LexArrayItem::CidLink(_) | LexArrayItem::Blob(_) | LexArrayItem::Unknown(_) => true,
            LexArrayItem::Object(_) => true, // Nested objects have lifetimes
            LexArrayItem::Ref(ref_type) => self.ref_needs_lifetime(&ref_type.r#ref),
            LexArrayItem::Union(_) => true,
        }
    }

    /// Check if a string type needs a lifetime parameter
    fn string_needs_lifetime(&self, s: &LexString) -> bool {
        match s.format {
            Some(LexStringFormat::Datetime)
            | Some(LexStringFormat::Language)
            | Some(LexStringFormat::Tid) => false,
            _ => true, // Most string types borrow
        }
    }

    /// Check if a ref needs a lifetime parameter
    fn ref_needs_lifetime(&self, ref_str: &str) -> bool {
        // Try to resolve the ref
        if let Some((_doc, def)) = self.corpus.resolve_ref(ref_str) {
            self.def_needs_lifetime(def)
        } else {
            // If we can't resolve it, assume it needs a lifetime (safe default)
            true
        }
    }

    /// Check if a lexicon def needs a lifetime parameter
    fn def_needs_lifetime(&self, def: &LexUserType<'static>) -> bool {
        match def {
            // Records and Objects always have lifetimes now since they get #[lexicon] attribute
            LexUserType::Record(_) => true,
            LexUserType::Object(_) => true,
            LexUserType::Token(_) => false,
            LexUserType::String(s) => {
                // Check if it's a known values enum or a regular string
                if s.known_values.is_some() {
                    // Known values enums have Other(CowStr<'a>) variant
                    true
                } else {
                    self.string_needs_lifetime(s)
                }
            }
            LexUserType::Integer(_) => false,
            LexUserType::Boolean(_) => false,
            LexUserType::Bytes(_) => false,
            LexUserType::CidLink(_) | LexUserType::Blob(_) | LexUserType::Unknown(_) => true,
            LexUserType::Array(array) => self.array_item_needs_lifetime(&array.items),
            LexUserType::XrpcQuery(_)
            | LexUserType::XrpcProcedure(_)
            | LexUserType::XrpcSubscription(_) => {
                // XRPC types generate multiple structs, not a single type we can reference
                // Shouldn't be referenced directly
                true
            }
        }
    }

    /// Check if xrpc params need a lifetime parameter
    fn params_need_lifetime(&self, params: &crate::lexicon::LexXrpcParameters<'static>) -> bool {
        params.properties.values().any(|prop| {
            use crate::lexicon::LexXrpcParametersProperty;
            match prop {
                LexXrpcParametersProperty::Boolean(_) | LexXrpcParametersProperty::Integer(_) => {
                    false
                }
                LexXrpcParametersProperty::String(s) => self.string_needs_lifetime(s),
                LexXrpcParametersProperty::Unknown(_) => true,
                LexXrpcParametersProperty::Array(arr) => {
                    use crate::lexicon::LexPrimitiveArrayItem;
                    match &arr.items {
                        LexPrimitiveArrayItem::Boolean(_) | LexPrimitiveArrayItem::Integer(_) => {
                            false
                        }
                        LexPrimitiveArrayItem::String(s) => self.string_needs_lifetime(s),
                        LexPrimitiveArrayItem::Unknown(_) => true,
                    }
                }
            }
        })
    }

    /// Convert a property type to Rust type
    fn property_to_rust_type(
        &self,
        nsid: &str,
        parent_type_name: &str,
        field_name: &str,
        prop: &LexObjectProperty<'static>,
    ) -> Result<TokenStream> {
        match prop {
            LexObjectProperty::Boolean(_) => Ok(quote! { bool }),
            LexObjectProperty::Integer(_) => Ok(quote! { i64 }),
            LexObjectProperty::String(s) => Ok(self.string_to_rust_type(s)),
            LexObjectProperty::Bytes(_) => Ok(quote! { bytes::Bytes }),
            LexObjectProperty::CidLink(_) => {
                Ok(quote! { jacquard_common::types::cid::CidLink<'a> })
            }
            LexObjectProperty::Blob(_) => Ok(quote! { jacquard_common::types::blob::Blob<'a> }),
            LexObjectProperty::Unknown(_) => Ok(quote! { jacquard_common::types::value::Data<'a> }),
            LexObjectProperty::Array(array) => {
                let item_type = self.array_item_to_rust_type(nsid, &array.items)?;
                Ok(quote! { Vec<#item_type> })
            }
            LexObjectProperty::Object(_object) => {
                // Generate unique nested object type name: StatusView + metadata -> StatusViewRecordMetadata
                let object_name =
                    format!("{}Record{}", parent_type_name, field_name.to_pascal_case());
                let object_ident = syn::Ident::new(&object_name, proc_macro2::Span::call_site());
                Ok(quote! { #object_ident<'a> })
            }
            LexObjectProperty::Ref(ref_type) => {
                // Handle local refs (starting with #) by prepending the current NSID
                let ref_str = if ref_type.r#ref.starts_with('#') {
                    format!("{}{}", nsid, ref_type.r#ref)
                } else {
                    ref_type.r#ref.to_string()
                };
                self.ref_to_rust_type(&ref_str)
            }
            LexObjectProperty::Union(_union) => {
                // Generate unique union type name: StatusView + embed -> StatusViewRecordEmbed
                let union_name =
                    format!("{}Record{}", parent_type_name, field_name.to_pascal_case());
                let union_ident = syn::Ident::new(&union_name, proc_macro2::Span::call_site());
                Ok(quote! { #union_ident<'a> })
            }
        }
    }

    /// Convert array item to Rust type
    fn array_item_to_rust_type(&self, nsid: &str, item: &LexArrayItem) -> Result<TokenStream> {
        match item {
            LexArrayItem::Boolean(_) => Ok(quote! { bool }),
            LexArrayItem::Integer(_) => Ok(quote! { i64 }),
            LexArrayItem::String(s) => Ok(self.string_to_rust_type(s)),
            LexArrayItem::Bytes(_) => Ok(quote! { bytes::Bytes }),
            LexArrayItem::CidLink(_) => Ok(quote! { jacquard_common::types::cid::CidLink<'a> }),
            LexArrayItem::Blob(_) => Ok(quote! { jacquard_common::types::blob::Blob<'a> }),
            LexArrayItem::Unknown(_) => Ok(quote! { jacquard_common::types::value::Data<'a> }),
            LexArrayItem::Object(_) => {
                // For inline objects in arrays, use Data since we can't generate a unique type name
                Ok(quote! { jacquard_common::types::value::Data<'a> })
            }
            LexArrayItem::Ref(ref_type) => {
                // Handle local refs (starting with #) by prepending the current NSID
                let ref_str = if ref_type.r#ref.starts_with('#') {
                    format!("{}{}", nsid, ref_type.r#ref)
                } else {
                    ref_type.r#ref.to_string()
                };
                self.ref_to_rust_type(&ref_str)
            }
            LexArrayItem::Union(_) => {
                // For now, use Data
                Ok(quote! { jacquard_common::types::value::Data<'a> })
            }
        }
    }

    /// Convert string type to Rust type
    fn string_to_rust_type(&self, s: &LexString) -> TokenStream {
        match s.format {
            Some(LexStringFormat::Datetime) => {
                quote! { jacquard_common::types::string::Datetime }
            }
            Some(LexStringFormat::Did) => quote! { jacquard_common::types::string::Did<'a> },
            Some(LexStringFormat::Handle) => quote! { jacquard_common::types::string::Handle<'a> },
            Some(LexStringFormat::AtIdentifier) => {
                quote! { jacquard_common::types::ident::AtIdentifier<'a> }
            }
            Some(LexStringFormat::Nsid) => quote! { jacquard_common::types::string::Nsid<'a> },
            Some(LexStringFormat::AtUri) => quote! { jacquard_common::types::string::AtUri<'a> },
            Some(LexStringFormat::Uri) => quote! { jacquard_common::types::string::Uri<'a> },
            Some(LexStringFormat::Cid) => quote! { jacquard_common::types::string::Cid<'a> },
            Some(LexStringFormat::Language) => {
                quote! { jacquard_common::types::string::Language }
            }
            Some(LexStringFormat::Tid) => quote! { jacquard_common::types::string::Tid },
            Some(LexStringFormat::RecordKey) => {
                quote! { jacquard_common::types::string::RecordKey<jacquard_common::types::string::Rkey<'a>> }
            }
            _ => quote! { jacquard_common::CowStr<'a> },
        }
    }

    /// Convert ref to Rust type path
    fn ref_to_rust_type(&self, ref_str: &str) -> Result<TokenStream> {
        // Parse NSID and fragment
        let (ref_nsid, ref_def) = if let Some((nsid, fragment)) = ref_str.split_once('#') {
            (nsid, fragment)
        } else {
            (ref_str, "main")
        };

        // Check if ref exists
        if !self.corpus.ref_exists(ref_str) {
            // Fallback to Data
            return Ok(quote! { jacquard_common::types::value::Data<'a> });
        }

        // Convert NSID to module path
        // com.atproto.repo.strongRef -> com_atproto::repo::strong_ref::StrongRef
        // app.bsky.richtext.facet -> app_bsky::richtext::facet::Facet
        // app.bsky.actor.defs#nux -> app_bsky::actor::Nux (defs go in parent module)
        let parts: Vec<&str> = ref_nsid.split('.').collect();
        let last_segment = parts.last().unwrap();

        let type_name = self.def_to_type_name(ref_nsid, ref_def);

        let path_str = if *last_segment == "defs" && parts.len() >= 3 {
            // defs types go in parent module
            let first_two = format!("{}_{}", parts[0], parts[1]);
            if parts.len() == 3 {
                // com.atproto.defs -> com_atproto::TypeName
                format!("{}::{}::{}", self.root_module, first_two, type_name)
            } else {
                // app.bsky.actor.defs -> app_bsky::actor::TypeName
                let middle: Vec<&str> = parts[2..parts.len() - 1].iter().copied().collect();
                format!(
                    "{}::{}::{}::{}",
                    self.root_module,
                    first_two,
                    middle.join("::"),
                    type_name
                )
            }
        } else {
            // Regular types go in their own module file
            let (module_path, file_module) = if parts.len() >= 3 {
                // Join first two segments with underscore
                let first_two = format!("{}_{}", parts[0], parts[1]);
                let file_name = last_segment.to_snake_case();

                if parts.len() > 3 {
                    // Middle segments form the module path
                    let middle: Vec<&str> = parts[2..parts.len() - 1].iter().copied().collect();
                    let base_path = format!("{}::{}", first_two, middle.join("::"));
                    (base_path, file_name)
                } else {
                    // Only 3 parts: com.atproto.label -> com_atproto, file: label
                    (first_two, file_name)
                }
            } else if parts.len() == 2 {
                // e.g., "com.example" -> "com_example", file: example
                let first = parts[0].to_string();
                let file_name = parts[1].to_snake_case();
                (first, file_name)
            } else {
                (parts[0].to_string(), "main".to_string())
            };

            format!(
                "{}::{}::{}::{}",
                self.root_module, module_path, file_module, type_name
            )
        };

        let path: syn::Path = syn::parse_str(&path_str).map_err(|e| CodegenError::Other {
            message: format!("Failed to parse path: {}", e),
            source: None,
        })?;

        // Only add lifetime if the target type needs it
        if self.ref_needs_lifetime(ref_str) {
            Ok(quote! { #path<'a> })
        } else {
            Ok(quote! { #path })
        }
    }

    /// Generate query type
    fn generate_query(
        &self,
        nsid: &str,
        def_name: &str,
        query: &LexXrpcQuery<'static>,
    ) -> Result<TokenStream> {
        let type_base = self.def_to_type_name(nsid, def_name);
        let mut output = Vec::new();

        let params_has_lifetime = query
            .parameters
            .as_ref()
            .map(|p| match p {
                crate::lexicon::LexXrpcQueryParameter::Params(params) => {
                    self.params_need_lifetime(params)
                }
            })
            .unwrap_or(false);
        let has_params = query.parameters.is_some();
        let has_output = query.output.is_some();
        let has_errors = query.errors.is_some();

        if let Some(params) = &query.parameters {
            let params_struct = self.generate_params_struct(&type_base, params)?;
            output.push(params_struct);
        }

        if let Some(body) = &query.output {
            let output_struct = self.generate_output_struct(&type_base, body)?;
            output.push(output_struct);
        }

        if let Some(errors) = &query.errors {
            let error_enum = self.generate_error_enum(&type_base, errors)?;
            output.push(error_enum);
        }

        // Generate XrpcRequest impl
        let output_encoding = query
            .output
            .as_ref()
            .map(|o| o.encoding.as_ref())
            .unwrap_or("application/json");
        let xrpc_impl = self.generate_xrpc_request_impl(
            nsid,
            &type_base,
            quote! { jacquard_common::xrpc::XrpcMethod::Query },
            output_encoding,
            has_params,
            params_has_lifetime,
            has_output,
            has_errors,
            false, // queries never have binary inputs
        )?;
        output.push(xrpc_impl);

        Ok(quote! {
            #(#output)*
        })
    }

    /// Generate procedure type
    fn generate_procedure(
        &self,
        nsid: &str,
        def_name: &str,
        proc: &LexXrpcProcedure<'static>,
    ) -> Result<TokenStream> {
        let type_base = self.def_to_type_name(nsid, def_name);
        let mut output = Vec::new();

        // Check if input is a binary body (no schema)
        let is_binary_input = proc
            .input
            .as_ref()
            .map(|i| i.schema.is_none())
            .unwrap_or(false);

        // Input bodies with schemas have lifetimes (they get #[lexicon] attribute)
        // Binary inputs don't have lifetimes
        let params_has_lifetime = proc.input.is_some() && !is_binary_input;
        let has_input = proc.input.is_some();
        let has_output = proc.output.is_some();
        let has_errors = proc.errors.is_some();

        if let Some(params) = &proc.parameters {
            let params_struct = self.generate_params_struct_proc(&type_base, params)?;
            output.push(params_struct);
        }

        if let Some(body) = &proc.input {
            let input_struct = self.generate_input_struct(&type_base, body)?;
            output.push(input_struct);
        }

        if let Some(body) = &proc.output {
            let output_struct = self.generate_output_struct(&type_base, body)?;
            output.push(output_struct);
        }

        if let Some(errors) = &proc.errors {
            let error_enum = self.generate_error_enum(&type_base, errors)?;
            output.push(error_enum);
        }

        // Generate XrpcRequest impl
        let input_encoding = proc
            .input
            .as_ref()
            .map(|i| i.encoding.as_ref())
            .unwrap_or("application/json");
        let output_encoding = proc
            .output
            .as_ref()
            .map(|o| o.encoding.as_ref())
            .unwrap_or("application/json");
        let xrpc_impl = self.generate_xrpc_request_impl(
            nsid,
            &type_base,
            quote! { jacquard_common::xrpc::XrpcMethod::Procedure(#input_encoding) },
            output_encoding,
            has_input,
            params_has_lifetime,
            has_output,
            has_errors,
            is_binary_input,
        )?;
        output.push(xrpc_impl);

        Ok(quote! {
            #(#output)*
        })
    }

    fn generate_subscription(
        &self,
        nsid: &str,
        def_name: &str,
        sub: &LexXrpcSubscription<'static>,
    ) -> Result<TokenStream> {
        let type_base = self.def_to_type_name(nsid, def_name);
        let mut output = Vec::new();

        if let Some(params) = &sub.parameters {
            // Extract LexXrpcParameters from the enum
            match params {
                crate::lexicon::LexXrpcSubscriptionParameter::Params(params_inner) => {
                    let params_struct =
                        self.generate_params_struct_inner(&type_base, params_inner)?;
                    output.push(params_struct);
                }
            }
        }

        if let Some(message) = &sub.message {
            if let Some(schema) = &message.schema {
                let message_type = self.generate_subscription_message(&type_base, schema)?;
                output.push(message_type);
            }
        }

        if let Some(errors) = &sub.errors {
            let error_enum = self.generate_error_enum(&type_base, errors)?;
            output.push(error_enum);
        }

        Ok(quote! {
            #(#output)*
        })
    }

    fn generate_subscription_message(
        &self,
        type_base: &str,
        schema: &LexXrpcSubscriptionMessageSchema<'static>,
    ) -> Result<TokenStream> {
        use crate::lexicon::LexXrpcSubscriptionMessageSchema;

        match schema {
            LexXrpcSubscriptionMessageSchema::Union(union) => {
                // Generate a union enum for the message
                let enum_name = format!("{}Message", type_base);
                let enum_ident = syn::Ident::new(&enum_name, proc_macro2::Span::call_site());

                let mut variants = Vec::new();
                for ref_str in &union.refs {
                    let ref_str_s = ref_str.as_ref();
                    // Parse ref to get NSID and def name
                    let (ref_nsid, ref_def) =
                        if let Some((nsid, fragment)) = ref_str.split_once('#') {
                            (nsid, fragment)
                        } else {
                            (ref_str.as_ref(), "main")
                        };

                    let variant_name = if ref_def == "main" {
                        ref_nsid.split('.').last().unwrap().to_pascal_case()
                    } else {
                        ref_def.to_pascal_case()
                    };
                    let variant_ident =
                        syn::Ident::new(&variant_name, proc_macro2::Span::call_site());
                    let type_path = self.ref_to_rust_type(ref_str)?;

                    variants.push(quote! {
                        #[serde(rename = #ref_str_s)]
                        #variant_ident(Box<#type_path>)
                    });
                }

                let doc = self.generate_doc_comment(union.description.as_ref());

                // Generate IntoStatic impl for the enum
                // let variant_info: Vec<(String, EnumVariantKind)> = union
                //     .refs
                //     .iter()
                //     .map(|ref_str| {
                //         let ref_def = if let Some((_, fragment)) = ref_str.split_once('#') {
                //             fragment
                //         } else {
                //             "main"
                //         };
                //         let variant_name = if ref_def == "main" {
                //             ref_str.split('.').last().unwrap().to_pascal_case()
                //         } else {
                //             ref_def.to_pascal_case()
                //         };
                //         (variant_name, EnumVariantKind::Tuple)
                //     })
                //     .collect();
                // let into_static_impl = self.generate_into_static_for_enum(
                //     &enum_name,
                //     &variant_info,
                //     true,
                //     true, // open union
                // );

                Ok(quote! {
                    #doc
                    #[jacquard_derive::open_union]
                    #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic)]
                    #[serde(tag = "$type")]
                    #[serde(bound(deserialize = "'de: 'a"))]
                    pub enum #enum_ident<'a> {
                        #(#variants,)*
                    }

                    //#into_static_impl
                })
            }
            LexXrpcSubscriptionMessageSchema::Object(obj) => {
                // Generate a struct for the message
                let struct_name = format!("{}Message", type_base);
                let struct_ident = syn::Ident::new(&struct_name, proc_macro2::Span::call_site());

                let fields = self.generate_object_fields("", &struct_name, obj, false)?;
                let doc = self.generate_doc_comment(obj.description.as_ref());

                // Subscription message structs always get a lifetime since they have the #[lexicon] attribute
                // which adds extra_data: BTreeMap<..., Data<'a>>
                let struct_def = quote! {
                    #doc
                    #[jacquard_derive::lexicon]
                    #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic)]
                    #[serde(rename_all = "camelCase")]
                    pub struct #struct_ident<'a> {
                        #fields
                    }
                };

                // Generate union types for this message
                let mut unions = Vec::new();
                for (field_name, field_type) in &obj.properties {
                    if let LexObjectProperty::Union(union) = field_type {
                        let union_name =
                            format!("{}Record{}", struct_name, field_name.to_pascal_case());
                        let refs: Vec<_> = union.refs.iter().cloned().collect();
                        let union_def =
                            self.generate_union(&union_name, &refs, None, union.closed)?;
                        unions.push(union_def);
                    }
                }

                // Generate IntoStatic impl
                // let field_names: Vec<&str> = obj.properties.keys().map(|k| k.as_str()).collect();
                // let into_static_impl =
                //     self.generate_into_static_for_struct(&struct_name, &field_names, true, true);

                Ok(quote! {
                    #struct_def
                    #(#unions)*
                    //#into_static_impl
                })
            }
            LexXrpcSubscriptionMessageSchema::Ref(ref_type) => {
                // Just a type alias to the referenced type
                // Refs generally have lifetimes, so always add <'a>
                let type_name = format!("{}Message", type_base);
                let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());
                let rust_type = self.ref_to_rust_type(&ref_type.r#ref)?;
                let doc = self.generate_doc_comment(ref_type.description.as_ref());

                Ok(quote! {
                    #doc
                    pub type #ident<'a> = #rust_type;
                })
            }
        }
    }

    /// Convert def name to Rust type name
    fn def_to_type_name(&self, nsid: &str, def_name: &str) -> String {
        if def_name == "main" {
            // Use last segment of NSID
            let base_name = nsid.split('.').last().unwrap().to_pascal_case();

            // Check if any other def would collide with this name
            if let Some(doc) = self.corpus.get(nsid) {
                let has_collision = doc.defs.keys().any(|other_def| {
                    let other_def_str: &str = other_def.as_ref();
                    other_def_str != "main" && other_def_str.to_pascal_case() == base_name
                });

                if has_collision {
                    return format!("{}Record", base_name);
                }
            }

            base_name
        } else {
            def_name.to_pascal_case()
        }
    }

    /// Convert NSID to file path relative to output directory
    ///
    /// - `app.bsky.feed.post` → `app_bsky/feed/post.rs`
    /// - `com.atproto.label.defs` → `com_atproto/label.rs` (defs go in parent)
    fn nsid_to_file_path(&self, nsid: &str) -> std::path::PathBuf {
        let parts: Vec<&str> = nsid.split('.').collect();

        if parts.len() < 2 {
            // Shouldn't happen with valid NSIDs, but handle gracefully
            return format!("{}.rs", sanitize_name(parts[0])).into();
        }

        let last = parts.last().unwrap();

        if *last == "defs" && parts.len() >= 3 {
            // defs go in parent module: com.atproto.label.defs → com_atproto/label.rs
            let first_two = format!("{}_{}", sanitize_name(parts[0]), sanitize_name(parts[1]));
            if parts.len() == 3 {
                // com.atproto.defs → com_atproto.rs
                format!("{}.rs", first_two).into()
            } else {
                // com.atproto.label.defs → com_atproto/label.rs
                let middle: Vec<&str> = parts[2..parts.len() - 1].iter().copied().collect();
                let mut path = std::path::PathBuf::from(first_two);
                for segment in &middle[..middle.len() - 1] {
                    path.push(sanitize_name(segment));
                }
                path.push(format!("{}.rs", sanitize_name(middle.last().unwrap())));
                path
            }
        } else {
            // Regular path: app.bsky.feed.post → app_bsky/feed/post.rs
            let first_two = format!("{}_{}", sanitize_name(parts[0]), sanitize_name(parts[1]));
            let mut path = std::path::PathBuf::from(first_two);

            for segment in &parts[2..parts.len() - 1] {
                path.push(sanitize_name(segment));
            }

            path.push(format!("{}.rs", sanitize_name(&last.to_snake_case())));
            path
        }
    }

    /// Generate all code for the corpus, organized by file
    /// Returns a map of file paths to (tokens, optional NSID)
    pub fn generate_all(
        &self,
    ) -> Result<std::collections::BTreeMap<std::path::PathBuf, (TokenStream, Option<String>)>> {
        use std::collections::BTreeMap;

        let mut file_contents: BTreeMap<std::path::PathBuf, Vec<TokenStream>> = BTreeMap::new();
        let mut file_nsids: BTreeMap<std::path::PathBuf, String> = BTreeMap::new();

        // Generate code for all lexicons
        for (nsid, doc) in self.corpus.iter() {
            let file_path = self.nsid_to_file_path(nsid.as_ref());

            // Track which NSID this file is for
            file_nsids.insert(file_path.clone(), nsid.to_string());

            for (def_name, def) in &doc.defs {
                let tokens = self.generate_def(nsid.as_ref(), def_name.as_ref(), def)?;
                file_contents
                    .entry(file_path.clone())
                    .or_default()
                    .push(tokens);
            }
        }

        // Combine all tokens for each file
        let mut result = BTreeMap::new();
        for (path, tokens_vec) in file_contents {
            let nsid = file_nsids.get(&path).cloned();
            result.insert(path, (quote! { #(#tokens_vec)* }, nsid));
        }

        Ok(result)
    }

    /// Generate parent module files with pub mod declarations
    pub fn generate_module_tree(
        &self,
        file_map: &std::collections::BTreeMap<std::path::PathBuf, (TokenStream, Option<String>)>,
        defs_only: &std::collections::BTreeMap<std::path::PathBuf, (TokenStream, Option<String>)>,
    ) -> std::collections::BTreeMap<std::path::PathBuf, (TokenStream, Option<String>)> {
        use std::collections::{BTreeMap, BTreeSet};

        // Track what modules each directory needs to declare
        // Key: directory path, Value: set of module names (file stems)
        let mut dir_modules: BTreeMap<std::path::PathBuf, BTreeSet<String>> = BTreeMap::new();

        // Collect all parent directories that have files
        let mut all_dirs: BTreeSet<std::path::PathBuf> = BTreeSet::new();
        for path in file_map.keys() {
            if let Some(parent_dir) = path.parent() {
                all_dirs.insert(parent_dir.to_path_buf());
            }
        }

        for path in file_map.keys() {
            if let Some(parent_dir) = path.parent() {
                if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                    // Skip mod.rs and lib.rs - they're module files, not modules to declare
                    if file_stem == "mod" || file_stem == "lib" {
                        continue;
                    }

                    // Always add the module declaration to parent
                    dir_modules
                        .entry(parent_dir.to_path_buf())
                        .or_default()
                        .insert(file_stem.to_string());
                }
            }
        }

        // Generate module files
        let mut result = BTreeMap::new();

        for (dir, module_names) in dir_modules {
            let mod_file_path = if dir.components().count() == 0 {
                // Root directory -> lib.rs for library crates
                std::path::PathBuf::from("lib.rs")
            } else {
                // Subdirectory: app_bsky/feed -> app_bsky/feed.rs (Rust 2018 style)
                let dir_name = dir.file_name().and_then(|s| s.to_str()).unwrap_or("mod");
                let sanitized_dir_name = sanitize_name(dir_name);
                let mut path = dir
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new(""))
                    .to_path_buf();
                path.push(format!("{}.rs", sanitized_dir_name));
                path
            };

            let is_root = dir.components().count() == 0;
            let mods: Vec<_> = module_names
                .iter()
                .map(|name| {
                    let ident = make_ident(name);
                    if is_root {
                        // Top-level modules get feature gates
                        quote! {
                            #[cfg(feature = #name)]
                            pub mod #ident;
                        }
                    } else {
                        quote! { pub mod #ident; }
                    }
                })
                .collect();

            // If this file already exists in defs_only (e.g., from defs), merge the content
            let module_tokens = quote! { #(#mods)* };
            if let Some((existing_tokens, nsid)) = defs_only.get(&mod_file_path) {
                // Put module declarations FIRST, then existing defs content
                result.insert(
                    mod_file_path,
                    (quote! { #module_tokens #existing_tokens }, nsid.clone()),
                );
            } else {
                result.insert(mod_file_path, (module_tokens, None));
            }
        }

        result
    }

    /// Write all generated code to disk
    pub fn write_to_disk(&self, output_dir: &std::path::Path) -> Result<()> {
        // Generate all code (defs only)
        let defs_files = self.generate_all()?;
        let mut all_files = defs_files.clone();

        // Generate module tree iteratively until no new files appear
        loop {
            let module_map = self.generate_module_tree(&all_files, &defs_files);
            let old_count = all_files.len();

            // Merge new module files
            for (path, tokens) in module_map {
                all_files.insert(path, tokens);
            }

            if all_files.len() == old_count {
                // No new files added
                break;
            }
        }

        // Write to disk
        for (path, (tokens, nsid)) in all_files {
            let full_path = output_dir.join(&path);

            // Create parent directories
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| CodegenError::Other {
                    message: format!("Failed to create directory {:?}: {}", parent, e),
                    source: None,
                })?;
            }

            // Format code
            let file: syn::File = syn::parse2(tokens.clone()).map_err(|e| CodegenError::Other {
                message: format!(
                    "Failed to parse tokens for {:?}: {}\nTokens: {}",
                    path, e, tokens
                ),
                source: None,
            })?;
            let mut formatted = prettyplease::unparse(&file);

            // Add blank lines between top-level items for better readability
            let lines: Vec<&str> = formatted.lines().collect();
            let mut result_lines = Vec::new();

            for (i, line) in lines.iter().enumerate() {
                result_lines.push(*line);

                // Add blank line after closing braces that are at column 0 (top-level items)
                if *line == "}" && i + 1 < lines.len() && !lines[i + 1].is_empty() {
                    result_lines.push("");
                }

                // Add blank line after last pub mod declaration before structs/enums
                if line.starts_with("pub mod ") && i + 1 < lines.len() {
                    let next_line = lines[i + 1];
                    if !next_line.starts_with("pub mod ") && !next_line.is_empty() {
                        result_lines.push("");
                    }
                }
            }

            formatted = result_lines.join("\n");

            // Add header comment
            let header = if let Some(nsid) = nsid {
                format!(
                    "// @generated by jacquard-lexicon. DO NOT EDIT.\n//\n// Lexicon: {}\n//\n// This file was automatically generated from Lexicon schemas.\n// Any manual changes will be overwritten on the next regeneration.\n\n",
                    nsid
                )
            } else {
                "// @generated by jacquard-lexicon. DO NOT EDIT.\n//\n// This file was automatically generated from Lexicon schemas.\n// Any manual changes will be overwritten on the next regeneration.\n\n".to_string()
            };
            formatted = format!("{}{}", header, formatted);

            // Write file
            std::fs::write(&full_path, formatted).map_err(|e| CodegenError::Other {
                message: format!("Failed to write file {:?}: {}", full_path, e),
                source: None,
            })?;
        }

        Ok(())
    }

    /// Generate doc comment from description
    fn generate_doc_comment(&self, desc: Option<&jacquard_common::CowStr>) -> TokenStream {
        if let Some(desc) = desc {
            let doc = desc.as_ref();
            quote! { #[doc = #doc] }
        } else {
            quote! {}
        }
    }

    /// Generate params struct from XRPC query parameters
    fn generate_params_struct(
        &self,
        type_base: &str,
        params: &crate::lexicon::LexXrpcQueryParameter<'static>,
    ) -> Result<TokenStream> {
        use crate::lexicon::LexXrpcQueryParameter;
        match params {
            LexXrpcQueryParameter::Params(p) => self.generate_params_struct_inner(type_base, p),
        }
    }

    /// Generate params struct from XRPC procedure parameters (query string params)
    fn generate_params_struct_proc(
        &self,
        type_base: &str,
        params: &crate::lexicon::LexXrpcProcedureParameter<'static>,
    ) -> Result<TokenStream> {
        use crate::lexicon::LexXrpcProcedureParameter;
        match params {
            // For procedures, query string params still get "Params" suffix since the main struct is the input
            LexXrpcProcedureParameter::Params(p) => {
                let struct_name = format!("{}Params", type_base);
                let ident = syn::Ident::new(&struct_name, proc_macro2::Span::call_site());
                self.generate_params_struct_inner_with_name(&ident, p)
            }
        }
    }

    /// Generate params struct inner (shared implementation)
    fn generate_params_struct_inner(
        &self,
        type_base: &str,
        p: &crate::lexicon::LexXrpcParameters<'static>,
    ) -> Result<TokenStream> {
        let ident = syn::Ident::new(type_base, proc_macro2::Span::call_site());
        self.generate_params_struct_inner_with_name(&ident, p)
    }

    /// Generate params struct with custom name
    fn generate_params_struct_inner_with_name(
        &self,
        ident: &syn::Ident,
        p: &crate::lexicon::LexXrpcParameters<'static>,
    ) -> Result<TokenStream> {
        let required = p.required.as_ref().map(|r| r.as_slice()).unwrap_or(&[]);
        let mut fields = Vec::new();
        let mut default_fns = Vec::new();

        for (field_name, field_type) in &p.properties {
            let is_required = required.contains(field_name);
            let (field_tokens, default_fn) =
                self.generate_param_field_with_default("", field_name, field_type, is_required)?;
            fields.push(field_tokens);
            if let Some(fn_def) = default_fn {
                default_fns.push(fn_def);
            }
        }

        let doc = self.generate_doc_comment(p.description.as_ref());
        let needs_lifetime = self.params_need_lifetime(p);

        let derives = quote! {
            #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, bon::Builder, jacquard_derive::IntoStatic)]
            #[builder(start_fn = new)]
        };

        // Generate IntoStatic impl
        // let field_names: Vec<&str> = p.properties.keys().map(|k| k.as_str()).collect();
        // let type_name = ident.to_string();
        // let into_static_impl =
        //     self.generate_into_static_for_struct(&type_name, &field_names, needs_lifetime, false);

        if needs_lifetime {
            Ok(quote! {
                #(#default_fns)*

                #doc
                #derives
                #[serde(rename_all = "camelCase")]
                pub struct #ident<'a> {
                    #(#fields)*
                }

                //#into_static_impl
            })
        } else {
            Ok(quote! {
                #(#default_fns)*

                #doc
                #derives
                #[serde(rename_all = "camelCase")]
                pub struct #ident {
                    #(#fields)*
                }

                //#into_static_impl
            })
        }
    }

    /// Generate param field with serde default if present
    /// Returns (field_tokens, optional_default_function)
    fn generate_param_field_with_default(
        &self,
        nsid: &str,
        field_name: &str,
        field_type: &crate::lexicon::LexXrpcParametersProperty<'static>,
        is_required: bool,
    ) -> Result<(TokenStream, Option<TokenStream>)> {
        use crate::lexicon::LexXrpcParametersProperty;
        use heck::ToSnakeCase;

        // Get base field
        let base_field = self.generate_param_field(nsid, field_name, field_type, is_required)?;

        // Generate default function and attribute for required fields with defaults
        // For optional fields, just add doc comments
        let (doc_comment, serde_attr, default_fn) = if is_required {
            match field_type {
                LexXrpcParametersProperty::Boolean(b) if b.default.is_some() => {
                    let v = b.default.unwrap();
                    let fn_name = format!("_default_{}", field_name.to_snake_case());
                    let fn_ident = syn::Ident::new(&fn_name, proc_macro2::Span::call_site());
                    (
                        Some(format!("Defaults to `{}`", v)),
                        Some(quote! { #[serde(default = #fn_name)] }),
                        Some(quote! {
                            fn #fn_ident() -> bool { #v }
                        }),
                    )
                }
                LexXrpcParametersProperty::Integer(i) if i.default.is_some() => {
                    let v = i.default.unwrap();
                    let fn_name = format!("_default_{}", field_name.to_snake_case());
                    let fn_ident = syn::Ident::new(&fn_name, proc_macro2::Span::call_site());
                    (
                        Some(format!("Defaults to `{}`", v)),
                        Some(quote! { #[serde(default = #fn_name)] }),
                        Some(quote! {
                            fn #fn_ident() -> i64 { #v }
                        }),
                    )
                }
                LexXrpcParametersProperty::String(s) if s.default.is_some() => {
                    let v = s.default.as_ref().unwrap().as_ref();
                    let fn_name = format!("_default_{}", field_name.to_snake_case());
                    let fn_ident = syn::Ident::new(&fn_name, proc_macro2::Span::call_site());
                    (
                        Some(format!("Defaults to `\"{}\"`", v)),
                        Some(quote! { #[serde(default = #fn_name)] }),
                        Some(quote! {
                            fn #fn_ident() -> jacquard_common::CowStr<'static> {
                                jacquard_common::CowStr::from(#v)
                            }
                        }),
                    )
                }
                _ => (None, None, None),
            }
        } else {
            // Optional fields - just doc comments, no serde defaults
            let doc = match field_type {
                LexXrpcParametersProperty::Integer(i) => {
                    let mut parts = Vec::new();
                    if let Some(def) = i.default {
                        parts.push(format!("default: {}", def));
                    }
                    if let Some(min) = i.minimum {
                        parts.push(format!("min: {}", min));
                    }
                    if let Some(max) = i.maximum {
                        parts.push(format!("max: {}", max));
                    }
                    if !parts.is_empty() {
                        Some(format!("({})", parts.join(", ")))
                    } else {
                        None
                    }
                }
                LexXrpcParametersProperty::String(s) => {
                    let mut parts = Vec::new();
                    if let Some(def) = s.default.as_ref() {
                        parts.push(format!("default: \"{}\"", def.as_ref()));
                    }
                    if let Some(min) = s.min_length {
                        parts.push(format!("min length: {}", min));
                    }
                    if let Some(max) = s.max_length {
                        parts.push(format!("max length: {}", max));
                    }
                    if !parts.is_empty() {
                        Some(format!("({})", parts.join(", ")))
                    } else {
                        None
                    }
                }
                LexXrpcParametersProperty::Boolean(b) => {
                    b.default.map(|v| format!("(default: {})", v))
                }
                _ => None,
            };
            (doc, None, None)
        };

        let doc = doc_comment.as_ref().map(|d| quote! { #[doc = #d] });
        let field_with_attrs = match (doc, serde_attr) {
            (Some(doc), Some(attr)) => quote! {
                #doc
                #attr
                #base_field
            },
            (Some(doc), None) => quote! {
                #doc
                #base_field
            },
            (None, Some(attr)) => quote! {
                #attr
                #base_field
            },
            (None, None) => base_field,
        };

        Ok((field_with_attrs, default_fn))
    }

    /// Generate input struct from XRPC body
    fn generate_input_struct(
        &self,
        type_base: &str,
        body: &LexXrpcBody<'static>,
    ) -> Result<TokenStream> {
        let ident = syn::Ident::new(type_base, proc_macro2::Span::call_site());

        // Check if this is a binary body (no schema, just raw bytes)
        let is_binary_body = body.schema.is_none();

        let fields = if let Some(schema) = &body.schema {
            self.generate_body_fields("", type_base, schema, true)?
        } else {
            // Binary body: just a bytes field
            quote! {
                pub body: bytes::Bytes,
            }
        };

        let doc = self.generate_doc_comment(body.description.as_ref());

        // Binary bodies don't need #[lexicon] attribute or lifetime
        let struct_def = if is_binary_body {
            quote! {
                #doc
                #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, bon::Builder, jacquard_derive::IntoStatic)]
                #[builder(start_fn = new)]
                #[serde(rename_all = "camelCase")]
                pub struct #ident {
                    #fields
                }
            }
        } else {
            // Input structs with schemas: manually add extra_data field with #[builder(default)]
            // for bon compatibility. The #[lexicon] macro will see it exists and skip adding it.
            quote! {
                #doc
                #[jacquard_derive::lexicon]
                #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, bon::Builder, jacquard_derive::IntoStatic)]
                #[serde(rename_all = "camelCase")]
                #[builder(start_fn = new)]
                pub struct #ident<'a> {
                    #fields
                    #[serde(flatten)]
                    #[serde(borrow)]
                    #[builder(default)]
                    pub extra_data: ::std::collections::BTreeMap<
                        ::jacquard_common::smol_str::SmolStr,
                        ::jacquard_common::types::value::Data<'a>
                    >,
                }
            }
        };

        // Generate union types if schema is an Object
        let mut unions = Vec::new();
        if let Some(crate::lexicon::LexXrpcBodySchema::Object(obj)) = &body.schema {
            for (field_name, field_type) in &obj.properties {
                if let LexObjectProperty::Union(union) = field_type {
                    let union_name = format!("{}Record{}", type_base, field_name.to_pascal_case());
                    let refs: Vec<_> = union.refs.iter().cloned().collect();
                    let union_def = self.generate_union(&union_name, &refs, None, union.closed)?;
                    unions.push(union_def);
                }
            }
        }

        // Generate IntoStatic impl
        // let into_static_impl = if is_binary_body {
        //     // Binary bodies: simple clone of the Bytes field
        //     quote! {
        //         impl jacquard_common::IntoStatic for #ident {
        //             type Output = #ident;
        //             fn into_static(self) -> Self::Output {
        //                 self
        //             }
        //         }
        //     }
        // } else {
        //     let field_names: Vec<&str> = match &body.schema {
        //         Some(crate::lexicon::LexXrpcBodySchema::Object(obj)) => {
        //             obj.properties.keys().map(|k| k.as_str()).collect()
        //         }
        //         Some(_) => {
        //             // For Ref or Union schemas, there's just a single flattened field
        //             vec!["value"]
        //         }
        //         None => {
        //             // No schema means no fields, just extra_data
        //             vec![]
        //         }
        //     };
        //     self.generate_into_static_for_struct(type_base, &field_names, true, true)
        // };

        Ok(quote! {
            #struct_def
            #(#unions)*
            //#into_static_impl
        })
    }

    /// Generate output struct from XRPC body
    fn generate_output_struct(
        &self,
        type_base: &str,
        body: &LexXrpcBody<'static>,
    ) -> Result<TokenStream> {
        let struct_name = format!("{}Output", type_base);
        let ident = syn::Ident::new(&struct_name, proc_macro2::Span::call_site());

        let fields = if let Some(schema) = &body.schema {
            self.generate_body_fields("", &struct_name, schema, false)?
        } else {
            quote! {}
        };

        let doc = self.generate_doc_comment(body.description.as_ref());

        // Output structs always get a lifetime since they have the #[lexicon] attribute
        // which adds extra_data: BTreeMap<..., Data<'a>>
        let struct_def = quote! {
            #doc
            #[jacquard_derive::lexicon]
            #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic)]
            #[serde(rename_all = "camelCase")]
            pub struct #ident<'a> {
                #fields
            }
        };

        // Generate union types if schema is an Object
        let mut unions = Vec::new();
        if let Some(crate::lexicon::LexXrpcBodySchema::Object(obj)) = &body.schema {
            for (field_name, field_type) in &obj.properties {
                if let LexObjectProperty::Union(union) = field_type {
                    let union_name =
                        format!("{}Record{}", struct_name, field_name.to_pascal_case());
                    let refs: Vec<_> = union.refs.iter().cloned().collect();
                    let union_def = self.generate_union(&union_name, &refs, None, union.closed)?;
                    unions.push(union_def);
                }
            }
        }

        // Generate IntoStatic impl
        // let field_names: Vec<&str> = match &body.schema {
        //     Some(crate::lexicon::LexXrpcBodySchema::Object(obj)) => {
        //         obj.properties.keys().map(|k| k.as_str()).collect()
        //     }
        //     Some(_) => {
        //         // For Ref or Union schemas, there's just a single flattened field
        //         vec!["value"]
        //     }
        //     None => {
        //         // No schema means no fields, just extra_data
        //         vec![]
        //     }
        // };
        // let into_static_impl =
        //     self.generate_into_static_for_struct(&struct_name, &field_names, true, true);

        Ok(quote! {
            #struct_def
            #(#unions)*
            //#into_static_impl
        })
    }

    /// Generate fields from XRPC body schema
    fn generate_body_fields(
        &self,
        nsid: &str,
        parent_type_name: &str,
        schema: &LexXrpcBodySchema<'static>,
        is_builder: bool,
    ) -> Result<TokenStream> {
        use crate::lexicon::LexXrpcBodySchema;

        match schema {
            LexXrpcBodySchema::Object(obj) => {
                self.generate_object_fields(nsid, parent_type_name, obj, is_builder)
            }
            LexXrpcBodySchema::Ref(ref_type) => {
                let rust_type = self.ref_to_rust_type(&ref_type.r#ref)?;
                Ok(quote! {
                    #[serde(flatten)]
                    #[serde(borrow)]
                    pub value: #rust_type,
                })
            }
            LexXrpcBodySchema::Union(_union) => {
                let rust_type = quote! { jacquard_common::types::value::Data<'a> };
                Ok(quote! {
                    #[serde(flatten)]
                    #[serde(borrow)]
                    pub value: #rust_type,
                })
            }
        }
    }

    /// Generate a field for XRPC parameters
    fn generate_param_field(
        &self,
        _nsid: &str,
        field_name: &str,
        field_type: &crate::lexicon::LexXrpcParametersProperty<'static>,
        is_required: bool,
    ) -> Result<TokenStream> {
        use crate::lexicon::LexXrpcParametersProperty;

        let field_ident = make_ident(&field_name.to_snake_case());

        let (rust_type, needs_lifetime, is_cowstr) = match field_type {
            LexXrpcParametersProperty::Boolean(_) => (quote! { bool }, false, false),
            LexXrpcParametersProperty::Integer(_) => (quote! { i64 }, false, false),
            LexXrpcParametersProperty::String(s) => {
                let is_cowstr = s.format.is_none(); // CowStr for plain strings
                (
                    self.string_to_rust_type(s),
                    self.string_needs_lifetime(s),
                    is_cowstr,
                )
            }
            LexXrpcParametersProperty::Unknown(_) => (
                quote! { jacquard_common::types::value::Data<'a> },
                true,
                false,
            ),
            LexXrpcParametersProperty::Array(arr) => {
                let needs_lifetime = match &arr.items {
                    crate::lexicon::LexPrimitiveArrayItem::Boolean(_)
                    | crate::lexicon::LexPrimitiveArrayItem::Integer(_) => false,
                    crate::lexicon::LexPrimitiveArrayItem::String(s) => {
                        self.string_needs_lifetime(s)
                    }
                    crate::lexicon::LexPrimitiveArrayItem::Unknown(_) => true,
                };
                let item_type = match &arr.items {
                    crate::lexicon::LexPrimitiveArrayItem::Boolean(_) => quote! { bool },
                    crate::lexicon::LexPrimitiveArrayItem::Integer(_) => quote! { i64 },
                    crate::lexicon::LexPrimitiveArrayItem::String(s) => self.string_to_rust_type(s),
                    crate::lexicon::LexPrimitiveArrayItem::Unknown(_) => {
                        quote! { jacquard_common::types::value::Data<'a> }
                    }
                };
                (quote! { Vec<#item_type> }, needs_lifetime, false)
            }
        };

        let rust_type = if is_required {
            rust_type
        } else {
            quote! { std::option::Option<#rust_type> }
        };

        let mut attrs = Vec::new();

        if !is_required {
            attrs.push(quote! { #[serde(skip_serializing_if = "std::option::Option::is_none")] });
        }

        // Add serde(borrow) to all fields with lifetimes
        if needs_lifetime {
            attrs.push(quote! { #[serde(borrow)] });
        }

        // Add builder(into) for CowStr fields to allow String, &str, etc.
        if is_cowstr {
            attrs.push(quote! { #[builder(into)] });
        }

        Ok(quote! {
            #(#attrs)*
            pub #field_ident: #rust_type,
        })
    }

    /// Generate error enum from XRPC errors
    fn generate_error_enum(
        &self,
        type_base: &str,
        errors: &[LexXrpcError<'static>],
    ) -> Result<TokenStream> {
        let enum_name = format!("{}Error", type_base);
        let ident = syn::Ident::new(&enum_name, proc_macro2::Span::call_site());

        let mut variants = Vec::new();
        let mut display_arms = Vec::new();

        for error in errors {
            let variant_name = error.name.to_pascal_case();
            let variant_ident = syn::Ident::new(&variant_name, proc_macro2::Span::call_site());

            let error_name = error.name.as_ref();
            let doc = self.generate_doc_comment(error.description.as_ref());

            variants.push(quote! {
                #doc
                #[serde(rename = #error_name)]
                #variant_ident(std::option::Option<String>)
            });

            display_arms.push(quote! {
                Self::#variant_ident(msg) => {
                    write!(f, #error_name)?;
                    if let Some(msg) = msg {
                        write!(f, ": {}", msg)?;
                    }
                    Ok(())
                }
            });
        }

        // Generate IntoStatic impl
        let variant_info: Vec<(String, EnumVariantKind)> = errors
            .iter()
            .map(|e| (e.name.to_pascal_case(), EnumVariantKind::Tuple))
            .collect();
        let into_static_impl =
            self.generate_into_static_for_enum(&enum_name, &variant_info, true, true);

        Ok(quote! {
            #[jacquard_derive::open_union]
            #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, thiserror::Error, miette::Diagnostic)]
            #[serde(tag = "error", content = "message")]
            #[serde(bound(deserialize = "'de: 'a"))]
            pub enum #ident<'a> {
                #(#variants,)*
            }

            impl std::fmt::Display for #ident<'_> {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    match self {
                        #(#display_arms)*
                        Self::Unknown(err) => write!(f, "Unknown error: {:?}", err),
                    }
                }
            }

            #into_static_impl
        })
    }

    /// Generate enum for string with known values
    fn generate_known_values_enum(
        &self,
        nsid: &str,
        def_name: &str,
        string: &LexString<'static>,
    ) -> Result<TokenStream> {
        let type_name = self.def_to_type_name(nsid, def_name);
        let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());

        let known_values = string.known_values.as_ref().unwrap();
        let mut variants = Vec::new();
        let mut from_str_arms = Vec::new();
        let mut as_str_arms = Vec::new();

        for value in known_values {
            // Convert value to valid Rust identifier
            let value_str = value.as_ref();
            let variant_name = value_to_variant_name(value_str);
            let variant_ident = syn::Ident::new(&variant_name, proc_macro2::Span::call_site());

            variants.push(quote! {
                #variant_ident
            });

            from_str_arms.push(quote! {
                #value_str => Self::#variant_ident
            });

            as_str_arms.push(quote! {
                Self::#variant_ident => #value_str
            });
        }

        let doc = self.generate_doc_comment(string.description.as_ref());

        // Generate IntoStatic impl
        let variant_info: Vec<(String, EnumVariantKind)> = known_values
            .iter()
            .map(|value| {
                let variant_name = value_to_variant_name(value.as_ref());
                (variant_name, EnumVariantKind::Unit)
            })
            .chain(std::iter::once((
                "Other".to_string(),
                EnumVariantKind::Tuple,
            )))
            .collect();
        let into_static_impl =
            self.generate_into_static_for_enum(&type_name, &variant_info, true, false);

        Ok(quote! {
            #doc
            #[derive(Debug, Clone, PartialEq, Eq, Hash)]
            pub enum #ident<'a> {
                #(#variants,)*
                Other(jacquard_common::CowStr<'a>),
            }

            impl<'a> #ident<'a> {
                pub fn as_str(&self) -> &str {
                    match self {
                        #(#as_str_arms,)*
                        Self::Other(s) => s.as_ref(),
                    }
                }
            }

            impl<'a> From<&'a str> for #ident<'a> {
                fn from(s: &'a str) -> Self {
                    match s {
                        #(#from_str_arms,)*
                        _ => Self::Other(jacquard_common::CowStr::from(s)),
                    }
                }
            }

            impl<'a> From<String> for #ident<'a> {
                fn from(s: String) -> Self {
                    match s.as_str() {
                        #(#from_str_arms,)*
                        _ => Self::Other(jacquard_common::CowStr::from(s)),
                    }
                }
            }

            impl<'a> AsRef<str> for #ident<'a> {
                fn as_ref(&self) -> &str {
                    self.as_str()
                }
            }

            impl<'a> serde::Serialize for #ident<'a> {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: serde::Serializer,
                {
                    serializer.serialize_str(self.as_str())
                }
            }

            impl<'de, 'a> serde::Deserialize<'de> for #ident<'a>
            where
                'de: 'a,
            {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    let s = <&'de str>::deserialize(deserializer)?;
                    Ok(Self::from(s))
                }
            }

            #into_static_impl
        })
    }

    /// Generate enum for integer with enum values
    fn generate_integer_enum(
        &self,
        nsid: &str,
        def_name: &str,
        integer: &LexInteger<'static>,
    ) -> Result<TokenStream> {
        let type_name = self.def_to_type_name(nsid, def_name);
        let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());

        let enum_values = integer.r#enum.as_ref().unwrap();
        let mut variants = Vec::new();
        let mut from_i64_arms = Vec::new();
        let mut to_i64_arms = Vec::new();

        for value in enum_values {
            let variant_name = format!("Value{}", value.abs());
            let variant_ident = syn::Ident::new(&variant_name, proc_macro2::Span::call_site());

            variants.push(quote! {
                #[serde(rename = #value)]
                #variant_ident
            });

            from_i64_arms.push(quote! {
                #value => Self::#variant_ident
            });

            to_i64_arms.push(quote! {
                Self::#variant_ident => #value
            });
        }

        let doc = self.generate_doc_comment(integer.description.as_ref());

        Ok(quote! {
            #doc
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            pub enum #ident {
                #(#variants,)*
                #[serde(untagged)]
                Other(i64),
            }

            impl #ident {
                pub fn as_i64(&self) -> i64 {
                    match self {
                        #(#to_i64_arms,)*
                        Self::Other(n) => *n,
                    }
                }
            }

            impl From<i64> for #ident {
                fn from(n: i64) -> Self {
                    match n {
                        #(#from_i64_arms,)*
                        _ => Self::Other(n),
                    }
                }
            }

            impl serde::Serialize for #ident {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: serde::Serializer,
                {
                    serializer.serialize_i64(self.as_i64())
                }
            }

            impl<'de> serde::Deserialize<'de> for #ident {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    let n = i64::deserialize(deserializer)?;
                    Ok(Self::from(n))
                }
            }
        })
    }

    /// Generate XrpcRequest trait impl for a query or procedure
    fn generate_xrpc_request_impl(
        &self,
        nsid: &str,
        type_base: &str,
        method: TokenStream,
        output_encoding: &str,
        has_params: bool,
        params_has_lifetime: bool,
        has_output: bool,
        has_errors: bool,
        is_binary_input: bool,
    ) -> Result<TokenStream> {
        let output_type = if has_output {
            let output_ident = syn::Ident::new(
                &format!("{}Output", type_base),
                proc_macro2::Span::call_site(),
            );
            quote! { #output_ident<'de> }
        } else {
            quote! { () }
        };

        let error_type = if has_errors {
            let error_ident = syn::Ident::new(
                &format!("{}Error", type_base),
                proc_macro2::Span::call_site(),
            );
            quote! { #error_ident<'de> }
        } else {
            quote! { jacquard_common::xrpc::GenericError<'de> }
        };

        // Generate the response type that implements XrpcResp
        let response_ident = syn::Ident::new(
            &format!("{}Response", type_base),
            proc_macro2::Span::call_site(),
        );

        let response_type = quote! {
            #[doc = "Response type for "]
            #[doc = #nsid]
            pub struct #response_ident;

            impl jacquard_common::xrpc::XrpcResp for #response_ident {
                const NSID: &'static str = #nsid;
                const ENCODING: &'static str = #output_encoding;
                type Output<'de> = #output_type;
                type Err<'de> = #error_type;
            }
        };

        // Generate encode_body() method for binary inputs
        let encode_body_method = if is_binary_input {
            quote! {
                fn encode_body(&self) -> Result<Vec<u8>, jacquard_common::xrpc::EncodeError> {
                    Ok(self.body.to_vec())
                }
            }
        } else {
            quote! {}
        };

        // Generate decode_body() method for binary inputs
        let decode_body_method = if is_binary_input {
            quote! {
                fn decode_body(
                    body: &'de [u8],
                ) -> Result<Box<Self>, jacquard_common::error::DecodeError> {
                    Ok(Box::new(Self {
                        body: bytes::Bytes::copy_from_slice(body),
                    }))
                }
            }
        } else {
            quote! {}
        };

        if has_params {
            // Implement on the params/input struct itself
            let request_ident = syn::Ident::new(type_base, proc_macro2::Span::call_site());
            let impl_target = if params_has_lifetime {
                quote! { #request_ident<'de> }
            } else {
                quote! { #request_ident }
            };

            Ok(quote! {
                #response_type

                impl<'de> jacquard_common::xrpc::XrpcRequest<'de> for #impl_target {
                    const NSID: &'static str = #nsid;
                    const METHOD: jacquard_common::xrpc::XrpcMethod = #method;

                    type Response = #response_ident;

                    #encode_body_method
                    #decode_body_method
                }
            })
        } else {
            // No params - generate a marker struct
            let request_ident = syn::Ident::new(type_base, proc_macro2::Span::call_site());

            Ok(quote! {
                /// XRPC request marker type
                #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
                pub struct #request_ident;

                #response_type

                impl<'de> jacquard_common::xrpc::XrpcRequest<'de> for #request_ident {
                    const NSID: &'static str = #nsid;
                    const METHOD: jacquard_common::xrpc::XrpcMethod = #method;

                    type Response = #response_ident;
                }
            })
        }
    }

    /// Generate a union enum
    pub fn generate_union(
        &self,
        union_name: &str,
        refs: &[jacquard_common::CowStr<'static>],
        description: Option<&str>,
        closed: Option<bool>,
    ) -> Result<TokenStream> {
        let enum_ident = syn::Ident::new(union_name, proc_macro2::Span::call_site());

        let mut variants = Vec::new();
        for ref_str in refs {
            // Parse ref to get NSID and def name
            let (ref_nsid, ref_def) = if let Some((nsid, fragment)) = ref_str.split_once('#') {
                (nsid, fragment)
            } else {
                (ref_str.as_ref(), "main")
            };

            // Skip unknown refs - they'll be handled by Unknown variant
            if !self.corpus.ref_exists(ref_str.as_ref()) {
                continue;
            }

            // Generate variant name from def name (or last NSID segment if main)
            // For non-main refs, include the last NSID segment to avoid collisions
            // e.g. app.bsky.embed.images#view -> ImagesView
            //      app.bsky.embed.video#view -> VideoView
            let variant_name = if ref_def == "main" {
                ref_nsid.split('.').last().unwrap().to_pascal_case()
            } else {
                let last_segment = ref_nsid.split('.').last().unwrap().to_pascal_case();
                format!("{}{}", last_segment, ref_def.to_pascal_case())
            };
            let variant_ident = syn::Ident::new(&variant_name, proc_macro2::Span::call_site());

            // Get the Rust type for this ref
            let rust_type = self.ref_to_rust_type(ref_str.as_ref())?;

            // Add serde rename for the full NSID
            let ref_str_literal = ref_str.as_ref();
            variants.push(quote! {
                #[serde(rename = #ref_str_literal)]
                #variant_ident(Box<#rust_type>)
            });
        }

        let doc = description
            .map(|d| quote! { #[doc = #d] })
            .unwrap_or_else(|| quote! {});

        // Only add open_union if not closed
        let is_open = closed != Some(true);

        // Generate IntoStatic impl
        // let variant_info: Vec<(String, EnumVariantKind)> = refs
        //     .iter()
        //     .filter_map(|ref_str| {
        //         // Skip unknown refs
        //         if !self.corpus.ref_exists(ref_str.as_ref()) {
        //             return None;
        //         }

        //         let (ref_nsid, ref_def) = if let Some((nsid, fragment)) = ref_str.split_once('#') {
        //             (nsid, fragment)
        //         } else {
        //             (ref_str.as_ref(), "main")
        //         };

        //         let variant_name = if ref_def == "main" {
        //             ref_nsid.split('.').last().unwrap().to_pascal_case()
        //         } else {
        //             let last_segment = ref_nsid.split('.').last().unwrap().to_pascal_case();
        //             format!("{}{}", last_segment, ref_def.to_pascal_case())
        //         };
        //         Some((variant_name, EnumVariantKind::Tuple))
        //     })
        //     .collect();
        // let into_static_impl =
        //     self.generate_into_static_for_enum(union_name, &variant_info, true, is_open);

        if is_open {
            Ok(quote! {
                #doc
                #[jacquard_derive::open_union]
                #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic)]
                #[serde(tag = "$type")]
                #[serde(bound(deserialize = "'de: 'a"))]
                pub enum #enum_ident<'a> {
                    #(#variants,)*
                }

                //#into_static_impl
            })
        } else {
            Ok(quote! {
                #doc
                #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic)]
                #[serde(tag = "$type")]
                #[serde(bound(deserialize = "'de: 'a"))]
                pub enum #enum_ident<'a> {
                    #(#variants,)*
                }

                //#into_static_impl
            })
        }
    }

    /// Generate IntoStatic impl for a struct
    #[allow(dead_code)]
    fn generate_into_static_for_struct(
        &self,
        type_name: &str,
        field_names: &[&str],
        has_lifetime: bool,
        has_extra_data: bool,
    ) -> TokenStream {
        let ident = syn::Ident::new(type_name, proc_macro2::Span::call_site());

        let field_idents: Vec<_> = field_names
            .iter()
            .map(|name| make_ident(&name.to_snake_case()))
            .collect();

        if has_lifetime {
            let field_conversions: Vec<_> = field_idents
                .iter()
                .map(|field| quote! { #field: self.#field.into_static() })
                .collect();

            let extra_data_conversion = if has_extra_data {
                quote! { extra_data: self.extra_data.into_static(), }
            } else {
                quote! {}
            };

            quote! {
                impl jacquard_common::IntoStatic for #ident<'_> {
                    type Output = #ident<'static>;

                    fn into_static(self) -> Self::Output {
                        #ident {
                            #(#field_conversions,)*
                            #extra_data_conversion
                        }
                    }
                }
            }
        } else {
            quote! {
                impl jacquard_common::IntoStatic for #ident {
                    type Output = #ident;

                    fn into_static(self) -> Self::Output {
                        self
                    }
                }
            }
        }
    }

    /// Generate IntoStatic impl for an enum
    fn generate_into_static_for_enum(
        &self,
        type_name: &str,
        variant_info: &[(String, EnumVariantKind)],
        has_lifetime: bool,
        is_open: bool,
    ) -> TokenStream {
        let ident = syn::Ident::new(type_name, proc_macro2::Span::call_site());

        if has_lifetime {
            let variant_conversions: Vec<_> = variant_info
                .iter()
                .map(|(variant_name, kind)| {
                    let variant_ident = syn::Ident::new(variant_name, proc_macro2::Span::call_site());
                    match kind {
                        EnumVariantKind::Unit => {
                            quote! {
                                #ident::#variant_ident => #ident::#variant_ident
                            }
                        }
                        EnumVariantKind::Tuple => {
                            quote! {
                                #ident::#variant_ident(v) => #ident::#variant_ident(v.into_static())
                            }
                        }
                        EnumVariantKind::Struct(fields) => {
                            let field_idents: Vec<_> = fields
                                .iter()
                                .map(|f| make_ident(&f.to_snake_case()))
                                .collect();
                            let field_conversions: Vec<_> = field_idents
                                .iter()
                                .map(|f| quote! { #f: #f.into_static() })
                                .collect();
                            quote! {
                                #ident::#variant_ident { #(#field_idents,)* } => #ident::#variant_ident {
                                    #(#field_conversions,)*
                                }
                            }
                        }
                    }
                })
                .collect();

            let unknown_conversion = if is_open {
                quote! {
                    #ident::Unknown(v) => #ident::Unknown(v.into_static()),
                }
            } else {
                quote! {}
            };

            quote! {
                impl jacquard_common::IntoStatic for #ident<'_> {
                    type Output = #ident<'static>;

                    fn into_static(self) -> Self::Output {
                        match self {
                            #(#variant_conversions,)*
                            #unknown_conversion
                        }
                    }
                }
            }
        } else {
            quote! {
                impl jacquard_common::IntoStatic for #ident {
                    type Output = #ident;

                    fn into_static(self) -> Self::Output {
                        self
                    }
                }
            }
        }
    }
}

/// Enum variant kind for IntoStatic generation
#[derive(Debug, Clone)]
#[allow(dead_code)]
enum EnumVariantKind {
    Unit,
    Tuple,
    Struct(Vec<String>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_record() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus.get("app.bsky.feed.post").expect("get post");
        let def = doc.defs.get("main").expect("get main def");

        let tokens = codegen
            .generate_def("app.bsky.feed.post", "main", def)
            .expect("generate");

        // Format and print for inspection
        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Check basic structure
        assert!(formatted.contains("struct Post"));
        assert!(formatted.contains("pub text"));
        assert!(formatted.contains("CowStr<'a>"));
    }

    #[test]
    fn test_generate_union() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        // Create a union with embed types
        let refs = vec![
            "app.bsky.embed.images".into(),
            "app.bsky.embed.video".into(),
            "app.bsky.embed.external".into(),
        ];

        let tokens = codegen
            .generate_union("RecordEmbed", &refs, Some("Post embed union"), None)
            .expect("generate union");

        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Check structure
        assert!(formatted.contains("enum RecordEmbed"));
        assert!(formatted.contains("Images"));
        assert!(formatted.contains("Video"));
        assert!(formatted.contains("External"));
        assert!(formatted.contains("#[serde(tag = \"$type\")]"));
        assert!(formatted.contains("#[jacquard_derive::open_union]"));
    }

    #[test]
    fn test_generate_query() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus
            .get("app.bsky.feed.getAuthorFeed")
            .expect("get getAuthorFeed");
        let def = doc.defs.get("main").expect("get main def");

        let tokens = codegen
            .generate_def("app.bsky.feed.getAuthorFeed", "main", def)
            .expect("generate");

        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Check structure
        assert!(formatted.contains("struct GetAuthorFeed"));
        assert!(formatted.contains("struct GetAuthorFeedOutput"));
        assert!(formatted.contains("enum GetAuthorFeedError"));
        assert!(formatted.contains("pub actor"));
        assert!(formatted.contains("pub limit"));
        assert!(formatted.contains("pub cursor"));
        assert!(formatted.contains("pub feed"));
        assert!(formatted.contains("BlockedActor"));
        assert!(formatted.contains("BlockedByActor"));
    }

    #[test]
    fn test_generate_known_values_enum() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus
            .get("com.atproto.label.defs")
            .expect("get label defs");
        let def = doc.defs.get("labelValue").expect("get labelValue def");

        let tokens = codegen
            .generate_def("com.atproto.label.defs", "labelValue", def)
            .expect("generate");

        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Check structure
        assert!(formatted.contains("enum LabelValue"));
        assert!(formatted.contains("Hide"));
        assert!(formatted.contains("NoPromote"));
        assert!(formatted.contains("Warn"));
        assert!(formatted.contains("DmcaViolation"));
        assert!(formatted.contains("Other(jacquard_common::CowStr"));
        assert!(formatted.contains("impl<'a> From<&'a str>"));
        assert!(formatted.contains("fn as_str(&self)"));
    }

    #[test]
    fn test_nsid_to_file_path() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        // Regular paths
        assert_eq!(
            codegen.nsid_to_file_path("app.bsky.feed.post"),
            std::path::PathBuf::from("app_bsky/feed/post.rs")
        );

        assert_eq!(
            codegen.nsid_to_file_path("app.bsky.feed.getAuthorFeed"),
            std::path::PathBuf::from("app_bsky/feed/get_author_feed.rs")
        );

        // Defs paths - should go in parent
        assert_eq!(
            codegen.nsid_to_file_path("com.atproto.label.defs"),
            std::path::PathBuf::from("com_atproto/label.rs")
        );
    }

    #[test]
    fn test_write_to_disk() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "test_generated");

        let tmp_dir =
            tempfile::tempdir().expect("should be able to create temp directory for output");
        let output_dir = std::path::PathBuf::from(tmp_dir.path());

        // Clean up any previous test output
        let _ = std::fs::remove_dir_all(&output_dir);

        // Generate and write
        codegen.write_to_disk(&output_dir).expect("write to disk");

        // Verify some files were created
        assert!(output_dir.join("app_bsky/feed/post.rs").exists());
        assert!(output_dir.join("app_bsky/feed/get_author_feed.rs").exists());
        assert!(output_dir.join("com_atproto/label.rs").exists());

        // Verify module files were created
        assert!(output_dir.join("lib.rs").exists());
        assert!(output_dir.join("app_bsky.rs").exists());

        // Read and verify post.rs contains expected content
        let post_content = std::fs::read_to_string(output_dir.join("app_bsky/feed/post.rs"))
            .expect("read post.rs");
        assert!(post_content.contains("pub struct Post"));
        assert!(post_content.contains("jacquard_common"));
    }
}
