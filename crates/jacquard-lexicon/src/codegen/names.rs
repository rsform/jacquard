use super::nsid_utils::NsidPath;
use super::utils::{namespace_prefix, sanitize_name, sanitize_name_cow};
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

    /// Convert lexicon def name to base Rust type name (without prelude collision handling)
    fn def_to_base_type_name(&self, nsid: &str, def_name: &str) -> String {
        if def_name == "main" {
            // Use last segment of NSID
            let nsid_path = NsidPath::parse(nsid);
            let base_name = nsid_path.last_segment().to_pascal_case();

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

    /// Apply prelude collision fix if needed
    fn apply_prelude_collision_fix(&self, nsid: &str, def_name: &str, base_name: String) -> String {
        // Prelude types that would shadow if used as type names
        const PRELUDE_TYPES: &[&str] = &[
            "Option", "Result", "String", "Vec", "Box",
            "Some", "None", "Ok", "Err",
        ];

        if !PRELUDE_TYPES.contains(&base_name.as_str()) {
            return base_name;
        }

        // Add contextual prefix to avoid collision
        if def_name == "main" {
            // Use second-to-last NSID segment for main defs
            let nsid_path = NsidPath::parse(nsid);
            let parts = nsid_path.segments();
            if parts.len() >= 2 {
                format!("{}{}", parts[parts.len() - 2].to_pascal_case(), base_name)
            } else {
                format!("Lex{}", base_name) // fallback
            }
        } else {
            // Use main def's type name as prefix for nested defs
            let main_base = self.def_to_base_type_name(nsid, "main");
            format!("{}{}", main_base, base_name)
        }
    }

    /// Convert lexicon def name to Rust type name
    pub(super) fn def_to_type_name(&self, nsid: &str, def_name: &str) -> String {
        let base_name = self.def_to_base_type_name(nsid, def_name);
        self.apply_prelude_collision_fix(nsid, def_name, base_name)
    }

    /// Convert NSID to file path relative to output directory
    ///
    /// - `app.bsky.feed.post` → `app_bsky/feed/post.rs`
    /// - `com.atproto.label.defs` → `com_atproto/label.rs` (defs go in parent)
    pub(super) fn nsid_to_file_path(&self, nsid: &str) -> std::path::PathBuf {
        let nsid_path = NsidPath::parse(nsid);
        let parts = nsid_path.segments();

        if parts.len() < 2 {
            // Shouldn't happen with valid NSIDs, but handle gracefully
            return format!("{}.rs", sanitize_name(parts[0])).into();
        }

        let last = nsid_path.last_segment();

        if nsid_path.is_defs() && parts.len() >= 3 {
            // defs go in parent module: com.atproto.label.defs → com_atproto/label.rs
            let first_two = namespace_prefix(parts[0], parts[1]);
            if parts.len() == 3 {
                // com.atproto.defs → com_atproto.rs
                format!("{}.rs", first_two).into()
            } else {
                // com.atproto.label.defs → com_atproto/label.rs
                let middle: Vec<&str> = parts[2..parts.len() - 1].iter().copied().collect();
                let mut path = std::path::PathBuf::from(first_two);
                for segment in &middle[..middle.len() - 1] {
                    path.push(sanitize_name_cow(segment).as_ref());
                }
                path.push(format!("{}.rs", sanitize_name_cow(middle.last().unwrap())));
                path
            }
        } else {
            // Regular path: app.bsky.feed.post → app_bsky/feed/post.rs
            let first_two = namespace_prefix(parts[0], parts[1]);
            let mut path = std::path::PathBuf::from(first_two);

            for segment in &parts[2..parts.len() - 1] {
                path.push(sanitize_name_cow(segment).as_ref());
            }

            path.push(format!("{}.rs", sanitize_name_cow(&last.to_snake_case())));
            path
        }
    }
}
