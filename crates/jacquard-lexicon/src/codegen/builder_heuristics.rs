use crate::lexicon::{LexObject, LexObjectProperty};

/// Decision about whether to generate builder and/or Default impl for a struct.
#[derive(Debug, Clone, Copy)]
pub struct BuilderDecision {
    /// Whether to generate a bon::Builder derive
    pub has_builder: bool,
    /// Whether to generate a Default derive
    pub has_default: bool,
}

/// Determine whether a struct should have builder and/or Default based on heuristics.
///
/// Rules:
/// - 0 required fields → Default (no builder)
/// - All required fields are bare strings → Default (no builder)
/// - 1+ required fields (not all strings) → Builder (no Default)
/// - Type name conflicts with bon::Builder → no builder regardless
///
/// # Parameters
/// - `type_name`: Name of the generated struct (to check for conflicts)
/// - `obj`: Lexicon object schema
///
/// # Returns
/// Decision about builder and Default generation
pub fn should_generate_builder(type_name: &str, obj: &LexObject<'static>) -> BuilderDecision {
    let required_count = count_required_fields(obj);
    let has_default = required_count == 0 || all_required_are_defaultable_strings(obj);
    let has_builder =
        required_count >= 1 && !has_default && !conflicts_with_builder_macro(type_name);

    BuilderDecision {
        has_builder,
        has_default,
    }
}

/// Check if a type name conflicts with types referenced by bon::Builder macro.
/// bon::Builder generates code that uses unqualified `Option` and `Result`,
/// so structs with these names cause compilation errors.
///
/// This is public for cases where a struct always wants a builder (like records)
/// but needs to check for conflicts.
pub fn conflicts_with_builder_macro(type_name: &str) -> bool {
    matches!(type_name, "Option" | "Result")
}

/// Count the number of required fields in a lexicon object.
/// Used to determine whether to generate builders or Default impls.
fn count_required_fields(obj: &LexObject<'static>) -> usize {
    let required = obj.required.as_ref().map(|r| r.as_slice()).unwrap_or(&[]);
    required.len()
}

/// Check if a field property is a plain string that can default to empty.
/// Returns true for bare CowStr fields (no format constraints).
fn is_defaultable_string(prop: &LexObjectProperty<'static>) -> bool {
    matches!(prop, LexObjectProperty::String(s) if s.format.is_none())
}

/// Check if all required fields in an object are defaultable strings.
fn all_required_are_defaultable_strings(obj: &LexObject<'static>) -> bool {
    let required = obj.required.as_ref().map(|r| r.as_slice()).unwrap_or(&[]);

    if required.is_empty() {
        return false; // Handled separately by count check
    }

    required.iter().all(|field_name| {
        let field_name_str: &str = field_name.as_ref();
        obj.properties
            .get(field_name_str)
            .map(is_defaultable_string)
            .unwrap_or(false)
    })
}
