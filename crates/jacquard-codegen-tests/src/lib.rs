//! Compile-time validation crate for Pretty and Macro codegen modes.
//!
//! This crate has no runtime functionality. Its purpose is to verify that
//! code generated in both Pretty mode (short names + use blocks) and Macro mode
//! (fully-qualified paths) compiles successfully. The build script generates
//! code from curated test lexicons into `src/generated/{pretty,macro_mode}/`.
//!
//! If this crate compiles, both codegen modes produce valid Rust.

extern crate alloc;

// Both generated module trees emit `use crate::builder_types::{...}` in
// their state modules. This is hardcoded in the codegen, so re-export from
// one of the generated copies at the crate root.
#[path = "generated/pretty/builder_types.rs"]
pub mod builder_types;

// Pretty mode generated code — module tree with proper path resolution.
// The build script generates with root_module="crate::pretty" so that
// cross-module refs like `crate::pretty::app_bsky::...` resolve correctly
// through the #[path] module boundary.
#[path = "generated/pretty/lib.rs"]
pub mod pretty;

// Macro mode generated code (root_module="crate::macro_mode").
#[path = "generated/macro_mode/lib.rs"]
pub mod macro_mode;

#[cfg(test)]
mod tests {
    // -- Pretty mode type accessibility --

    #[test]
    fn pretty_strong_ref_accessible() {
        // Compile-time type check: strongRef from com.atproto.repo.
        let _: Option<super::pretty::com_atproto::repo::strong_ref::StrongRef> = None;
    }

    #[test]
    fn pretty_facet_types_accessible() {
        // Compile-time type check: facet and its sub-defs.
        let _: Option<super::pretty::app_bsky::richtext::facet::Facet> = None;
        let _: Option<super::pretty::app_bsky::richtext::facet::Mention> = None;
        let _: Option<super::pretty::app_bsky::richtext::facet::Link> = None;
        let _: Option<super::pretty::app_bsky::richtext::facet::Tag> = None;
        let _: Option<super::pretty::app_bsky::richtext::facet::ByteSlice> = None;
    }

    #[test]
    fn pretty_label_defs_accessible() {
        // Compile-time type check: defs types from com.atproto.label.
        let _: Option<super::pretty::com_atproto::label::Label> = None;
    }

    #[test]
    fn pretty_embed_external_accessible() {
        // Compile-time type check: embed external types.
        let _: Option<super::pretty::app_bsky::embed::external::ExternalRecord> = None;
        let _: Option<super::pretty::app_bsky::embed::external::External> = None;
        let _: Option<super::pretty::app_bsky::embed::external::View> = None;
        let _: Option<super::pretty::app_bsky::embed::external::ViewExternal> = None;
    }

    #[test]
    fn pretty_collision_collection_accessible() {
        // Compile-time type check: local Collection type compiles despite trait name collision.
        // The record is named CollectionRecord, the local def is Collection.
        let _: Option<super::pretty::test_collision::collection::CollectionRecord> = None;
        let _: Option<super::pretty::test_collision::collection::Collection> = None;
    }

    #[test]
    fn pretty_collision_did_accessible() {
        // Compile-time type check: local Did type compiles despite string type name collision.
        // The main def is DidRecord, the local def is Did.
        let _: Option<super::pretty::test_collision::did::DidRecord> = None;
        let _: Option<super::pretty::test_collision::did::Did> = None;
    }

    #[test]
    fn pretty_collision_option_accessible() {
        // Compile-time type check: local Option type compiles despite std Option collision.
        // The main def is OptionRecord, the local def is OptionRecordOption.
        let _: Option<super::pretty::test_collision::option::OptionRecord> = None;
        let _: Option<super::pretty::test_collision::option::OptionRecordOption> = None;
    }

    #[test]
    fn pretty_cross_namespace_ns1_accessible() {
        // Compile-time type check: types defined in test.ns1.defs.
        let _: Option<super::pretty::test_ns1::Foo> = None;
        let _: Option<super::pretty::test_ns1::Bar> = None;
    }

    #[test]
    fn pretty_cross_namespace_ns2_accessible() {
        // Compile-time type check: types in test.ns2 that reference test.ns1.
        let _: Option<super::pretty::test_ns2::consumer::Consumer> = None;
    }

    #[test]
    fn pretty_cross_namespace_ns3_collision_accessible() {
        // Compile-time type check: ns3 defines local Foo AND refs external Foo.
        let _: Option<super::pretty::test_ns3::collision::Collision> = None;
        let _: Option<super::pretty::test_ns3::collision::Foo> = None;
    }

    // -- Macro mode type accessibility (same types, different module root) --

    #[test]
    fn macro_strong_ref_accessible() {
        let _: Option<super::macro_mode::com_atproto::repo::strong_ref::StrongRef> = None;
    }

    #[test]
    fn macro_facet_types_accessible() {
        let _: Option<super::macro_mode::app_bsky::richtext::facet::Facet> = None;
        let _: Option<super::macro_mode::app_bsky::richtext::facet::Mention> = None;
    }

    #[test]
    fn macro_label_defs_accessible() {
        let _: Option<super::macro_mode::com_atproto::label::Label> = None;
    }

    #[test]
    fn macro_collision_collection_accessible() {
        let _: Option<super::macro_mode::test_collision::collection::CollectionRecord> = None;
        let _: Option<super::macro_mode::test_collision::collection::Collection> = None;
    }

    #[test]
    fn macro_cross_namespace_ns1_accessible() {
        let _: Option<super::macro_mode::test_ns1::Foo> = None;
        let _: Option<super::macro_mode::test_ns1::Bar> = None;
    }

    #[test]
    fn macro_cross_namespace_ns2_accessible() {
        let _: Option<super::macro_mode::test_ns2::consumer::Consumer> = None;
    }

    #[test]
    fn macro_cross_namespace_ns3_accessible() {
        let _: Option<super::macro_mode::test_ns3::collision::Collision> = None;
        let _: Option<super::macro_mode::test_ns3::collision::Foo> = None;
    }
}
