//! Runtime validation of Data values against lexicon schemas
//!
//! This module provides infrastructure for validating untyped `Data` values against
//! lexicon schemas, enabling partial deserialization, debugging, and schema migration.

use crate::lexicon::{LexArrayItem, LexObjectProperty};
use crate::ref_utils::RefPath;
use crate::schema::SchemaRegistry;
use cid::Cid as IpldCid;
use dashmap::DashMap;
use jacquard_common::{smol_str, types::value::Data};
use sha2::{Digest, Sha256};
use smol_str::SmolStr;
use std::{
    fmt,
    sync::{Arc, LazyLock},
};

/// Path to a value within a data structure
///
/// Tracks the location of values during validation for precise error reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationPath {
    segments: Vec<PathSegment>,
}

/// A segment in a validation path
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSegment {
    /// Object field access
    Field(SmolStr),
    /// Array index access
    Index(usize),
    /// Union variant discriminator
    UnionVariant(SmolStr),
}

impl ValidationPath {
    /// Create a new empty path
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// Create a path with a single field segment
    pub fn from_field(name: &str) -> Self {
        let mut path = Self::new();
        path.push_field(name);
        path
    }

    /// Add a field segment to the path
    pub fn push_field(&mut self, name: &str) {
        self.segments.push(PathSegment::Field(name.into()));
    }

    /// Add an index segment to the path
    pub fn push_index(&mut self, idx: usize) {
        self.segments.push(PathSegment::Index(idx));
    }

    /// Add a union variant segment to the path
    pub fn push_variant(&mut self, type_str: &str) {
        self.segments
            .push(PathSegment::UnionVariant(type_str.into()));
    }

    /// Remove the last segment from the path
    pub fn pop(&mut self) {
        self.segments.pop();
    }

    /// Get the depth of the path
    pub fn depth(&self) -> usize {
        self.segments.len()
    }

    /// Check if the path is empty
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    pub fn segments(&self) -> &[PathSegment] {
        &self.segments
    }
}

impl Default for ValidationPath {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ValidationPath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.segments.is_empty() {
            return write!(f, "(root)");
        }

        for seg in &self.segments {
            match seg {
                PathSegment::Field(name) => write!(f, ".{}", name)?,
                PathSegment::Index(idx) => write!(f, "[{}]", idx)?,
                PathSegment::UnionVariant(t) => write!(f, "($type={})", t)?,
            }
        }
        Ok(())
    }
}

/// Structural validation errors
///
/// These errors indicate that the data structure doesn't match the schema's type expectations.
#[derive(Debug, Clone, thiserror::Error, miette::Diagnostic)]
pub enum StructuralError {
    #[error("Type mismatch at {path}: expected {expected}, got {actual}")]
    TypeMismatch {
        path: ValidationPath,
        expected: jacquard_common::types::DataModelType,
        actual: jacquard_common::types::DataModelType,
    },

    #[error("Missing required field at {path}: '{field}'")]
    MissingRequiredField {
        path: ValidationPath,
        field: SmolStr,
    },

    #[error("Missing union discriminator ($type) at {path}")]
    MissingUnionDiscriminator { path: ValidationPath },

    #[error("Union type mismatch at {path}: $type='{actual_type}' not in [{expected_refs}]")]
    UnionNoMatch {
        path: ValidationPath,
        actual_type: SmolStr,
        expected_refs: SmolStr,
    },

    #[error("Unresolved ref at {path}: '{ref_nsid}'")]
    UnresolvedRef {
        path: ValidationPath,
        ref_nsid: SmolStr,
    },

    #[error("Reference cycle detected at {path}: '{ref_nsid}' (stack: {stack})")]
    RefCycle {
        path: ValidationPath,
        ref_nsid: SmolStr,
        stack: SmolStr,
    },

    #[error("Max validation depth exceeded at {path}: {max}")]
    MaxDepthExceeded { path: ValidationPath, max: usize },
}

/// Constraint validation errors
///
/// These errors indicate that the data violates lexicon constraints like max_length,
/// max_graphemes, ranges, etc. The structure is correct but values are out of bounds.
#[derive(Debug, Clone, thiserror::Error, miette::Diagnostic)]
pub enum ConstraintError {
    #[error("{path} exceeds max length: {actual} > {max}")]
    MaxLength {
        path: ValidationPath,
        max: usize,
        actual: usize,
    },

    #[error("{path} exceeds max graphemes: {actual} > {max}")]
    MaxGraphemes {
        path: ValidationPath,
        max: usize,
        actual: usize,
    },

    #[error("{path} below min length: {actual} < {min}")]
    MinLength {
        path: ValidationPath,
        min: usize,
        actual: usize,
    },

    #[error("{path} below min graphemes: {actual} < {min}")]
    MinGraphemes {
        path: ValidationPath,
        min: usize,
        actual: usize,
    },

    #[error("{path} value {actual} exceeds maximum: {max}")]
    Maximum {
        path: ValidationPath,
        max: i64,
        actual: i64,
    },

    #[error("{path} value {actual} below minimum: {min}")]
    Minimum {
        path: ValidationPath,
        min: i64,
        actual: i64,
    },
}

/// Unified validation error type
#[derive(Debug, Clone, thiserror::Error)]
pub enum ValidationError {
    #[error(transparent)]
    Structural(#[from] StructuralError),

    #[error(transparent)]
    Constraint(#[from] ConstraintError),
}

/// Cache key for validation results
///
/// Content-addressed by CID to enable efficient caching across identical data.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct ValidationCacheKey {
    nsid: SmolStr,
    def_name: SmolStr,
    cid: IpldCid,
}

impl ValidationCacheKey {
    /// Create cache key from schema info and data
    fn from_data<T: crate::schema::LexiconSchema>(
        data: &Data,
    ) -> Result<Self, CidComputationError> {
        let cid = compute_data_cid(data)?;
        Ok(Self {
            nsid: SmolStr::new_static(T::nsid()),
            def_name: SmolStr::new_static(T::def_name()),
            cid,
        })
    }
}

/// Errors that can occur when computing CIDs
#[derive(Debug, thiserror::Error)]
pub enum CidComputationError {
    #[error("Failed to serialize data to DAG-CBOR: {0}")]
    DagCborEncode(#[from] serde_ipld_dagcbor::EncodeError<std::collections::TryReserveError>),

    #[error("Failed to create multihash: {0}")]
    Multihash(#[from] multihash::Error),
}

/// Compute CID for Data value
///
/// Uses SHA-256 hash and DAG-CBOR codec for content addressing.
fn compute_data_cid(data: &Data) -> Result<IpldCid, CidComputationError> {
    // Serialize to DAG-CBOR
    let dag_cbor = data.to_dag_cbor()?;

    // Compute SHA-256 hash
    let hash = Sha256::digest(&dag_cbor);

    // Create multihash (code 0x12 = sha2-256)
    let multihash = multihash::Multihash::wrap(0x12, &hash)?;

    // Create CIDv1 with dag-cbor codec (0x71)
    Ok(IpldCid::new_v1(0x71, multihash))
}

/// Trait for converting lexicon types to object properties
///
/// This enables type-safe conversion between array items and object properties
/// for unified validation logic.
trait IntoObjectProperty<'a> {
    /// Convert this type to an equivalent object property
    fn into_object_property(self) -> LexObjectProperty<'a>;
}

impl<'a> IntoObjectProperty<'a> for LexArrayItem<'a> {
    fn into_object_property(self) -> LexObjectProperty<'a> {
        match self {
            LexArrayItem::String(s) => LexObjectProperty::String(s),
            LexArrayItem::Integer(i) => LexObjectProperty::Integer(i),
            LexArrayItem::Boolean(b) => LexObjectProperty::Boolean(b),
            LexArrayItem::Object(o) => LexObjectProperty::Object(o),
            LexArrayItem::Unknown(u) => LexObjectProperty::Unknown(u),
            LexArrayItem::Bytes(b) => LexObjectProperty::Bytes(b),
            LexArrayItem::CidLink(c) => LexObjectProperty::CidLink(c),
            LexArrayItem::Blob(b) => LexObjectProperty::Blob(b),
            LexArrayItem::Ref(r) => LexObjectProperty::Ref(r),
            LexArrayItem::Union(u) => LexObjectProperty::Union(u),
        }
    }
}

/// Result of validating Data against a schema
///
/// Distinguishes between structural errors (type mismatches, missing fields) and
/// constraint violations (max_length, ranges, etc.).
#[derive(Debug, Clone)]
pub enum ValidationResult {
    /// Only structural validation was performed (or data was structurally invalid)
    StructuralOnly { structural: Vec<StructuralError> },
    /// Both structural and constraint validation were performed
    Complete {
        structural: Vec<StructuralError>,
        constraints: Vec<ConstraintError>,
    },
}

impl ValidationResult {
    /// Check if validation passed (no structural or constraint errors)
    pub fn is_valid(&self) -> bool {
        match self {
            ValidationResult::StructuralOnly { structural } => structural.is_empty(),
            ValidationResult::Complete {
                structural,
                constraints,
            } => structural.is_empty() && constraints.is_empty(),
        }
    }

    /// Check if structurally valid (ignoring constraint checks)
    pub fn is_structurally_valid(&self) -> bool {
        match self {
            ValidationResult::StructuralOnly { structural } => structural.is_empty(),
            ValidationResult::Complete { structural, .. } => structural.is_empty(),
        }
    }

    /// Get structural errors
    pub fn structural_errors(&self) -> &[StructuralError] {
        match self {
            ValidationResult::StructuralOnly { structural } => structural,
            ValidationResult::Complete { structural, .. } => structural,
        }
    }

    /// Get constraint errors
    pub fn constraint_errors(&self) -> &[ConstraintError] {
        match self {
            ValidationResult::StructuralOnly { .. } => &[],
            ValidationResult::Complete { constraints, .. } => constraints,
        }
    }

    /// Check if there are any constraint violations
    pub fn has_constraint_violations(&self) -> bool {
        !self.constraint_errors().is_empty()
    }

    /// Get all errors (structural and constraint)
    pub fn all_errors(&self) -> impl Iterator<Item = ValidationError> + '_ {
        self.structural_errors()
            .iter()
            .cloned()
            .map(ValidationError::Structural)
            .chain(
                self.constraint_errors()
                    .iter()
                    .cloned()
                    .map(ValidationError::Constraint),
            )
    }
}

/// Schema validator with caching
///
/// Validates Data values against lexicon schemas, caching results by content hash.
pub struct SchemaValidator {
    registry: SchemaRegistry,
    cache: DashMap<ValidationCacheKey, Arc<ValidationResult>>,
}

static VALIDATOR: LazyLock<SchemaValidator> = LazyLock::new(|| SchemaValidator {
    registry: SchemaRegistry::from_inventory(),
    cache: DashMap::new(),
});

impl SchemaValidator {
    /// Get the global validator instance
    pub fn global() -> &'static Self {
        &VALIDATOR
    }

    /// Create a new validator with empty registry
    pub fn new() -> Self {
        Self {
            registry: SchemaRegistry::new(),
            cache: DashMap::new(),
        }
    }

    pub fn from_registry(registry: SchemaRegistry) -> Self {
        Self {
            registry,
            cache: DashMap::new(),
        }
    }

    /// Validate data against a schema (structural and constraints)
    ///
    /// Performs both structural validation (types, required fields) and constraint
    /// validation (max_length, ranges, etc.). Results are cached by content hash.
    pub fn validate<T: crate::schema::LexiconSchema>(
        &self,
        data: &Data,
    ) -> Result<ValidationResult, CidComputationError> {
        // Compute cache key
        let key = ValidationCacheKey::from_data::<T>(data)?;

        // Check cache (clone Arc immediately to avoid holding ref)
        if let Some(cached) = self.cache.get(&key).map(|r| Arc::clone(&r)) {
            return Ok((*cached).clone());
        }

        // Perform validation
        let result = self.validate_uncached::<T>(data);

        // Cache result
        self.cache.insert(key, Arc::new(result.clone()));

        Ok(result)
    }

    /// Validate only the structural aspects of data against a schema
    ///
    /// Only checks types, required fields, and schema structure. Does not check
    /// constraints like max_length, ranges, etc. This is faster when you only
    /// care about type correctness.
    pub fn validate_structural<T: crate::schema::LexiconSchema>(
        &self,
        data: &Data,
    ) -> ValidationResult {
        self.validate_structural_uncached::<T>(data)
    }

    /// Validate without caching (internal)
    fn validate_uncached<T: crate::schema::LexiconSchema>(&self, data: &Data) -> ValidationResult {
        let def = match self.registry.get_def(T::nsid(), T::def_name()) {
            Some(d) => d,
            None => {
                // Schema not found - this is a structural error
                return ValidationResult::StructuralOnly {
                    structural: vec![StructuralError::UnresolvedRef {
                        path: ValidationPath::new(),
                        ref_nsid: format!("{}#{}", T::nsid(), T::def_name()).into(),
                    }],
                };
            }
        };

        let mut path = ValidationPath::new();
        let mut ctx = ValidationContext::new(T::nsid(), T::def_name());

        let structural_errors = validate_def(&mut path, data, &def, &self.registry, &mut ctx);

        // If structurally invalid, return structural errors only
        if !structural_errors.is_empty() {
            return ValidationResult::StructuralOnly {
                structural: structural_errors,
            };
        }

        // Structurally valid - compute constraints eagerly
        let mut path = ValidationPath::new();
        let constraint_errors = validate_constraints(
            &mut path,
            data,
            T::nsid(),
            T::def_name(),
            Some(&Arc::new(self.registry.clone())),
        );

        ValidationResult::Complete {
            structural: structural_errors,
            constraints: constraint_errors,
        }
    }

    /// Validate structural aspects only without caching (internal)
    fn validate_structural_uncached<T: crate::schema::LexiconSchema>(
        &self,
        data: &Data,
    ) -> ValidationResult {
        let def = match self.registry.get_def(T::nsid(), T::def_name()) {
            Some(d) => d,
            None => {
                // Schema not found - this is a structural error
                return ValidationResult::StructuralOnly {
                    structural: vec![StructuralError::UnresolvedRef {
                        path: ValidationPath::new(),
                        ref_nsid: format!("{}#{}", T::nsid(), T::def_name()).into(),
                    }],
                };
            }
        };

        let mut path = ValidationPath::new();
        let mut ctx = ValidationContext::new(T::nsid(), T::def_name());

        let structural_errors = validate_def(&mut path, data, &def, &self.registry, &mut ctx);

        ValidationResult::StructuralOnly {
            structural: structural_errors,
        }
    }

    pub fn validate_by_nsid_structural(&self, nsid: &str, data: &Data) -> ValidationResult {
        let mut split = nsid.split('#');
        let nsid = split.next().unwrap();
        let def_name = split.next().unwrap_or("main");
        let def = match self.registry.get_def(nsid, def_name) {
            Some(d) => d,
            None => {
                // Schema not found - this is a structural error
                return ValidationResult::StructuralOnly {
                    structural: vec![StructuralError::UnresolvedRef {
                        path: ValidationPath::new(),
                        ref_nsid: format!("{}#{}", nsid, def_name).into(),
                    }],
                };
            }
        };

        let mut path = ValidationPath::new();
        let mut ctx = ValidationContext::new(nsid, def_name);

        let structural_errors = validate_def(&mut path, data, &def, &self.registry, &mut ctx);

        ValidationResult::StructuralOnly {
            structural: structural_errors,
        }
    }

    pub fn validate_by_nsid(&self, nsid: &str, data: &Data) -> ValidationResult {
        let mut split = nsid.split('#');
        let nsid = split.next().unwrap();
        let def_name = split.next().unwrap_or("main");
        let def = match self.registry.get_def(nsid, def_name) {
            Some(d) => d,
            None => {
                // Schema not found - this is a structural error
                return ValidationResult::StructuralOnly {
                    structural: vec![StructuralError::UnresolvedRef {
                        path: ValidationPath::new(),
                        ref_nsid: format!("{}#{}", nsid, def_name).into(),
                    }],
                };
            }
        };

        let mut path = ValidationPath::new();
        let mut ctx = ValidationContext::new(nsid, def_name);

        let structural_errors = validate_def(&mut path, data, &def, &self.registry, &mut ctx);

        // If structurally invalid, return structural errors only
        if !structural_errors.is_empty() {
            return ValidationResult::StructuralOnly {
                structural: structural_errors,
            };
        }

        // Structurally valid - compute constraints eagerly
        let mut path = ValidationPath::new();
        let constraint_errors = validate_constraints(
            &mut path,
            data,
            nsid,
            def_name,
            Some(&Arc::new(self.registry.clone())),
        );

        ValidationResult::Complete {
            structural: structural_errors,
            constraints: constraint_errors,
        }
    }

    /// Get the schema registry
    pub fn registry(&self) -> &SchemaRegistry {
        &self.registry
    }
}

impl Default for SchemaValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Validation context for tracking refs and preventing cycles
struct ValidationContext {
    current_nsid: String,
    current_def: String,
    ref_stack: Vec<String>,
    max_depth: usize,
}

impl ValidationContext {
    fn new(nsid: &str, def_name: &str) -> Self {
        Self {
            current_nsid: nsid.to_string(),
            current_def: def_name.to_string(),
            ref_stack: Vec::new(),
            max_depth: 32,
        }
    }
}

/// Validate data against a lexicon def
fn validate_def(
    path: &mut ValidationPath,
    data: &Data,
    def: &crate::lexicon::LexUserType,
    registry: &SchemaRegistry,
    ctx: &mut ValidationContext,
) -> Vec<StructuralError> {
    use crate::lexicon::LexUserType;
    use jacquard_common::types::DataModelType;

    match def {
        LexUserType::Object(obj) => {
            // Must be an object
            let Data::Object(obj_data) = data else {
                return vec![StructuralError::TypeMismatch {
                    path: path.clone(),
                    expected: DataModelType::Object,
                    actual: data.data_type(),
                }];
            };

            let mut errors = Vec::new();

            // Check required fields
            if let Some(required) = &obj.required {
                for field in required {
                    if !obj_data.get(field.as_ref()).is_some() {
                        errors.push(StructuralError::MissingRequiredField {
                            path: path.clone(),
                            field: field.clone(),
                        });
                    }
                }
            }

            // Validate each property that's present
            for (name, prop) in &obj.properties {
                if let Some(field_data) = obj_data.get(name.as_ref()) {
                    path.push_field(name.as_ref());
                    errors.extend(validate_property(path, field_data, prop, registry, ctx));
                    path.pop();
                }
            }

            errors
        }
        LexUserType::Record(rec) => {
            // Records are objects with record-specific metadata
            let crate::lexicon::LexRecordRecord::Object(obj) = &rec.record;

            let Data::Object(obj_data) = data else {
                return vec![StructuralError::TypeMismatch {
                    path: path.clone(),
                    expected: data.data_type(),
                    actual: DataModelType::Object,
                }];
            };

            let mut errors = Vec::new();

            // Check required fields
            if let Some(required) = &obj.required {
                for field in required {
                    if !obj_data.get(field.as_ref()).is_some() {
                        errors.push(StructuralError::MissingRequiredField {
                            path: path.clone(),
                            field: field.clone(),
                        });
                    }
                }
            }

            // Validate each property that's present
            for (name, prop) in &obj.properties {
                if let Some(field_data) = obj_data.get(name.as_ref()) {
                    path.push_field(name.as_ref());
                    errors.extend(validate_property(path, field_data, prop, registry, ctx));
                    path.pop();
                }
            }

            errors
        }
        // Token types are unit types, no validation needed beyond type checking
        LexUserType::Token(_) => Vec::new(),
        // XRPC types are endpoint definitions, not data types
        LexUserType::XrpcQuery(_)
        | LexUserType::XrpcProcedure(_)
        | LexUserType::XrpcSubscription(_) => Vec::new(),
        // Other types
        _ => Vec::new(),
    }
}

/// Validate data against a property schema
fn validate_property(
    path: &mut ValidationPath,
    data: &Data,
    prop: &crate::lexicon::LexObjectProperty,
    registry: &SchemaRegistry,
    ctx: &mut ValidationContext,
) -> Vec<StructuralError> {
    use crate::lexicon::LexObjectProperty;
    use jacquard_common::types::DataModelType;

    match prop {
        LexObjectProperty::String(_) => {
            // Accept any string type
            if !matches!(data.data_type(), DataModelType::String(_)) {
                vec![StructuralError::TypeMismatch {
                    path: path.clone(),
                    expected: DataModelType::String(
                        jacquard_common::types::LexiconStringType::String,
                    ),
                    actual: data.data_type(),
                }]
            } else {
                Vec::new()
            }
        }

        LexObjectProperty::Integer(_) => {
            if !matches!(data.data_type(), DataModelType::Integer) {
                vec![StructuralError::TypeMismatch {
                    path: path.clone(),
                    expected: DataModelType::Integer,
                    actual: data.data_type(),
                }]
            } else {
                Vec::new()
            }
        }

        LexObjectProperty::Boolean(_) => {
            if !matches!(data.data_type(), DataModelType::Boolean) {
                vec![StructuralError::TypeMismatch {
                    path: path.clone(),
                    expected: DataModelType::Boolean,
                    actual: data.data_type(),
                }]
            } else {
                Vec::new()
            }
        }

        LexObjectProperty::Object(obj) => {
            let Data::Object(obj_data) = data else {
                return vec![StructuralError::TypeMismatch {
                    path: path.clone(),
                    expected: DataModelType::Object,
                    actual: data.data_type(),
                }];
            };

            let mut errors = Vec::new();

            // Check required fields
            if let Some(required) = &obj.required {
                for field in required {
                    if !obj_data.get(field.as_ref()).is_some() {
                        errors.push(StructuralError::MissingRequiredField {
                            path: path.clone(),
                            field: field.clone(),
                        });
                    }
                }
            }

            // Recursively validate each property
            for (name, schema_prop) in &obj.properties {
                if let Some(field_data) = obj_data.get(name.as_ref()) {
                    path.push_field(name.as_ref());
                    errors.extend(validate_property(
                        path,
                        field_data,
                        schema_prop,
                        registry,
                        ctx,
                    ));
                    path.pop();
                }
            }

            errors
        }

        LexObjectProperty::Array(arr) => {
            let Data::Array(array) = data else {
                return vec![StructuralError::TypeMismatch {
                    path: path.clone(),
                    expected: DataModelType::Array,
                    actual: data.data_type(),
                }];
            };

            let mut errors = Vec::new();
            for (idx, item) in array.iter().enumerate() {
                path.push_index(idx);
                errors.extend(validate_array_item(path, item, &arr.items, registry, ctx));
                path.pop();
            }
            errors
        }

        LexObjectProperty::Union(u) => {
            let Data::Object(obj) = data else {
                return vec![StructuralError::TypeMismatch {
                    path: path.clone(),
                    expected: DataModelType::Object,
                    actual: data.data_type(),
                }];
            };

            // Get $type discriminator
            let Some(type_str) = obj.type_discriminator() else {
                return vec![StructuralError::MissingUnionDiscriminator { path: path.clone() }];
            };

            // Reject empty $type
            if type_str.is_empty() {
                return vec![StructuralError::MissingUnionDiscriminator { path: path.clone() }];
            }

            // Try to match against refs
            for variant_ref in &u.refs {
                let ref_path = RefPath::parse(variant_ref.as_ref(), Some(&ctx.current_nsid));
                let variant_nsid = ref_path.nsid().to_string();
                let variant_def = ref_path.def().to_string();
                let full_variant = ref_path.full_ref();

                // Match by full ref or just nsid
                if type_str == full_variant || type_str == variant_nsid {
                    // Found match - validate against this variant
                    let Some(variant_def_type) = registry.get_def(&variant_nsid, &variant_def)
                    else {
                        return vec![StructuralError::UnresolvedRef {
                            path: path.clone(),
                            ref_nsid: full_variant.into(),
                        }];
                    };

                    path.push_variant(type_str);
                    let old_nsid = std::mem::replace(&mut ctx.current_nsid, variant_nsid);
                    let old_def = std::mem::replace(&mut ctx.current_def, variant_def);

                    let errors = validate_def(path, data, &variant_def_type, registry, ctx);

                    ctx.current_nsid = old_nsid;
                    ctx.current_def = old_def;
                    path.pop();

                    return errors;
                }
            }

            // No match found
            if u.closed.unwrap_or(false) {
                // Closed union - this is an error
                let expected_refs = u
                    .refs
                    .iter()
                    .map(|r| r.as_ref())
                    .collect::<Vec<_>>()
                    .join(", ");
                vec![StructuralError::UnionNoMatch {
                    path: path.clone(),
                    actual_type: type_str.into(),
                    expected_refs: expected_refs.into(),
                }]
            } else {
                // Open union - allow unknown variants
                Vec::new()
            }
        }

        LexObjectProperty::Ref(r) => {
            // Depth check
            if path.depth() >= ctx.max_depth {
                return vec![StructuralError::MaxDepthExceeded {
                    path: path.clone(),
                    max: ctx.max_depth,
                }];
            }

            // Normalize ref
            let ref_path = RefPath::parse(r.r#ref.as_ref(), Some(&ctx.current_nsid));
            let ref_nsid = ref_path.nsid().to_string();
            let ref_def = ref_path.def().to_string();
            let full_ref = ref_path.full_ref();

            // Cycle detection
            if ctx.ref_stack.contains(&full_ref) {
                let stack = ctx.ref_stack.join(" -> ");
                return vec![StructuralError::RefCycle {
                    path: path.clone(),
                    ref_nsid: full_ref.into(),
                    stack: stack.into(),
                }];
            }

            // Look up ref
            let Some(ref_def_type) = registry.get_def(&ref_nsid, &ref_def) else {
                return vec![StructuralError::UnresolvedRef {
                    path: path.clone(),
                    ref_nsid: full_ref.into(),
                }];
            };

            // Push, validate, pop
            ctx.ref_stack.push(full_ref);
            let old_nsid = std::mem::replace(&mut ctx.current_nsid, ref_nsid);
            let old_def = std::mem::replace(&mut ctx.current_def, ref_def);

            let errors = validate_def(path, data, &ref_def_type, registry, ctx);

            ctx.current_nsid = old_nsid;
            ctx.current_def = old_def;
            ctx.ref_stack.pop();

            errors
        }

        LexObjectProperty::Bytes(_) => {
            if !matches!(data.data_type(), DataModelType::Bytes) {
                vec![StructuralError::TypeMismatch {
                    path: path.clone(),
                    expected: DataModelType::Bytes,
                    actual: data.data_type(),
                }]
            } else {
                Vec::new()
            }
        }

        LexObjectProperty::CidLink(_) => {
            if !matches!(data.data_type(), DataModelType::CidLink) {
                vec![StructuralError::TypeMismatch {
                    path: path.clone(),
                    expected: DataModelType::CidLink,
                    actual: data.data_type(),
                }]
            } else {
                Vec::new()
            }
        }

        LexObjectProperty::Blob(_) => {
            if !matches!(data.data_type(), DataModelType::Blob) {
                vec![StructuralError::TypeMismatch {
                    path: path.clone(),
                    expected: DataModelType::Blob,
                    actual: data.data_type(),
                }]
            } else {
                Vec::new()
            }
        }

        LexObjectProperty::Unknown(_) => {
            // Any type allowed
            Vec::new()
        }
    }
}

/// Validate array item against array item schema
fn validate_array_item(
    path: &mut ValidationPath,
    data: &Data,
    item_schema: &LexArrayItem,
    registry: &SchemaRegistry,
    ctx: &mut ValidationContext,
) -> Vec<StructuralError> {
    validate_property(
        path,
        data,
        &item_schema.clone().into_object_property(),
        registry,
        ctx,
    )
}

// ============================================================================
// CONSTRAINT VALIDATION
// ============================================================================

/// Validate constraints on data against schema (entry point with optional registry)
fn validate_constraints(
    path: &mut ValidationPath,
    data: &Data,
    nsid: &str,
    def_name: &str,
    registry: Option<&Arc<SchemaRegistry>>,
) -> Vec<ConstraintError> {
    // Use provided registry or fall back to global inventory
    let fallback_registry;
    let registry_ref = match registry {
        Some(r) => r.as_ref(),
        None => {
            fallback_registry = SchemaRegistry::from_inventory();
            &fallback_registry
        }
    };

    validate_constraints_impl(path, data, nsid, def_name, registry_ref)
}

/// Internal implementation that takes materialized registry
fn validate_constraints_impl(
    path: &mut ValidationPath,
    data: &Data,
    nsid: &str,
    def_name: &str,
    registry: &SchemaRegistry,
) -> Vec<ConstraintError> {
    use crate::lexicon::LexUserType;

    // Get schema def
    let Some(def) = registry.get_def(nsid, def_name) else {
        return Vec::new();
    };

    match def {
        LexUserType::Object(obj) => {
            let Data::Object(obj_data) = data else {
                return Vec::new();
            };

            let mut errors = Vec::new();

            // Check constraints on each property
            for (name, prop) in &obj.properties {
                if let Some(field_data) = obj_data.get(name.as_ref()) {
                    path.push_field(name.as_ref());
                    errors.extend(check_property_constraints(
                        path, field_data, prop, nsid, registry,
                    ));
                    path.pop();
                }
            }

            errors
        }
        LexUserType::Record(rec) => {
            // Records are objects with record-specific metadata
            let crate::lexicon::LexRecordRecord::Object(obj) = &rec.record;

            let Data::Object(obj_data) = data else {
                return Vec::new();
            };

            let mut errors = Vec::new();

            // Check constraints on each property
            for (name, prop) in &obj.properties {
                if let Some(field_data) = obj_data.get(name.as_ref()) {
                    path.push_field(name.as_ref());
                    errors.extend(check_property_constraints(
                        path, field_data, prop, nsid, registry,
                    ));
                    path.pop();
                }
            }

            errors
        }
        // Token types, XRPC types, and other types don't have constraints
        _ => Vec::new(),
    }
}

/// Check constraints on a property
fn check_property_constraints(
    path: &mut ValidationPath,
    data: &Data,
    prop: &crate::lexicon::LexObjectProperty,
    current_nsid: &str,
    registry: &SchemaRegistry,
) -> Vec<ConstraintError> {
    use crate::lexicon::LexObjectProperty;

    match prop {
        LexObjectProperty::String(s) => {
            if let Data::String(str_val) = data {
                check_string_constraints(path, str_val.as_str(), s)
            } else {
                Vec::new()
            }
        }

        LexObjectProperty::Integer(i) => {
            if let Data::Integer(int_val) = data {
                check_integer_constraints(path, *int_val, i)
            } else {
                Vec::new()
            }
        }

        LexObjectProperty::Array(arr) => {
            if let Data::Array(array) = data {
                let mut errors = check_array_constraints(path, array, arr);

                // Also check constraints on array items
                for (idx, item) in array.iter().enumerate() {
                    path.push_index(idx);
                    errors.extend(check_array_item_constraints(
                        path,
                        item,
                        &arr.items,
                        current_nsid,
                        registry,
                    ));
                    path.pop();
                }

                errors
            } else {
                Vec::new()
            }
        }

        LexObjectProperty::Object(obj) => {
            if let Data::Object(obj_data) = data {
                let mut errors = Vec::new();

                // Recursively check nested object properties
                for (name, schema_prop) in &obj.properties {
                    if let Some(field_data) = obj_data.get(name.as_ref()) {
                        path.push_field(name.as_ref());
                        errors.extend(check_property_constraints(
                            path,
                            field_data,
                            schema_prop,
                            current_nsid,
                            registry,
                        ));
                        path.pop();
                    }
                }

                errors
            } else {
                Vec::new()
            }
        }

        LexObjectProperty::Ref(r) => {
            // Follow ref and check constraints
            let ref_path = RefPath::parse(r.r#ref.as_ref(), Some(current_nsid));
            let ref_nsid = ref_path.nsid();
            let ref_def = ref_path.def();

            if registry.get_def(ref_nsid, ref_def).is_some() {
                validate_constraints_impl(path, data, ref_nsid, ref_def, registry)
            } else {
                Vec::new()
            }
        }

        // Other property types don't have constraints
        _ => Vec::new(),
    }
}

/// Check string constraints
fn check_string_constraints(
    path: &ValidationPath,
    value: &str,
    schema: &crate::lexicon::LexString,
) -> Vec<ConstraintError> {
    let mut errors = Vec::new();

    // Check byte length constraints
    let byte_len = value.len();

    if let Some(min) = schema.min_length {
        if byte_len < min as usize {
            errors.push(ConstraintError::MinLength {
                path: path.clone(),
                min: min as usize,
                actual: byte_len,
            });
        }
    }

    if let Some(max) = schema.max_length {
        if byte_len > max as usize {
            errors.push(ConstraintError::MaxLength {
                path: path.clone(),
                max: max as usize,
                actual: byte_len,
            });
        }
    }

    // Check grapheme count constraints
    if schema.min_graphemes.is_some() || schema.max_graphemes.is_some() {
        use unicode_segmentation::UnicodeSegmentation;
        let grapheme_count = value.graphemes(true).count();

        if let Some(min) = schema.min_graphemes {
            if grapheme_count < min as usize {
                errors.push(ConstraintError::MinGraphemes {
                    path: path.clone(),
                    min: min as usize,
                    actual: grapheme_count,
                });
            }
        }

        if let Some(max) = schema.max_graphemes {
            if grapheme_count > max as usize {
                errors.push(ConstraintError::MaxGraphemes {
                    path: path.clone(),
                    max: max as usize,
                    actual: grapheme_count,
                });
            }
        }
    }

    errors
}

/// Check integer constraints
fn check_integer_constraints(
    path: &ValidationPath,
    value: i64,
    schema: &crate::lexicon::LexInteger,
) -> Vec<ConstraintError> {
    let mut errors = Vec::new();

    if let Some(min) = schema.minimum {
        if value < min {
            errors.push(ConstraintError::Minimum {
                path: path.clone(),
                min,
                actual: value,
            });
        }
    }

    if let Some(max) = schema.maximum {
        if value > max {
            errors.push(ConstraintError::Maximum {
                path: path.clone(),
                max,
                actual: value,
            });
        }
    }

    errors
}

/// Check array length constraints
fn check_array_constraints(
    path: &ValidationPath,
    array: &jacquard_common::types::value::Array,
    schema: &crate::lexicon::LexArray,
) -> Vec<ConstraintError> {
    let mut errors = Vec::new();
    let len = array.len();

    if let Some(min) = schema.min_length {
        if len < min as usize {
            errors.push(ConstraintError::MinLength {
                path: path.clone(),
                min: min as usize,
                actual: len,
            });
        }
    }

    if let Some(max) = schema.max_length {
        if len > max as usize {
            errors.push(ConstraintError::MaxLength {
                path: path.clone(),
                max: max as usize,
                actual: len,
            });
        }
    }

    errors
}

/// Check constraints on array items
fn check_array_item_constraints(
    path: &mut ValidationPath,
    data: &Data,
    item_schema: &LexArrayItem,
    current_nsid: &str,
    registry: &SchemaRegistry,
) -> Vec<ConstraintError> {
    check_property_constraints(
        path,
        data,
        &item_schema.clone().into_object_property(),
        current_nsid,
        registry,
    )
}

#[cfg(test)]
mod tests;
