use super::utils::sanitize_name;
use super::CodeGenerator;
use heck::{ToPascalCase, ToSnakeCase};

impl<'c> CodeGenerator<'c> {
    /// Check if a single-variant union is self-referential
    pub(super) fn is_self_referential_union(
        &self,
        nsid: &str,
        parent_type_name: &str,
        union: &crate::lexicon::LexRefUnion,
    ) -> bool {
        if union.refs.len() != 1 {
            return false;
        }

        let ref_str = if union.refs[0].starts_with('#') {
            format!("{}{}", nsid, union.refs[0])
        } else {
            union.refs[0].to_string()
        };

        let (ref_nsid, ref_def) = if let Some((nsid_part, fragment)) = ref_str.split_once('#') {
            (nsid_part, fragment)
        } else {
            (ref_str.as_str(), "main")
        };

        let ref_type_name = self.def_to_type_name(ref_nsid, ref_def);
        ref_type_name == parent_type_name
    }

    /// Helper to generate field-based type name with collision detection
    pub(super) fn generate_field_type_name(
        &self,
        nsid: &str,
        parent_type_name: &str,
        field_name: &str,
        suffix: &str, // "" for union/object, "Item" for array unions
    ) -> String {
        let base_name = format!("{}{}{}", parent_type_name, field_name.to_pascal_case(), suffix);

        // Check for collisions with lexicon defs
        if let Some(doc) = self.corpus.get(nsid) {
            let def_names: std::collections::HashSet<String> = doc
                .defs
                .keys()
                .map(|name| self.def_to_type_name(nsid, name.as_ref()))
                .collect();

            if def_names.contains(&base_name) {
                // Use "Union" suffix for union types, "Record" for objects
                let disambiguator = if suffix.is_empty() && !parent_type_name.is_empty() {
                    "Union"
                } else {
                    "Record"
                };
                return format!("{}{}{}{}", parent_type_name, disambiguator, field_name.to_pascal_case(), suffix);
            }
        }

        base_name
    }

    /// Convert lexicon def name to Rust type name
    pub(super) fn def_to_type_name(&self, nsid: &str, def_name: &str) -> String {
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
    pub(super) fn nsid_to_file_path(&self, nsid: &str) -> std::path::PathBuf {
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
}
