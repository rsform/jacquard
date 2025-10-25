//! Builder API for manual lexicon schema construction
//!
//! Provides ergonomic API for building lexicon documents without implementing the trait.
//! Useful for prototyping, testing, and dynamic schema generation.

use crate::lexicon::{
    LexArray, LexArrayItem, LexBoolean, LexInteger, LexObject, LexObjectProperty, LexRecord,
    LexRecordRecord, LexRef, LexString, LexStringFormat, LexUserType, LexXrpcBody,
    LexXrpcBodySchema, LexXrpcError, LexXrpcParameters, LexXrpcParametersProperty, LexXrpcQuery,
    LexXrpcQueryParameter, Lexicon, LexiconDoc,
};
use jacquard_common::CowStr;
use jacquard_common::smol_str::SmolStr;
use std::collections::BTreeMap;

/// Builder for lexicon documents
pub struct LexiconDocBuilder {
    nsid: SmolStr,
    description: Option<CowStr<'static>>,
    defs: BTreeMap<SmolStr, LexUserType<'static>>,
}

impl LexiconDocBuilder {
    /// Start building a lexicon document
    pub fn new(nsid: impl Into<SmolStr>) -> Self {
        Self {
            nsid: nsid.into(),
            description: None,
            defs: BTreeMap::new(),
        }
    }

    /// Set document description
    pub fn description(mut self, desc: impl Into<CowStr<'static>>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Add a record def (becomes "main")
    pub fn record(self) -> RecordBuilder {
        RecordBuilder {
            doc_builder: self,
            key: None,
            description: None,
            properties: BTreeMap::new(),
            required: Vec::new(),
        }
    }

    /// Add an object def
    pub fn object(self, name: impl Into<SmolStr>) -> ObjectBuilder {
        ObjectBuilder {
            doc_builder: self,
            def_name: name.into(),
            description: None,
            properties: BTreeMap::new(),
            required: Vec::new(),
        }
    }

    /// Add a query def
    pub fn query(self) -> QueryBuilder {
        QueryBuilder {
            doc_builder: self,
            description: None,
            parameters: BTreeMap::new(),
            required_params: Vec::new(),
            output: None,
            errors: Vec::new(),
        }
    }

    /// Build the final document
    pub fn build(self) -> LexiconDoc<'static> {
        LexiconDoc {
            lexicon: Lexicon::Lexicon1,
            id: self.nsid.into(),
            revision: None,
            description: self.description,
            defs: self.defs,
        }
    }
}

pub struct RecordBuilder {
    doc_builder: LexiconDocBuilder,
    key: Option<CowStr<'static>>,
    description: Option<CowStr<'static>>,
    properties: BTreeMap<SmolStr, LexObjectProperty<'static>>,
    required: Vec<SmolStr>,
}

impl RecordBuilder {
    /// Set record key type (e.g., "tid")
    pub fn key(mut self, key: impl Into<CowStr<'static>>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set description
    pub fn description(mut self, desc: impl Into<CowStr<'static>>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Add a field
    pub fn field<F>(mut self, name: impl Into<SmolStr>, builder: F) -> Self
    where
        F: FnOnce(FieldBuilder) -> FieldBuilder,
    {
        let field_builder = FieldBuilder::new();
        let field_builder = builder(field_builder);

        let name = name.into();
        if field_builder.required {
            self.required.push(name.clone());
        }

        self.properties.insert(name, field_builder.build());
        self
    }

    /// Build and add to document
    pub fn build(mut self) -> LexiconDocBuilder {
        let record_obj = LexObject {
            description: self.description,
            required: if self.required.is_empty() {
                None
            } else {
                Some(self.required)
            },
            nullable: None,
            properties: self.properties,
        };

        let record = LexRecord {
            description: None,
            key: self.key,
            record: LexRecordRecord::Object(record_obj),
        };

        self.doc_builder
            .defs
            .insert("main".into(), LexUserType::Record(record));
        self.doc_builder
    }
}

pub struct ObjectBuilder {
    doc_builder: LexiconDocBuilder,
    def_name: SmolStr,
    description: Option<CowStr<'static>>,
    properties: BTreeMap<SmolStr, LexObjectProperty<'static>>,
    required: Vec<SmolStr>,
}

impl ObjectBuilder {
    /// Set description
    pub fn description(mut self, desc: impl Into<CowStr<'static>>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Add a field
    pub fn field<F>(mut self, name: impl Into<SmolStr>, builder: F) -> Self
    where
        F: FnOnce(FieldBuilder) -> FieldBuilder,
    {
        let field_builder = FieldBuilder::new();
        let field_builder = builder(field_builder);

        let name = name.into();
        if field_builder.required {
            self.required.push(name.clone());
        }

        self.properties.insert(name, field_builder.build());
        self
    }

    /// Build and add to document
    pub fn build(mut self) -> LexiconDocBuilder {
        let object = LexObject {
            description: self.description,
            required: if self.required.is_empty() {
                None
            } else {
                Some(self.required)
            },
            nullable: None,
            properties: self.properties,
        };

        self.doc_builder
            .defs
            .insert(self.def_name, LexUserType::Object(object));
        self.doc_builder
    }
}

pub struct QueryBuilder {
    doc_builder: LexiconDocBuilder,
    description: Option<CowStr<'static>>,
    parameters: BTreeMap<SmolStr, LexXrpcParametersProperty<'static>>,
    required_params: Vec<SmolStr>,
    output: Option<LexXrpcBody<'static>>,
    errors: Vec<LexXrpcError<'static>>,
}

impl QueryBuilder {
    /// Set description
    pub fn description(mut self, desc: impl Into<CowStr<'static>>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Add a string parameter
    pub fn param_string(mut self, name: impl Into<SmolStr>, required: bool) -> Self {
        let param = LexXrpcParametersProperty::String(LexString {
            description: None,
            format: None,
            default: None,
            min_length: None,
            max_length: None,
            min_graphemes: None,
            max_graphemes: None,
            r#enum: None,
            r#const: None,
            known_values: None,
        });

        let name = name.into();
        if required {
            self.required_params.push(name.clone());
        }
        self.parameters.insert(name, param);
        self
    }

    /// Set output schema
    pub fn output(
        mut self,
        encoding: impl Into<CowStr<'static>>,
        schema: LexXrpcBodySchema<'static>,
    ) -> Self {
        self.output = Some(LexXrpcBody {
            description: None,
            encoding: encoding.into(),
            schema: Some(schema),
        });
        self
    }

    /// Build and add to document
    pub fn build(mut self) -> LexiconDocBuilder {
        let params = if self.parameters.is_empty() {
            None
        } else {
            Some(LexXrpcQueryParameter::Params(LexXrpcParameters {
                description: None,
                required: if self.required_params.is_empty() {
                    None
                } else {
                    Some(self.required_params)
                },
                properties: self.parameters,
            }))
        };

        let query = LexXrpcQuery {
            description: self.description,
            parameters: params,
            output: self.output,
            errors: if self.errors.is_empty() {
                None
            } else {
                Some(self.errors)
            },
        };

        self.doc_builder
            .defs
            .insert("main".into(), LexUserType::XrpcQuery(query));
        self.doc_builder
    }
}

pub struct FieldBuilder {
    property: Option<LexObjectProperty<'static>>,
    required: bool,
}

impl FieldBuilder {
    fn new() -> Self {
        Self {
            property: None,
            required: false,
        }
    }

    /// Mark field as required
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// String field
    pub fn string(self) -> StringFieldBuilder {
        StringFieldBuilder {
            field_builder: self,
            format: None,
            max_length: None,
            max_graphemes: None,
            min_length: None,
            min_graphemes: None,
            description: None,
        }
    }

    /// Integer field
    pub fn integer(self) -> IntegerFieldBuilder {
        IntegerFieldBuilder {
            field_builder: self,
            minimum: None,
            maximum: None,
            description: None,
        }
    }

    /// Boolean field
    pub fn boolean(mut self) -> Self {
        self.property = Some(LexObjectProperty::Boolean(LexBoolean {
            description: None,
            default: None,
            r#const: None,
        }));
        self
    }

    /// Ref field (to another type)
    pub fn ref_to(mut self, ref_nsid: impl Into<CowStr<'static>>) -> Self {
        self.property = Some(LexObjectProperty::Ref(LexRef {
            description: None,
            r#ref: ref_nsid.into(),
        }));
        self
    }

    /// Array field
    pub fn array<F>(mut self, item_builder: F) -> Self
    where
        F: FnOnce(ArrayItemBuilder) -> ArrayItemBuilder,
    {
        let builder = ArrayItemBuilder::new();
        let builder = item_builder(builder);
        self.property = Some(LexObjectProperty::Array(builder.build()));
        self
    }

    pub fn build(self) -> LexObjectProperty<'static> {
        self.property.expect("field type not set")
    }
}

pub struct StringFieldBuilder {
    field_builder: FieldBuilder,
    format: Option<LexStringFormat>,
    max_length: Option<usize>,
    max_graphemes: Option<usize>,
    min_length: Option<usize>,
    min_graphemes: Option<usize>,
    description: Option<CowStr<'static>>,
}

impl StringFieldBuilder {
    pub fn format(mut self, format: LexStringFormat) -> Self {
        self.format = Some(format);
        self
    }

    pub fn max_length(mut self, max: usize) -> Self {
        self.max_length = Some(max);
        self
    }

    pub fn max_graphemes(mut self, max: usize) -> Self {
        self.max_graphemes = Some(max);
        self
    }

    pub fn min_length(mut self, min: usize) -> Self {
        self.min_length = Some(min);
        self
    }

    pub fn min_graphemes(mut self, min: usize) -> Self {
        self.min_graphemes = Some(min);
        self
    }

    pub fn description(mut self, desc: impl Into<CowStr<'static>>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn required(mut self) -> Self {
        self.field_builder.required = true;
        self
    }

    pub fn build(mut self) -> FieldBuilder {
        self.field_builder.property = Some(LexObjectProperty::String(LexString {
            description: self.description,
            format: self.format,
            default: None,
            min_length: self.min_length,
            max_length: self.max_length,
            min_graphemes: self.min_graphemes,
            max_graphemes: self.max_graphemes,
            r#enum: None,
            r#const: None,
            known_values: None,
        }));
        self.field_builder
    }
}

pub struct IntegerFieldBuilder {
    field_builder: FieldBuilder,
    minimum: Option<i64>,
    maximum: Option<i64>,
    description: Option<CowStr<'static>>,
}

impl IntegerFieldBuilder {
    pub fn minimum(mut self, min: i64) -> Self {
        self.minimum = Some(min);
        self
    }

    pub fn maximum(mut self, max: i64) -> Self {
        self.maximum = Some(max);
        self
    }

    pub fn description(mut self, desc: impl Into<CowStr<'static>>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn build(mut self) -> FieldBuilder {
        self.field_builder.property = Some(LexObjectProperty::Integer(LexInteger {
            description: self.description,
            default: None,
            minimum: self.minimum,
            maximum: self.maximum,
            r#enum: None,
            r#const: None,
        }));
        self.field_builder
    }
}

pub struct ArrayItemBuilder {
    item: Option<LexArrayItem<'static>>,
    description: Option<CowStr<'static>>,
    min_length: Option<usize>,
    max_length: Option<usize>,
}

impl ArrayItemBuilder {
    fn new() -> Self {
        Self {
            item: None,
            description: None,
            min_length: None,
            max_length: None,
        }
    }

    pub fn description(mut self, desc: impl Into<CowStr<'static>>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn min_length(mut self, min: usize) -> Self {
        self.min_length = Some(min);
        self
    }

    pub fn max_length(mut self, max: usize) -> Self {
        self.max_length = Some(max);
        self
    }

    /// String items
    pub fn string_items(mut self) -> Self {
        self.item = Some(LexArrayItem::String(LexString {
            description: None,
            format: None,
            default: None,
            min_length: None,
            max_length: None,
            min_graphemes: None,
            max_graphemes: None,
            r#enum: None,
            r#const: None,
            known_values: None,
        }));
        self
    }

    /// Ref items
    pub fn ref_items(mut self, ref_nsid: impl Into<CowStr<'static>>) -> Self {
        self.item = Some(LexArrayItem::Ref(LexRef {
            description: None,
            r#ref: ref_nsid.into(),
        }));
        self
    }

    fn build(self) -> LexArray<'static> {
        LexArray {
            description: self.description,
            items: self.item.expect("array item type not set"),
            min_length: self.min_length,
            max_length: self.max_length,
        }
    }
}
