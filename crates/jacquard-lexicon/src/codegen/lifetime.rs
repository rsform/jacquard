use super::CodeGenerator;
use crate::lexicon::{
    LexArrayItem, LexObjectProperty, LexString, LexStringFormat, LexUserType,
};

impl<'c> CodeGenerator<'c> {
    /// Check if a property type needs a lifetime parameter
    pub(super) fn property_needs_lifetime(&self, prop: &LexObjectProperty<'static>) -> bool {
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
    pub(super) fn array_item_needs_lifetime(&self, item: &LexArrayItem) -> bool {
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
    pub(super) fn string_needs_lifetime(&self, s: &LexString) -> bool {
        match s.format {
            Some(LexStringFormat::Datetime)
            | Some(LexStringFormat::Language)
            | Some(LexStringFormat::Tid) => false,
            _ => true, // Most string types borrow
        }
    }

    /// Check if a ref needs a lifetime parameter
    pub(super) fn ref_needs_lifetime(&self, ref_str: &str) -> bool {
        // Try to resolve the ref
        if let Some((_doc, def)) = self.corpus.resolve_ref(ref_str) {
            self.def_needs_lifetime(def)
        } else {
            // If we can't resolve it, assume it needs a lifetime (safe default)
            true
        }
    }

    /// Check if a lexicon def needs a lifetime parameter
    pub(super) fn def_needs_lifetime(&self, def: &LexUserType<'static>) -> bool {
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
    pub(super) fn params_need_lifetime(&self, params: &crate::lexicon::LexXrpcParameters<'static>) -> bool {
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
}
