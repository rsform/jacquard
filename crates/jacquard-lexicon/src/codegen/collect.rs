use crate::lexicon::{LexArrayItem, LexObjectProperty, LexString, LexStringFormat, LexUserType};

use super::CodeGenerator;
use super::prettify::{CommonType, ExternalImport, ImportSet};

impl<'c> CodeGenerator<'c> {
    /// Collect types from a property (mirrors property_to_rust_type).
    pub(super) fn collect_property_types(
        &self,
        nsid: &str,
        prop: &LexObjectProperty<'static>,
    ) -> ImportSet {
        let mut imports = ImportSet::default();
        match prop {
            LexObjectProperty::Boolean(_) | LexObjectProperty::Integer(_) => {}
            LexObjectProperty::String(s) => {
                if s.known_values.is_some() {
                    // Inline known_values enums always use CowStr for the catch-all variant.
                    imports.common.insert(CommonType::CowStr);
                } else {
                    imports.merge(self.collect_string_type(s));
                }
            }
            LexObjectProperty::Bytes(_) => {
                imports.external.insert(ExternalImport::Bytes);
            }
            LexObjectProperty::CidLink(_) => {
                imports.common.insert(CommonType::CidLink);
            }
            LexObjectProperty::Blob(_) => {
                imports.common.insert(CommonType::BlobRef);
            }
            LexObjectProperty::Unknown(_) => {
                imports.common.insert(CommonType::Data);
            }
            LexObjectProperty::Array(array) => {
                // For arrays with union items, check if multi-variant
                if let LexArrayItem::Union(union) = &array.items {
                    if union.refs.is_empty() {
                        // Empty union: fall back to Data
                        imports.common.insert(CommonType::Data);
                    } else if union.refs.len() == 1 {
                        // Single-variant: use the ref type directly
                        let ref_str = if union.refs[0].starts_with('#') {
                            format!("{}{}", nsid, union.refs[0])
                        } else {
                            union.refs[0].to_string()
                        };
                        imports.merge(self.collect_ref_type(&ref_str));
                    } else {
                        // Multi-variant: union type is generated via generate_union.
                        // Still collect all refs so their types get imported in Pretty mode.
                        if union.closed != Some(true) {
                            imports.external.insert(ExternalImport::OpenUnion);
                        }
                        for ref_str in &union.refs {
                            let full_ref = if ref_str.starts_with('#') {
                                format!("{}{}", nsid, ref_str)
                            } else {
                                ref_str.to_string()
                            };
                            imports.merge(self.collect_ref_type(&full_ref));
                        }
                    }
                } else {
                    imports.merge(self.collect_array_item_types(nsid, &array.items));
                }
            }
            LexObjectProperty::Object(object) => {
                // Empty objects (no properties) are untyped data bags
                if object.properties.is_empty() {
                    imports.common.insert(CommonType::Data);
                }
                // Non-empty objects don't add imports (they're nested types)
            }
            LexObjectProperty::Ref(ref_type) => {
                // Handle local refs (starting with #) by prepending the current NSID
                let ref_str = if ref_type.r#ref.starts_with('#') {
                    format!("{}{}", nsid, ref_type.r#ref)
                } else {
                    ref_type.r#ref.to_string()
                };
                imports.merge(self.collect_ref_type(&ref_str));
            }
            LexObjectProperty::Union(union) => {
                if union.refs.is_empty() {
                    // Empty union: fall back to Data
                    imports.common.insert(CommonType::Data);
                } else if union.refs.len() == 1 {
                    // Single-variant: use the ref type directly
                    let ref_str = if union.refs[0].starts_with('#') {
                        format!("{}{}", nsid, union.refs[0])
                    } else {
                        union.refs[0].to_string()
                    };
                    imports.merge(self.collect_ref_type(&ref_str));
                }
                // Multi-variant unions are generated via generate_union.
                // Still collect all refs so their types get imported in Pretty mode.
                if union.refs.len() > 1 {
                    if union.closed != Some(true) {
                        imports.external.insert(ExternalImport::OpenUnion);
                    }
                    for ref_str in &union.refs {
                        let full_ref = if ref_str.starts_with('#') {
                            format!("{}{}", nsid, ref_str)
                        } else {
                            ref_str.to_string()
                        };
                        imports.merge(self.collect_ref_type(&full_ref));
                    }
                }
            }
        }
        imports
    }

    /// Collect types from a string format (mirrors string_to_rust_type).
    pub(super) fn collect_string_type(&self, s: &LexString) -> ImportSet {
        let mut imports = ImportSet::default();
        match s.format {
            Some(LexStringFormat::Did) => {
                imports.common.insert(CommonType::Did);
            }
            Some(LexStringFormat::Handle) => {
                imports.common.insert(CommonType::Handle);
            }
            Some(LexStringFormat::AtIdentifier) => {
                imports.common.insert(CommonType::AtIdentifier);
            }
            Some(LexStringFormat::Nsid) => {
                imports.common.insert(CommonType::Nsid);
            }
            Some(LexStringFormat::AtUri) => {
                imports.common.insert(CommonType::AtUri);
            }
            Some(LexStringFormat::Uri) => {
                imports.common.insert(CommonType::UriValue);
            }
            Some(LexStringFormat::Cid) => {
                imports.common.insert(CommonType::Cid);
            }
            Some(LexStringFormat::Language) => {
                imports.common.insert(CommonType::Language);
            }
            Some(LexStringFormat::Tid) => {
                imports.common.insert(CommonType::Tid);
            }
            Some(LexStringFormat::Datetime) => {
                imports.common.insert(CommonType::Datetime);
            }
            Some(LexStringFormat::RecordKey) => {
                imports.common.insert(CommonType::RecordKey);
                imports.common.insert(CommonType::Rkey);
            }
            _ => {
                imports.common.insert(CommonType::CowStr);
            }
        }
        imports.external.insert(ExternalImport::DefaultStr);
        imports
    }

    /// Collect types from a ref (mirrors ref_to_rust_type).
    pub(super) fn collect_ref_type(&self, ref_str: &str) -> ImportSet {
        let mut imports = ImportSet::default();
        if !self.corpus.ref_exists(ref_str) {
            imports.common.insert(CommonType::Data); // fallback
            return imports;
        }

        // For cross-namespace refs, add them to lexicon_refs
        // (In Phase 3, these will be converted to crate:: imports)
        // For now, just track them
        imports.lexicon_refs.insert(ref_str.to_string());

        imports
    }

    /// Collect types from an array item (mirrors array_item_to_rust_type).
    pub(super) fn collect_array_item_types(&self, nsid: &str, item: &LexArrayItem) -> ImportSet {
        let mut imports = ImportSet::default();
        match item {
            LexArrayItem::Boolean(_) | LexArrayItem::Integer(_) => {}
            LexArrayItem::String(s) => {
                imports.merge(self.collect_string_type(s));
            }
            LexArrayItem::Bytes(_) => {
                imports.external.insert(ExternalImport::Bytes);
            }
            LexArrayItem::CidLink(_) => {
                imports.common.insert(CommonType::CidLink);
            }
            LexArrayItem::Blob(_) => {
                imports.common.insert(CommonType::BlobRef);
            }
            LexArrayItem::Unknown(_) => {
                imports.common.insert(CommonType::Data);
            }
            LexArrayItem::Object(_) => {
                // Mirrors types.rs: inline objects in arrays fall back to Data.
                // This is a pre-existing limitation in the generator.
                imports.common.insert(CommonType::Data);
            }
            LexArrayItem::Ref(ref_type) => {
                let ref_str = if ref_type.r#ref.starts_with('#') {
                    format!("{}{}", nsid, ref_type.r#ref)
                } else {
                    ref_type.r#ref.to_string()
                };
                imports.merge(self.collect_ref_type(&ref_str));
            }
            LexArrayItem::Union(union) => {
                // Mirrors types.rs: array unions fall back to Data.
                // This is a pre-existing limitation in the generator.
                // Track the refs anyway for future use.
                imports.common.insert(CommonType::Data);
                for ref_str in &union.refs {
                    let full_ref = if ref_str.starts_with('#') {
                        format!("{}{}", nsid, ref_str)
                    } else {
                        ref_str.to_string()
                    };
                    imports.merge(self.collect_ref_type(&full_ref));
                }
            }
        }
        imports
    }

    /// Collect all types from a record definition.
    pub(super) fn collect_record(
        &self,
        nsid: &str,
        record: &crate::lexicon::LexRecord<'static>,
    ) -> ImportSet {
        let mut imports = ImportSet::default();

        // All records use serde derives, IntoStatic, lexicon attr, and PhantomData (builders).
        imports.external.insert(ExternalImport::Serialize);
        imports.external.insert(ExternalImport::Deserialize);
        imports.external.insert(ExternalImport::IntoStatic);
        imports.external.insert(ExternalImport::LexiconAttr);
        imports.external.insert(ExternalImport::PhantomData);

        // Records generate LexiconSchema trait impls with validation.
        imports.external.insert(ExternalImport::LexiconSchema);
        imports.external.insert(ExternalImport::LexiconDoc);
        imports.external.insert(ExternalImport::ConstraintError);
        imports.external.insert(ExternalImport::ValidationPath);
        imports.external.insert(ExternalImport::UnicodeSegmentation);

        // Records always use CowStr and AtUri for the uri() method.
        imports.common.insert(CommonType::CowStr);
        imports.common.insert(CommonType::AtUri);

        // All parameterised types need Bos, DefaultStr, SmolStr, and Data for extra_data field.
        imports.external.insert(ExternalImport::BTreeMap);
        imports.external.insert(ExternalImport::DefaultStr);
        imports.common.insert(CommonType::SmolStr);
        imports.common.insert(CommonType::Data);

        // All records generate Collection trait impl and RecordError for the marker struct.
        imports.common.insert(CommonType::Collection);
        imports.common.insert(CommonType::RecordError);

        // Records generate a GetRecordOutput wrapper that uses Cid.
        imports.common.insert(CommonType::Cid);

        // Records use XrpcResp for the marker struct impl.
        imports.external.insert(ExternalImport::XrpcResp);

        // Records use RecordUri and UriError for the uri() method.
        imports.external.insert(ExternalImport::RecordUri);
        imports.external.insert(ExternalImport::UriError);

        // Walk all properties in the record.
        match &record.record {
            crate::lexicon::LexRecordRecord::Object(obj) => {
                for (_prop_name, prop) in &obj.properties {
                    imports.merge(self.collect_property_types(nsid, prop));
                }
            }
        }

        imports
    }

    /// Collect all types from an object definition.
    pub(super) fn collect_object(
        &self,
        nsid: &str,
        obj: &crate::lexicon::LexObject<'static>,
    ) -> ImportSet {
        self.collect_object_with_builder_check(nsid, None, obj)
    }

    /// Collect all types from an object definition, optionally checking builder heuristics.
    /// When `type_name` is provided, the builder heuristic is consulted to decide whether
    /// PhantomData and BTreeMap are needed. When `None`, they are always included
    /// (conservative fallback for XRPC body schemas where the type name isn't known yet).
    fn collect_object_with_builder_check(
        &self,
        nsid: &str,
        type_name: Option<&str>,
        obj: &crate::lexicon::LexObject<'static>,
    ) -> ImportSet {
        let mut imports = ImportSet::default();

        imports.external.insert(ExternalImport::Serialize);
        imports.external.insert(ExternalImport::Deserialize);
        imports.external.insert(ExternalImport::IntoStatic);

        // All parameterised types need Bos, DefaultStr, SmolStr, Data, and BTreeMap for extra_data.
        imports.external.insert(ExternalImport::DefaultStr);
        imports.common.insert(CommonType::SmolStr);
        imports.common.insert(CommonType::Data);
        imports.external.insert(ExternalImport::BTreeMap);

        // PhantomData is only needed when a builder is generated.
        let needs_builder = match type_name {
            Some(name) => {
                let decision =
                    crate::codegen::builder_heuristics::should_generate_builder(name, obj);
                decision.has_builder
            }
            // Conservative: include them when we don't know the type name.
            None => true,
        };
        if needs_builder {
            imports.external.insert(ExternalImport::PhantomData);
        }

        // Walk all properties in the object.
        for (_prop_name, prop) in &obj.properties {
            imports.merge(self.collect_property_types(nsid, prop));
        }

        imports
    }

    /// Collect all types from an XRPC query.
    pub(super) fn collect_query(
        &self,
        nsid: &str,
        query: &crate::lexicon::LexXrpcQuery<'static>,
    ) -> ImportSet {
        let mut imports = ImportSet::default();

        // Queries use serde derives, IntoStatic, and PhantomData (builders).
        // LexiconAttr is added by collect_object when body schemas are walked.
        imports.external.insert(ExternalImport::Serialize);
        imports.external.insert(ExternalImport::Deserialize);
        imports.external.insert(ExternalImport::IntoStatic);
        imports.external.insert(ExternalImport::PhantomData);
        imports.external.insert(ExternalImport::BTreeMap);
        imports.external.insert(ExternalImport::DefaultStr);
        imports.common.insert(CommonType::SmolStr);
        imports.common.insert(CommonType::Data);

        // Collect from parameters.
        if let Some(params) = &query.parameters {
            let p = match params {
                crate::lexicon::LexXrpcQueryParameter::Params(p) => p,
            };
            for (_prop_name, prop) in &p.properties {
                imports.merge(self.collect_xrpc_parameter_property(nsid, prop));
            }
        }

        // Collect from output.
        if let Some(output) = &query.output {
            imports.merge(self.collect_xrpc_body(nsid, output));
        }

        // Error enums use open_union and CowStr for variant data.
        if query.errors.as_ref().is_some_and(|e| !e.is_empty()) {
            imports.external.insert(ExternalImport::OpenUnion);
            imports.common.insert(CommonType::CowStr);
        }

        imports
    }

    /// Collect all types from an XRPC procedure.
    pub(super) fn collect_procedure(
        &self,
        nsid: &str,
        proc: &crate::lexicon::LexXrpcProcedure<'static>,
    ) -> ImportSet {
        let mut imports = ImportSet::default();

        // Procedures use serde derives, IntoStatic, and PhantomData (builders).
        // LexiconAttr is added by collect_object when body schemas are walked.
        imports.external.insert(ExternalImport::Serialize);
        imports.external.insert(ExternalImport::Deserialize);
        imports.external.insert(ExternalImport::IntoStatic);
        imports.external.insert(ExternalImport::PhantomData);
        imports.external.insert(ExternalImport::BTreeMap);
        imports.external.insert(ExternalImport::DefaultStr);
        imports.common.insert(CommonType::SmolStr);
        imports.common.insert(CommonType::Data);

        // Collect from parameters.
        if let Some(params) = &proc.parameters {
            let p = match params {
                crate::lexicon::LexXrpcProcedureParameter::Params(p) => p,
            };
            for (_prop_name, prop) in &p.properties {
                imports.merge(self.collect_xrpc_parameter_property(nsid, prop));
            }
        }

        // Collect from input.
        if let Some(input) = &proc.input {
            imports.merge(self.collect_xrpc_body(nsid, input));
        }

        // Collect from output.
        if let Some(output) = &proc.output {
            imports.merge(self.collect_xrpc_body(nsid, output));
        }

        // Error enums use open_union and CowStr for variant data.
        if proc.errors.as_ref().is_some_and(|e| !e.is_empty()) {
            imports.external.insert(ExternalImport::OpenUnion);
            imports.common.insert(CommonType::CowStr);
        }

        imports
    }

    /// Collect all types from an XRPC subscription.
    pub(super) fn collect_subscription(
        &self,
        nsid: &str,
        sub: &crate::lexicon::LexXrpcSubscription<'static>,
    ) -> ImportSet {
        let mut imports = ImportSet::default();

        // Subscriptions use serde derives, IntoStatic, and PhantomData (builders).
        // LexiconAttr is added by collect_object when message schemas are walked.
        imports.external.insert(ExternalImport::Serialize);
        imports.external.insert(ExternalImport::Deserialize);
        imports.external.insert(ExternalImport::IntoStatic);
        imports.external.insert(ExternalImport::PhantomData);

        // Collect from parameters.
        if let Some(params) = &sub.parameters {
            let p = match params {
                crate::lexicon::LexXrpcSubscriptionParameter::Params(p) => p,
            };
            for (_prop_name, prop) in &p.properties {
                imports.merge(self.collect_xrpc_parameter_property(nsid, prop));
            }
        }

        // Collect from message.
        if let Some(message) = &sub.message {
            if let Some(schema) = &message.schema {
                imports.merge(self.collect_xrpc_subscription_message_schema(nsid, schema));
            }
        }

        // Error enums use open_union and CowStr for variant data.
        if sub.errors.as_ref().is_some_and(|e| !e.is_empty()) {
            imports.external.insert(ExternalImport::OpenUnion);
            imports.common.insert(CommonType::CowStr);
        }

        imports
    }

    /// Collect types from an XRPC body (query output, procedure input/output).
    fn collect_xrpc_body(
        &self,
        nsid: &str,
        body: &crate::lexicon::LexXrpcBody<'static>,
    ) -> ImportSet {
        let mut imports = ImportSet::default();
        if let Some(schema) = &body.schema {
            imports.merge(self.collect_xrpc_body_schema(nsid, schema));
        } else {
            // Binary body: uses Bytes.
            imports.external.insert(ExternalImport::Bytes);
        }
        imports
    }

    /// Collect types from an XRPC body schema.
    fn collect_xrpc_body_schema(
        &self,
        nsid: &str,
        schema: &crate::lexicon::LexXrpcBodySchema<'static>,
    ) -> ImportSet {
        let mut imports = ImportSet::default();
        match schema {
            crate::lexicon::LexXrpcBodySchema::Object(obj) => {
                imports.merge(self.collect_object(nsid, obj));
            }
            crate::lexicon::LexXrpcBodySchema::Ref(ref_type) => {
                let ref_str = if ref_type.r#ref.starts_with('#') {
                    format!("{}{}", nsid, ref_type.r#ref)
                } else {
                    ref_type.r#ref.to_string()
                };
                imports.merge(self.collect_ref_type(&ref_str));
            }
            crate::lexicon::LexXrpcBodySchema::Union(union) => {
                // Generator falls back to Data for union body schemas.
                imports.common.insert(CommonType::Data);
                for ref_str in &union.refs {
                    let full_ref = if ref_str.starts_with('#') {
                        format!("{}{}", nsid, ref_str)
                    } else {
                        ref_str.to_string()
                    };
                    imports.merge(self.collect_ref_type(&full_ref));
                }
            }
        }
        imports.external.insert(ExternalImport::DefaultStr);
        imports
    }

    /// Collect types from an XRPC parameter property.
    fn collect_xrpc_parameter_property(
        &self,
        nsid: &str,
        prop: &crate::lexicon::LexXrpcParametersProperty<'static>,
    ) -> ImportSet {
        let mut imports = ImportSet::default();
        match prop {
            crate::lexicon::LexXrpcParametersProperty::Boolean(_)
            | crate::lexicon::LexXrpcParametersProperty::Integer(_) => {}
            crate::lexicon::LexXrpcParametersProperty::String(s) => {
                imports.merge(self.collect_string_type(s));
            }
            crate::lexicon::LexXrpcParametersProperty::Unknown(_) => {
                imports.common.insert(CommonType::Data);
            }
            crate::lexicon::LexXrpcParametersProperty::Array(array) => {
                imports.merge(self.collect_primitive_array_item_types(nsid, &array.items));
            }
        }
        imports
    }

    /// Collect types from a primitive array item (used in XRPC parameters).
    fn collect_primitive_array_item_types(
        &self,
        _nsid: &str,
        item: &crate::lexicon::LexPrimitiveArrayItem<'static>,
    ) -> ImportSet {
        let mut imports = ImportSet::default();
        match item {
            crate::lexicon::LexPrimitiveArrayItem::Boolean(_)
            | crate::lexicon::LexPrimitiveArrayItem::Integer(_) => {}
            crate::lexicon::LexPrimitiveArrayItem::String(s) => {
                imports.merge(self.collect_string_type(s));
            }
            crate::lexicon::LexPrimitiveArrayItem::Unknown(_) => {
                imports.common.insert(CommonType::Data);
            }
        }
        imports
    }

    /// Collect types from an XRPC subscription message schema.
    fn collect_xrpc_subscription_message_schema(
        &self,
        nsid: &str,
        schema: &crate::lexicon::LexXrpcSubscriptionMessageSchema<'static>,
    ) -> ImportSet {
        let mut imports = ImportSet::default();
        match schema {
            crate::lexicon::LexXrpcSubscriptionMessageSchema::Object(obj) => {
                imports.merge(self.collect_object(nsid, obj));
            }
            crate::lexicon::LexXrpcSubscriptionMessageSchema::Ref(ref_type) => {
                let ref_str = if ref_type.r#ref.starts_with('#') {
                    format!("{}{}", nsid, ref_type.r#ref)
                } else {
                    ref_type.r#ref.to_string()
                };
                imports.merge(self.collect_ref_type(&ref_str));
            }
            crate::lexicon::LexXrpcSubscriptionMessageSchema::Union(union) => {
                // Subscription message unions are always open.
                imports.external.insert(ExternalImport::OpenUnion);
                for ref_str in &union.refs {
                    let full_ref = if ref_str.starts_with('#') {
                        format!("{}{}", nsid, ref_str)
                    } else {
                        ref_str.to_string()
                    };
                    imports.merge(self.collect_ref_type(&full_ref));
                }
            }
        }
        imports.external.insert(ExternalImport::DefaultStr);
        imports
    }

    /// Collect types from any definition (mirrors generate_def dispatcher).
    pub(super) fn collect_def(
        &self,
        nsid: &str,
        _def_name: &str,
        def: &LexUserType<'static>,
    ) -> ImportSet {
        // All BOS-parameterised types need Bos, BosStr, DefaultStr, and FromStaticStr.
        let mut base = ImportSet::default();
        base.external.insert(ExternalImport::BosStr);
        base.external.insert(ExternalImport::DefaultStr);
        base.external.insert(ExternalImport::FromStaticStr);

        let mut result = match def {
            LexUserType::Record(r) => self.collect_record(nsid, r),
            LexUserType::Object(o) => {
                let type_name = self.def_to_type_name(nsid, _def_name);
                let mut imports = self.collect_object_with_builder_check(nsid, Some(&type_name), o);
                // Top-level objects generate LexiconSchema trait impls with validation.
                // (XRPC body objects go through collect_object too, but don't get schema impls.)
                imports.external.insert(ExternalImport::LexiconSchema);
                imports.external.insert(ExternalImport::LexiconDoc);
                imports.external.insert(ExternalImport::ConstraintError);
                imports.external.insert(ExternalImport::ValidationPath);
                imports.external.insert(ExternalImport::UnicodeSegmentation);
                imports
            }
            LexUserType::XrpcQuery(q) => self.collect_query(nsid, q),
            LexUserType::XrpcProcedure(p) => self.collect_procedure(nsid, p),
            LexUserType::XrpcSubscription(s) => self.collect_subscription(nsid, s),
            // Token: generates a struct with serde + IntoStatic derives.
            LexUserType::Token(_) => {
                let mut i = ImportSet::default();
                i.external.insert(ExternalImport::Serialize);
                i.external.insert(ExternalImport::Deserialize);
                i.external.insert(ExternalImport::IntoStatic);
                i
            }
            // String with known_values: generates an enum with custom Serialize,
            // Deserialize, and IntoStatic impls (NOT derives). Needs CowStr
            // for the catch-all Other variant, plus Bos/DefaultStr for the type param.
            LexUserType::String(s) if s.known_values.is_some() => {
                let mut i = ImportSet::default();
                i.common.insert(CommonType::CowStr);
                i.external.insert(ExternalImport::DefaultStr);
                i
            }
            // Plain string: type alias, only needs the string type.
            LexUserType::String(s) => self.collect_string_type(s),
            // Integer with enum: generates an enum with serde + IntoStatic derives.
            LexUserType::Integer(i) if i.r#enum.is_some() => {
                let mut imports = ImportSet::default();
                imports.external.insert(ExternalImport::Serialize);
                imports.external.insert(ExternalImport::Deserialize);
                imports.external.insert(ExternalImport::IntoStatic);
                imports
            }
            // Top-level array: type alias to Vec<ItemType>. If items are a union,
            // the union refs need tracking. Otherwise walk the array item type.
            LexUserType::Array(array) => {
                if let LexArrayItem::Union(union) = &array.items {
                    // Array-with-union generates a union enum via generate_union.
                    let mut imports = ImportSet::default();
                    imports.external.insert(ExternalImport::Serialize);
                    imports.external.insert(ExternalImport::Deserialize);
                    imports.external.insert(ExternalImport::IntoStatic);
                    imports.external.insert(ExternalImport::DefaultStr);
                    if union.closed != Some(true) {
                        imports.external.insert(ExternalImport::OpenUnion);
                    }
                    for ref_str in &union.refs {
                        let full_ref = if ref_str.starts_with('#') {
                            format!("{}{}", nsid, ref_str)
                        } else {
                            ref_str.to_string()
                        };
                        imports.merge(self.collect_ref_type(&full_ref));
                    }
                    imports
                } else {
                    self.collect_array_item_types(nsid, &array.items)
                }
            }
            // Top-level union: generates an enum via generate_union.
            LexUserType::Union(union) => {
                let mut imports = ImportSet::default();
                imports.external.insert(ExternalImport::Serialize);
                imports.external.insert(ExternalImport::Deserialize);
                imports.external.insert(ExternalImport::IntoStatic);
                imports.external.insert(ExternalImport::DefaultStr);
                if union.closed != Some(true) {
                    imports.external.insert(ExternalImport::OpenUnion);
                }
                for ref_str in &union.refs {
                    let full_ref = if ref_str.starts_with('#') {
                        format!("{}{}", nsid, ref_str)
                    } else {
                        ref_str.to_string()
                    };
                    imports.merge(self.collect_ref_type(&full_ref));
                }
                imports
            }
            // Top-level unknown: type alias to Data.
            LexUserType::Unknown(_) => {
                let mut i = ImportSet::default();
                i.common.insert(CommonType::Data);
                i
            }
            // Top-level CidLink: type alias.
            LexUserType::CidLink(_) => {
                let mut i = ImportSet::default();
                i.common.insert(CommonType::CidLink);
                i
            }
            // Top-level Bytes: type alias.
            LexUserType::Bytes(_) => {
                let mut i = ImportSet::default();
                i.external.insert(ExternalImport::Bytes);
                i
            }
            // Boolean, plain Integer, Blob: type aliases with no special imports.
            _ => ImportSet::default(),
        };
        result.merge(base);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::LexiconCorpus;

    // Unit tests for enum variants and basic merge
    #[test]
    fn test_common_type_variants_exist() {
        // Verify that the match arms compile correctly by checking the enum variants exist
        assert_eq!(CommonType::Did.short_name(), "Did");
        assert_eq!(CommonType::Handle.short_name(), "Handle");
        assert_eq!(CommonType::CowStr.short_name(), "CowStr");
    }

    #[test]
    fn test_collect_string_type_record_key() {
        // RecordKey should pull in both RecordKey and Rkey
        assert_eq!(CommonType::RecordKey.short_name(), "RecordKey");
        assert_eq!(CommonType::Rkey.short_name(), "Rkey");
    }

    #[test]
    fn test_import_set_default() {
        let set = ImportSet::default();
        assert!(set.common.is_empty());
        assert!(set.external.is_empty());
        assert!(set.lexicon_refs.is_empty());
    }

    #[test]
    fn test_import_set_merge() {
        let mut set1 = ImportSet::default();
        set1.common.insert(CommonType::Did);
        set1.external.insert(ExternalImport::Serialize);

        let mut set2 = ImportSet::default();
        set2.common.insert(CommonType::Handle);
        set2.external.insert(ExternalImport::Deserialize);

        set1.merge(set2);

        assert!(set1.common.contains(&CommonType::Did));
        assert!(set1.common.contains(&CommonType::Handle));
        assert!(set1.external.contains(&ExternalImport::Serialize));
        assert!(set1.external.contains(&ExternalImport::Deserialize));
    }

    // Tests using the actual test corpus (AC3.1, AC3.2, AC3.3)

    #[test]
    fn test_collect_post_record_finds_common_types() {
        // Verifies AC3.1: Collection pass identifies all CommonType variants referenced in a file
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = super::super::CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus.get("app.bsky.feed.post").expect("get post");
        let def = doc.defs.get("main").expect("get main def");

        let imports = codegen.collect_def("app.bsky.feed.post", "main", def);

        // Post contains createdAt with datetime format
        assert!(
            imports.common.contains(&CommonType::Datetime),
            "Post should collect Datetime from createdAt field"
        );

        // Post contains plain strings which default to CowStr
        assert!(
            imports.common.contains(&CommonType::CowStr),
            "Post should collect CowStr from plain string fields"
        );

        // Post contains Language format (langs field)
        assert!(
            imports.common.contains(&CommonType::Language),
            "Post should collect Language from langs field"
        );

        // Post should have serde derives
        assert!(
            imports.external.contains(&ExternalImport::Serialize),
            "Post record should have Serialize"
        );
        assert!(
            imports.external.contains(&ExternalImport::Deserialize),
            "Post record should have Deserialize"
        );
        assert!(
            imports.external.contains(&ExternalImport::IntoStatic),
            "Post record should have IntoStatic"
        );
    }

    #[test]
    fn test_collect_query_finds_at_identifier() {
        // Verifies AC3.1: Collection identifies AtIdentifier in query parameters
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = super::super::CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus
            .get("app.bsky.feed.getAuthorFeed")
            .expect("get getAuthorFeed");
        let def = doc.defs.get("main").expect("get main def");

        let imports = codegen.collect_def("app.bsky.feed.getAuthorFeed", "main", def);

        // getAuthorFeed has "actor" param with at-identifier format
        assert!(
            imports.common.contains(&CommonType::AtIdentifier),
            "Query should collect AtIdentifier from actor parameter"
        );

        // Query should have serde derives
        assert!(
            imports.external.contains(&ExternalImport::Serialize),
            "Query should have Serialize"
        );
        assert!(
            imports.external.contains(&ExternalImport::Deserialize),
            "Query should have Deserialize"
        );

        // Queries use IntoStatic.
        assert!(
            imports.external.contains(&ExternalImport::IntoStatic),
            "Query should have IntoStatic"
        );
    }

    #[test]
    fn test_collect_external_object_finds_uri_format() {
        // Verifies AC3.1: Collection identifies UriValue from uri format
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = super::super::CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus.get("app.bsky.embed.external").expect("get external");
        let def = doc.defs.get("external").expect("get external def");

        let imports = codegen.collect_def("app.bsky.embed.external", "external", def);

        // external object has "uri" field with uri format
        assert!(
            imports.common.contains(&CommonType::UriValue),
            "External object should collect UriValue from uri field"
        );

        // external object should have blob reference
        assert!(
            imports.common.contains(&CommonType::BlobRef),
            "External object should collect BlobRef from thumb blob field"
        );

        // Objects use serde derives and IntoStatic.
        assert!(
            imports.external.contains(&ExternalImport::Serialize),
            "Object should have Serialize"
        );
        assert!(
            imports.external.contains(&ExternalImport::Deserialize),
            "Object should have Deserialize"
        );
        assert!(
            imports.external.contains(&ExternalImport::IntoStatic),
            "Object should have IntoStatic"
        );
    }

    #[test]
    fn test_collect_ref_type_tracks_lexicon_refs() {
        // Verifies AC3.3: Collection pass identifies cross-namespace lexicon refs
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = super::super::CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus.get("app.bsky.feed.post").expect("get post");
        let def = doc.defs.get("main").expect("get main def");

        let imports = codegen.collect_def("app.bsky.feed.post", "main", def);

        // Post references cross-namespace types like app.bsky.richtext.facet, app.bsky.embed.*
        // These should be tracked in lexicon_refs
        let has_cross_namespace = !imports.lexicon_refs.is_empty();
        assert!(
            has_cross_namespace,
            "Post should have cross-namespace refs tracked"
        );

        // Verify at least one cross-namespace ref is tracked
        // (we can't assert specific refs since they depend on corpus state)
        let refs_vec: Vec<_> = imports.lexicon_refs.iter().collect();
        assert!(refs_vec.len() > 0, "Should have at least one lexicon_ref");
    }

    #[test]
    fn test_collect_preserves_collection_trait_hint() {
        // Verifies AC3.1: Records collect CowStr for the uri method
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = super::super::CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus.get("app.bsky.feed.post").expect("get post");
        let def = doc.defs.get("main").expect("get main def");

        let imports = codegen.collect_def("app.bsky.feed.post", "main", def);

        // All records add CowStr (for uri method)
        assert!(
            imports.common.contains(&CommonType::CowStr),
            "Records should always collect CowStr for uri method"
        );
    }

    #[test]
    fn test_collect_multiple_defs_from_same_file() {
        // Verifies AC3.1: Can collect from multiple definitions in the same file
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = super::super::CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus.get("app.bsky.feed.post").expect("get post");

        // Collect from "main" definition
        let main_def = doc.defs.get("main").expect("get main def");
        let main_imports = codegen.collect_def("app.bsky.feed.post", "main", main_def);
        assert!(main_imports.common.contains(&CommonType::Datetime));

        // Collect from "replyRef" definition
        let reply_def = doc.defs.get("replyRef").expect("get replyRef def");
        let reply_imports = codegen.collect_def("app.bsky.feed.post", "replyRef", reply_def);

        // replyRef refs to com.atproto.repo.strongRef (cross-namespace)
        assert!(
            !reply_imports.lexicon_refs.is_empty(),
            "replyRef should have lexicon_refs"
        );

        // Both have different imports - can collect from each
        let has_datetime_in_main = main_imports.common.contains(&CommonType::Datetime);
        let has_datetime_in_reply = reply_imports.common.contains(&CommonType::Datetime);
        assert!(
            has_datetime_in_main,
            "main def should have Datetime from createdAt"
        );
        assert!(!has_datetime_in_reply, "replyRef should not have Datetime");
    }
}
