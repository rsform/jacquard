use crate::corpus::LexiconCorpus;
use crate::lexicon::{
    LexArrayItem, LexObjectProperty, LexUserType, LexXrpcBodySchema,
    LexXrpcSubscriptionMessageSchema,
};
use jacquard_common::smol_str::{SmolStr, ToSmolStr};
use jacquard_common::{CowStr, smol_str};
use std::collections::{BTreeMap, BTreeSet};

/// Information about a single union type found in the corpus
#[derive(Debug, Clone)]
pub struct UnionInfo {
    /// NSID of the lexicon containing this union
    pub lexicon_nsid: SmolStr,
    /// Name of the def containing this union (e.g., "main", "replyRef")
    pub def_name: SmolStr,
    /// Field path within the def (e.g., "embed", "properties.embed")
    pub field_path: CowStr<'static>,
    /// Refs that exist in the corpus
    pub known_refs: Vec<CowStr<'static>>,
    /// Refs that don't exist in the corpus
    pub unknown_refs: Vec<CowStr<'static>>,
    /// Whether the union is closed (default true if not specified)
    pub closed: bool,
}

impl UnionInfo {
    /// Get the source text for this union's lexicon from the corpus
    pub fn get_source<'c>(&self, corpus: &'c LexiconCorpus) -> Option<&'c str> {
        corpus.get_source(&self.lexicon_nsid)
    }

    /// Check if this union has any unknown refs
    pub fn has_unknown_refs(&self) -> bool {
        !self.unknown_refs.is_empty()
    }

    /// Get all refs (known + unknown)
    pub fn all_refs(&self) -> impl Iterator<Item = &CowStr<'static>> {
        self.known_refs.iter().chain(self.unknown_refs.iter())
    }
}

/// Registry of all union types found in the corpus
#[derive(Debug, Clone)]
pub struct UnionRegistry {
    /// Map from union identifier to union info
    /// Key is "{lexicon_nsid}#{def_name}:{field_path}"
    unions: BTreeMap<SmolStr, UnionInfo>,
}

impl UnionRegistry {
    /// Create a new empty union registry
    pub fn new() -> Self {
        Self {
            unions: BTreeMap::new(),
        }
    }

    /// Build a union registry from a corpus
    pub fn from_corpus(corpus: &LexiconCorpus) -> Self {
        let mut registry = Self::new();

        for (nsid, doc) in corpus.iter() {
            for (def_name, def) in &doc.defs {
                registry.collect_unions_from_def(corpus, nsid, def_name, def);
            }
        }

        registry
    }

    /// Collect unions from a single def
    fn collect_unions_from_def(
        &mut self,
        corpus: &LexiconCorpus,
        nsid: &SmolStr,
        def_name: &SmolStr,
        def: &LexUserType<'static>,
    ) {
        match def {
            LexUserType::Record(record) => match &record.record {
                crate::lexicon::LexRecordRecord::Object(obj) => {
                    self.collect_unions_from_object(corpus, nsid, def_name, "", obj);
                }
            },
            LexUserType::Object(obj) => {
                self.collect_unions_from_object(corpus, nsid, def_name, "", obj);
            }
            LexUserType::XrpcQuery(query) => {
                if let Some(output) = &query.output {
                    if let Some(schema) = &output.schema {
                        self.collect_unions_from_xrpc_body_schema(
                            corpus, nsid, def_name, "output", schema,
                        );
                    }
                }
            }
            LexUserType::XrpcProcedure(proc) => {
                if let Some(input) = &proc.input {
                    if let Some(schema) = &input.schema {
                        self.collect_unions_from_xrpc_body_schema(
                            corpus, nsid, def_name, "input", schema,
                        );
                    }
                }
                if let Some(output) = &proc.output {
                    if let Some(schema) = &output.schema {
                        self.collect_unions_from_xrpc_body_schema(
                            corpus, nsid, def_name, "output", schema,
                        );
                    }
                }
            }
            LexUserType::XrpcSubscription(sub) => {
                if let Some(message) = &sub.message {
                    if let Some(schema) = &message.schema {
                        self.collect_unions_from_subscription_message_schema(
                            corpus, nsid, def_name, "message", schema,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    /// Collect unions from an object's properties
    fn collect_unions_from_object(
        &mut self,
        corpus: &LexiconCorpus,
        nsid: &SmolStr,
        def_name: &SmolStr,
        path_prefix: &str,
        obj: &crate::lexicon::LexObject<'static>,
    ) {
        for (prop_name, prop) in &obj.properties {
            let prop_path = if path_prefix.is_empty() {
                prop_name.to_smolstr()
            } else {
                smol_str::format_smolstr!("{}.{}", path_prefix, prop_name)
            };

            match prop {
                LexObjectProperty::Union(union) => {
                    self.register_union(
                        corpus,
                        nsid,
                        def_name,
                        &prop_path,
                        &union.refs,
                        union.closed,
                    );
                }
                LexObjectProperty::Array(array) => {
                    if let LexArrayItem::Union(union) = &array.items {
                        let array_path = format!("{}[]", prop_path);
                        self.register_union(
                            corpus,
                            nsid,
                            def_name,
                            &array_path,
                            &union.refs,
                            union.closed,
                        );
                    }
                }
                LexObjectProperty::Ref(ref_type) => {
                    // Check if ref points to a union
                    if let Some((_, ref_def)) = corpus.resolve_ref(ref_type.r#ref.as_ref()) {
                        if matches!(ref_def, LexUserType::Object(_)) {
                            // Recursively check the referenced object
                            // (we'll handle this in a future iteration if needed)
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Collect unions from XRPC body schema
    fn collect_unions_from_xrpc_body_schema(
        &mut self,
        corpus: &LexiconCorpus,
        nsid: &SmolStr,
        def_name: &SmolStr,
        path: &str,
        schema: &LexXrpcBodySchema<'static>,
    ) {
        match schema {
            LexXrpcBodySchema::Union(union) => {
                self.register_union(corpus, nsid, def_name, path, &union.refs, union.closed);
            }
            LexXrpcBodySchema::Object(obj) => {
                self.collect_unions_from_object(corpus, nsid, def_name, path, obj);
            }
            _ => {}
        }
    }

    /// Collect unions from subscription message schema
    fn collect_unions_from_subscription_message_schema(
        &mut self,
        corpus: &LexiconCorpus,
        nsid: &SmolStr,
        def_name: &SmolStr,
        path: &str,
        schema: &LexXrpcSubscriptionMessageSchema<'static>,
    ) {
        match schema {
            LexXrpcSubscriptionMessageSchema::Union(union) => {
                self.register_union(corpus, nsid, def_name, path, &union.refs, union.closed);
            }
            LexXrpcSubscriptionMessageSchema::Object(obj) => {
                self.collect_unions_from_object(corpus, nsid, def_name, path, obj);
            }
            _ => {}
        }
    }

    /// Register a union with the registry
    fn register_union(
        &mut self,
        corpus: &LexiconCorpus,
        nsid: &SmolStr,
        def_name: &SmolStr,
        field_path: &str,
        refs: &[jacquard_common::CowStr<'static>],
        closed: Option<bool>,
    ) {
        let mut known_refs = Vec::new();
        let mut unknown_refs = Vec::new();

        for ref_str in refs {
            if corpus.ref_exists(&ref_str) {
                known_refs.push(ref_str.clone());
            } else {
                unknown_refs.push(ref_str.clone());
            }
        }

        let key = smol_str::format_smolstr!("{}#{}:{}", nsid, def_name, field_path);
        self.unions.insert(
            key,
            UnionInfo {
                lexicon_nsid: nsid.clone(),
                def_name: def_name.clone(),
                field_path: CowStr::Owned(field_path.to_smolstr()),
                known_refs,
                unknown_refs,
                closed: closed.unwrap_or(true),
            },
        );
    }

    /// Get all unions
    pub fn iter(&self) -> impl Iterator<Item = (&SmolStr, &UnionInfo)> {
        self.unions.iter()
    }

    /// Get a specific union
    pub fn get(&self, key: &str) -> Option<&UnionInfo> {
        self.unions.get(key)
    }

    /// Number of unions in registry
    pub fn len(&self) -> usize {
        self.unions.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.unions.is_empty()
    }

    /// Get all unique refs across all unions
    pub fn all_refs(&self) -> BTreeSet<CowStr<'static>> {
        let mut refs = BTreeSet::new();
        for union in self.unions.values() {
            refs.extend(union.known_refs.iter().cloned());
            refs.extend(union.unknown_refs.iter().cloned());
        }
        refs
    }
}

impl Default for UnionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_union_registry_from_corpus() {
        let corpus = LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons")
            .expect("failed to load lexicons");

        let registry = UnionRegistry::from_corpus(&corpus);

        assert!(!registry.is_empty());

        // Check that we found the embed union in post
        let post_embed = registry
            .iter()
            .find(|(_, info)| {
                info.lexicon_nsid == "app.bsky.feed.post"
                    && info.def_name == "main"
                    && info.field_path.contains("embed")
            })
            .expect("should find post embed union");

        let info = post_embed.1;
        assert!(info.known_refs.contains(&"app.bsky.embed.images".into()));
        assert!(info.known_refs.contains(&"app.bsky.embed.video".into()));
        assert!(info.known_refs.contains(&"app.bsky.embed.external".into()));
    }

    #[test]
    fn test_union_registry_tracks_unknown_refs() {
        let corpus = LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons")
            .expect("failed to load lexicons");

        let registry = UnionRegistry::from_corpus(&corpus);

        // If there are any unknown refs, they should be tracked
        for (_, info) in registry.iter() {
            for unknown in &info.unknown_refs {
                assert!(!corpus.ref_exists(unknown));
            }
        }
    }
}
