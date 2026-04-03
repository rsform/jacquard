use super::CodeGenerator;
use crate::lexicon::{
    LexArrayItem, LexObjectProperty, LexPrimitiveArrayItem, LexString, LexStringFormat,
    LexUserType, LexXrpcParametersProperty,
};

/// Trait for lexicon types that can determine type parameter requirements
trait HasTypeParam {
    /// Check if this type needs a type parameter when generated
    fn needs_type_param(&self, generator: &CodeGenerator) -> bool;
}

impl HasTypeParam for LexObjectProperty<'_> {
    fn needs_type_param(&self, generator: &CodeGenerator) -> bool {
        match self {
            LexObjectProperty::Boolean(_) | LexObjectProperty::Integer(_) => false,
            LexObjectProperty::String(s) => s.needs_type_param(generator),
            LexObjectProperty::Bytes(_) => false, // Bytes is owned
            LexObjectProperty::CidLink(_)
            | LexObjectProperty::Blob(_)
            | LexObjectProperty::Unknown(_) => true,
            LexObjectProperty::Array(array) => array.items.needs_type_param(generator),
            LexObjectProperty::Object(_) => true, // Nested objects have type params
            LexObjectProperty::Ref(ref_type) => generator.ref_needs_type_param(&ref_type.r#ref),
            LexObjectProperty::Union(_) => true, // Unions generally have type params
        }
    }
}

impl HasTypeParam for LexArrayItem<'_> {
    fn needs_type_param(&self, generator: &CodeGenerator) -> bool {
        match self {
            LexArrayItem::Boolean(_) | LexArrayItem::Integer(_) => false,
            LexArrayItem::String(s) => s.needs_type_param(generator),
            LexArrayItem::Bytes(_) => false,
            LexArrayItem::CidLink(_) | LexArrayItem::Blob(_) | LexArrayItem::Unknown(_) => true,
            LexArrayItem::Object(_) => true, // Nested objects have type params
            LexArrayItem::Ref(ref_type) => generator.ref_needs_type_param(&ref_type.r#ref),
            LexArrayItem::Union(_) => true,
        }
    }
}

impl HasTypeParam for LexString<'_> {
    fn needs_type_param(&self, _generator: &CodeGenerator) -> bool {
        match self.format {
            Some(LexStringFormat::Datetime)
            | Some(LexStringFormat::Language)
            | Some(LexStringFormat::Tid) => false,
            _ => true, // Most string types are parameterized on S
        }
    }
}

impl HasTypeParam for LexUserType<'_> {
    fn needs_type_param(&self, generator: &CodeGenerator) -> bool {
        match self {
            LexUserType::Record(_) => true,
            LexUserType::Object(_) => true,
            LexUserType::Token(_) => false,
            LexUserType::String(s) => {
                // Check if it's a known values enum or a regular string
                if s.known_values.is_some() {
                    // Known values enums have Other(S) variant
                    true
                } else {
                    s.needs_type_param(generator)
                }
            }
            LexUserType::Integer(_) => false,
            LexUserType::Boolean(_) => false,
            LexUserType::Bytes(_) => false,
            LexUserType::CidLink(_) | LexUserType::Blob(_) | LexUserType::Unknown(_) => true,
            LexUserType::Array(array) => array.items.needs_type_param(generator),
            LexUserType::XrpcQuery(_)
            | LexUserType::XrpcProcedure(_)
            | LexUserType::XrpcSubscription(_) => {
                // XRPC types generate multiple structs, not a single type we can reference
                // Shouldn't be referenced directly
                true
            }
            LexUserType::Union(_) => true, // Union enums are always generated with <S>
            LexUserType::PermissionSet(_) => true, // Permission sets are parameterized on S
        }
    }
}

impl HasTypeParam for LexXrpcParametersProperty<'_> {
    fn needs_type_param(&self, generator: &CodeGenerator) -> bool {
        match self {
            LexXrpcParametersProperty::Boolean(_) | LexXrpcParametersProperty::Integer(_) => false,
            LexXrpcParametersProperty::String(s) => s.needs_type_param(generator),
            LexXrpcParametersProperty::Unknown(_) => true,
            LexXrpcParametersProperty::Array(arr) => arr.items.needs_type_param(generator),
        }
    }
}

impl HasTypeParam for LexPrimitiveArrayItem<'_> {
    fn needs_type_param(&self, generator: &CodeGenerator) -> bool {
        match self {
            LexPrimitiveArrayItem::Boolean(_) | LexPrimitiveArrayItem::Integer(_) => false,
            LexPrimitiveArrayItem::String(s) => s.needs_type_param(generator),
            LexPrimitiveArrayItem::Unknown(_) => true,
        }
    }
}

impl<'c> CodeGenerator<'c> {
    /// Check if an array item type needs a type parameter
    pub(super) fn array_item_needs_type_param(&self, item: &LexArrayItem<'_>) -> bool {
        item.needs_type_param(self)
    }

    /// Check if a string type needs a type parameter
    pub(super) fn string_needs_type_param(&self, s: &LexString<'_>) -> bool {
        s.needs_type_param(self)
    }

    /// Check if a ref needs a type parameter
    pub(super) fn ref_needs_type_param(&self, ref_str: &str) -> bool {
        // Try to resolve the ref
        if let Some((_doc, def)) = self.corpus.resolve_ref(ref_str) {
            def.needs_type_param(self)
        } else {
            // If we can't resolve it, assume it needs a type parameter (safe default)
            true
        }
    }

    /// Check if xrpc params need a type parameter
    pub(super) fn params_need_type_param(
        &self,
        params: &crate::lexicon::LexXrpcParameters<'_>,
    ) -> bool {
        params
            .properties
            .values()
            .any(|prop| prop.needs_type_param(self))
    }
}
