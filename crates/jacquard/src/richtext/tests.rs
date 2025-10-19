use super::*;

#[test]
fn test_parse_mentions() {
    let text = "Hey @alice.bsky.social check this out";
    let builder = RichText::parse(text);

    assert_eq!(builder.facet_candidates.len(), 1);
    match &builder.facet_candidates[0] {
        FacetCandidate::Mention { range, .. } => {
            // Verify the text in the range includes the @ symbol
            assert_eq!(&builder.text[range.clone()], "@alice.bsky.social");
        }
        _ => panic!("Expected mention facet"),
    }
}

#[test]
fn test_parse_links() {
    let text = "Check out https://example.com for more info";
    let builder = RichText::parse(text);

    assert!(builder.facet_candidates.iter().any(|fc| {
        matches!(fc, FacetCandidate::Link { range } if text[range.clone()].contains("example.com"))
    }));
}

#[test]
fn test_parse_tags() {
    let text = "This is #cool and #awesome";
    let builder = RichText::parse(text);

    let tags: Vec<_> = builder
        .facet_candidates
        .iter()
        .filter_map(|fc| match fc {
            FacetCandidate::Tag { range } => Some(&builder.text[range.clone()]),
            _ => None,
        })
        .collect();

    assert!(tags.contains(&"#cool"));
    assert!(tags.contains(&"#awesome"));
}

#[test]
fn test_markdown_links() {
    let text = "Check out [this link](https://example.com)";
    let builder = RichText::parse(text);

    // Should have stripped markdown syntax
    assert!(builder.text.contains("this link"));
    assert!(!builder.text.contains("["));
    assert!(!builder.text.contains("]"));

    // Should have detected the link facet
    assert!(builder.facet_candidates.iter().any(|fc| matches!(
        fc,
        FacetCandidate::MarkdownLink { url, .. } if url == "https://example.com"
    )));
}

#[test]
#[cfg(feature = "api_bluesky")]
fn test_builder_manual_construction() {
    let did = crate::types::did::Did::new_static("did:plc:z72i7hdynmk6r22z27h6tvur").unwrap();

    let result = RichText::builder()
        .text("Hello @alice check out example.com".to_string())
        .mention(&did, 6..12)
        .link("https://example.com", Some(23..34))
        .build()
        .unwrap();

    assert_eq!(result.text.as_str(), "Hello @alice check out example.com");
    assert!(result.facets.is_some());
    let facets = result.facets.unwrap();
    assert_eq!(facets.len(), 2);
}

#[test]
#[cfg(feature = "api_bluesky")]
fn test_overlapping_facets_error() {
    let did1 = crate::types::did::Did::new_static("did:plc:z72i7hdynmk6r22z27h6tvur").unwrap();
    let did2 = crate::types::did::Did::new_static("did:plc:ewvi7nxzyoun6zhxrhs64oiz").unwrap();

    let result = RichText::builder()
        .text("Hello world".to_string())
        .mention(&did1, 0..5)
        .mention(&did2, 3..8) // Overlaps with previous
        .build();

    assert!(matches!(
        result,
        Err(RichTextError::OverlappingFacets(_, _))
    ));
}

#[test]
fn test_parse_did_mentions() {
    let text = "Hey @did:plc:z72i7hdynmk6r22z27h6tvur check this out";
    let builder = RichText::parse(text);

    assert_eq!(builder.facet_candidates.len(), 1);
    match &builder.facet_candidates[0] {
        FacetCandidate::Mention { range, did } => {
            assert_eq!(&text[range.clone()], "@did:plc:z72i7hdynmk6r22z27h6tvur");
            assert!(did.is_some()); // DID should be pre-resolved
        }
        _ => panic!("Expected mention facet"),
    }
}

#[test]
fn test_bare_domain_link() {
    let text = "Visit example.com for info";
    let builder = RichText::parse(text);

    assert!(builder.facet_candidates.iter().any(|fc| {
        matches!(fc, FacetCandidate::Link { range } if text[range.clone()].contains("example.com"))
    }));
}

#[test]
fn test_trailing_punctuation_stripped() {
    let text = "Check https://example.com, and https://test.org.";
    let builder = RichText::parse(text);

    // Count link facets
    let link_count = builder
        .facet_candidates
        .iter()
        .filter(|fc| matches!(fc, FacetCandidate::Link { .. }))
        .count();

    assert_eq!(link_count, 2);

    // Verify punctuation is not included in ranges
    for fc in &builder.facet_candidates {
        if let FacetCandidate::Link { range } = fc {
            let url = &text[range.clone()];
            assert!(!url.ends_with(','));
            assert!(!url.ends_with('.'));
        }
    }
}

#[test]
#[cfg(feature = "api_bluesky")]
fn test_embed_detection_external() {
    let text = "Check out https://external.com/article";
    let builder = RichText::parse(text);

    assert!(builder.embed_candidates.is_some());
    let embeds = builder.embed_candidates.unwrap();
    assert_eq!(embeds.len(), 1);

    match &embeds[0] {
        EmbedCandidate::External { url, metadata } => {
            assert!(url.contains("external.com"));
            assert!(metadata.is_none());
        }
        _ => panic!("Expected external embed"),
    }
}

#[test]
#[cfg(feature = "api_bluesky")]
fn test_embed_detection_bsky_post() {
    let text = "See https://bsky.app/profile/alice.bsky.social/post/abc123";
    let builder = RichText::parse(text);

    assert!(builder.embed_candidates.is_some());
    let embeds = builder.embed_candidates.unwrap();
    assert_eq!(embeds.len(), 1);

    match &embeds[0] {
        EmbedCandidate::Record { at_uri, .. } => {
            assert_eq!(
                at_uri.as_str(),
                "at://alice.bsky.social/app.bsky.feed.post/abc123"
            );
        }
        _ => panic!("Expected record embed"),
    }
}

#[test]
#[cfg(feature = "api_bluesky")]
fn test_markdown_link_with_embed() {
    let text = "Read [my post](https://bsky.app/profile/me.bsky.social/post/xyz)";
    let builder = RichText::parse(text);

    // Should have markdown facet
    assert!(
        builder
            .facet_candidates
            .iter()
            .any(|fc| matches!(fc, FacetCandidate::MarkdownLink { .. }))
    );

    // Should also detect embed
    assert!(builder.embed_candidates.is_some());
    let embeds = builder.embed_candidates.unwrap();
    assert_eq!(embeds.len(), 1);
}

// === Sanitization Tests ===

#[test]
fn test_sanitize_soft_hyphen() {
    // Soft hyphens should be removed
    let text = "Hello\u{00AD}World";
    let builder = RichText::parse(text);

    assert_eq!(builder.text, "HelloWorld");
}

#[test]
fn test_sanitize_zero_width_space() {
    // Zero-width spaces should be removed
    let text = "Hello\u{200B}World";
    let builder = RichText::parse(text);

    assert_eq!(builder.text, "HelloWorld");
}

#[test]
fn test_sanitize_normalize_newlines() {
    // \r\n should normalize to \n
    let text = "Hello\r\nWorld";
    let builder = RichText::parse(text);

    assert_eq!(builder.text, "Hello\nWorld");
}

#[test]
fn test_sanitize_collapse_multiple_newlines() {
    // Multiple newlines should collapse to \n\n
    let text = "Hello\n\n\n\nWorld";
    let builder = RichText::parse(text);

    assert_eq!(builder.text, "Hello\n\nWorld");
}

#[test]
fn test_sanitize_mixed_invisible_and_newlines() {
    // Mix of invisible chars and newlines
    let text = "Hello\u{200B}\n\u{200C}\n\u{00AD}World";
    let builder = RichText::parse(text);

    assert_eq!(builder.text, "Hello\n\nWorld");
}

#[test]
fn test_sanitize_preserves_facets() {
    // Make sure sanitization doesn't break facet detection
    let text = "Hey @alice.bsky.social\u{200B} check\u{00AD}out https://example.com";
    let builder = RichText::parse(text);

    // Should still detect both mention and link
    assert!(builder
        .facet_candidates
        .iter()
        .any(|fc| matches!(fc, FacetCandidate::Mention { .. })));
    assert!(builder
        .facet_candidates
        .iter()
        .any(|fc| matches!(fc, FacetCandidate::Link { .. })));
}

#[test]
fn test_sanitize_newlines_with_spaces() {
    // Newlines with spaces between should collapse
    let text = "Hello\n \n \nWorld";
    let builder = RichText::parse(text);

    // 3 newlines with spaces -> collapses to \n\n
    assert_eq!(builder.text, "Hello\n\nWorld");
}

#[test]
fn test_sanitize_preserves_no_excess_newlines() {
    // Text without excessive newlines should be unchanged
    let text = "Hello\nWorld";
    let builder = RichText::parse(text);

    assert_eq!(builder.text, "Hello\nWorld");
}

#[test]
fn test_sanitize_empty_input() {
    // Empty string should remain empty
    let text = "";
    let builder = RichText::parse(text);

    assert_eq!(builder.text, "");
}

#[test]
fn test_sanitize_only_invisible_chars() {
    // Only invisible chars should be removed entirely
    let text = "\u{200B}\u{200C}\u{200D}\u{00AD}";
    let builder = RichText::parse(text);

    assert_eq!(builder.text, "");
}

#[test]
fn test_sanitize_cr_normalization() {
    // Standalone \r should normalize to \n
    let text = "Hello\rWorld";
    let builder = RichText::parse(text);

    assert_eq!(builder.text, "Hello\nWorld");
}

#[test]
fn test_sanitize_mixed_line_endings() {
    // Mix of \r\n, \r, \n should all normalize
    let text = "Line1\r\nLine2\rLine3\nLine4";
    let builder = RichText::parse(text);

    assert_eq!(builder.text, "Line1\nLine2\nLine3\nLine4");
}

#[test]
fn test_sanitize_preserves_regular_spaces() {
    // Regular spaces without newlines should be preserved
    let text = "Hello    World";
    let builder = RichText::parse(text);

    assert_eq!(builder.text, "Hello    World");
}

// === Adversarial / Edge Case Tests ===

#[test]
fn test_tag_too_long() {
    // Tags must be 64 chars or less
    let long_tag = "a".repeat(65);
    let text = format!("#{}", long_tag);
    let builder = RichText::parse(text);

    // Should NOT detect the tag
    assert!(builder
        .facet_candidates
        .iter()
        .all(|fc| !matches!(fc, FacetCandidate::Tag { .. })));
}

#[test]
fn test_tag_with_zero_width_chars() {
    // Zero-width joiners and other invisible unicode
    let text = "This is #cool\u{200B}tag";
    let builder = RichText::parse(text);

    // Tag should stop at zero-width char
    let tags: Vec<_> = builder
        .facet_candidates
        .iter()
        .filter_map(|fc| match fc {
            FacetCandidate::Tag { range } => Some(&builder.text[range.clone()]),
            _ => None,
        })
        .collect();

    // Should only capture up to the zero-width char
    assert!(tags.iter().any(|t| t.starts_with("#cool")));
}

#[test]
fn test_url_with_parens() {
    // URLs like wikipedia with (parens) in them
    let text = "See https://en.wikipedia.org/wiki/Rust_(programming_language)";
    let builder = RichText::parse(text);

    // Should capture the full URL including parens
    assert!(builder.facet_candidates.iter().any(|fc| {
        matches!(fc, FacetCandidate::Link { range } if text[range.clone()].contains("programming_language"))
    }));
}

#[test]
fn test_markdown_link_unclosed() {
    // Malformed markdown should not be processed
    let text = "This is [unclosed link";
    let builder = RichText::parse(text);

    // Should not detect markdown link, text unchanged
    assert_eq!(builder.text, text);
    assert!(builder
        .facet_candidates
        .iter()
        .all(|fc| !matches!(fc, FacetCandidate::MarkdownLink { .. })));
}

#[test]
fn test_nested_markdown_attempts() {
    // Try to nest markdown links
    let text = "[[nested](https://inner.com)](https://outer.com)";
    let builder = RichText::parse(text);

    // Should only match the innermost valid pattern
    let markdown_count = builder
        .facet_candidates
        .iter()
        .filter(|fc| matches!(fc, FacetCandidate::MarkdownLink { .. }))
        .count();

    // Regex should match leftmost, should get one
    assert!(markdown_count > 0);
}

#[test]
fn test_mention_with_emoji() {
    // Handles can't have emoji but let's make sure it doesn't crash
    let text = "Hey @alice😎.bsky.social wassup";
    let builder = RichText::parse(text);

    // Should not match or should stop at emoji
    let mentions: Vec<_> = builder
        .facet_candidates
        .iter()
        .filter_map(|fc| match fc {
            FacetCandidate::Mention { range, .. } => Some(&text[range.clone()]),
            _ => None,
        })
        .collect();

    // Either no mentions or mention stops before emoji
    for mention in mentions {
        assert!(!mention.contains('😎'));
    }
}

#[test]
fn test_handle_with_trailing_dots() {
    // Handles like @alice... should not include trailing dots
    let text = "Hey @alice.bsky.social... how are you";
    let builder = RichText::parse(text);

    if let Some(FacetCandidate::Mention { range, .. }) = builder.facet_candidates.first() {
        let mention = &text[range.clone()];
        assert!(!mention.ends_with('.'));
    }
}

#[test]
fn test_url_javascript_protocol() {
    // Should not detect javascript: or data: URLs
    let text = "Click javascript:alert(1) or data:text/html,<script>alert(1)</script>";
    let builder = RichText::parse(text);

    // Should not match non-http(s) URLs
    for fc in &builder.facet_candidates {
        if let FacetCandidate::Link { range } = fc {
            let url = &text[range.clone()];
            assert!(!url.starts_with("javascript:"));
            assert!(!url.starts_with("data:"));
        }
    }
}

#[test]
fn test_extremely_long_url() {
    // Very long URL should still work (no panic)
    let long_path = "a/".repeat(1000);
    let text = format!("Visit https://example.com/{}", long_path);
    let builder = RichText::parse(text);

    // Should detect the URL without panicking
    assert!(builder
        .facet_candidates
        .iter()
        .any(|fc| matches!(fc, FacetCandidate::Link { .. })));
}

#[test]
fn test_empty_string() {
    let text = "";
    let builder = RichText::parse(text);

    assert_eq!(builder.text, "");
    assert!(builder.facet_candidates.is_empty());
}

#[test]
fn test_only_whitespace() {
    let text = "   \t\n  ";
    let builder = RichText::parse(text);

    assert!(builder.facet_candidates.is_empty());
}

#[test]
fn test_markdown_with_newlines() {
    // Markdown regex should not match across newlines
    let text = "This is [text\nwith](https://example.com) newline";
    let builder = RichText::parse(text);

    // Current regex won't match \n in the display text part
    // Just make sure it doesn't panic
    let _ = builder.facet_candidates;
}

#[test]
fn test_multiple_at_signs() {
    // @@alice should only match @alice
    let text = "Hey @@alice.bsky.social";
    let builder = RichText::parse(text);

    // Regex requires word boundary before @, so @@ might not match
    // Or might match the second @
    // Just verify it doesn't panic and produces valid ranges
    for fc in &builder.facet_candidates {
        if let FacetCandidate::Mention { range, .. } = fc {
            assert!(range.end <= text.len());
            let _ = &text[range.clone()]; // Shouldn't panic
        }
    }
}

#[test]
fn test_url_with_unicode_domain() {
    // IDN domains
    let text = "Visit https://例え.jp for info";
    let builder = RichText::parse(text);

    // Current regex only matches ASCII domains, so this might not detect
    // Just make sure no panic
    let _ = builder.facet_candidates;
}

#[test]
#[cfg(feature = "api_bluesky")]
fn test_build_with_invalid_range() {
    let did = crate::types::did::Did::new_static("did:plc:z72i7hdynmk6r22z27h6tvur").unwrap();

    // Range exceeds text length
    let result = RichText::builder()
        .text("Short".to_string())
        .mention(&did, 0..100)
        .build();

    assert!(matches!(
        result,
        Err(RichTextError::InvalidRange { .. })
    ));
}

#[test]
fn test_rtl_override_injection() {
    // Right-to-left override character attempts
    let text = "Hey @alice\u{202E}reversed\u{202C}.bsky.social";
    let builder = RichText::parse(text);

    // Should either not match or handle gracefully
    let _ = builder.facet_candidates;
}

#[test]
fn test_tag_empty_after_hash() {
    // Just # with nothing after
    let text = "This is # a test";
    let builder = RichText::parse(text);

    // Should not detect empty tag
    assert!(builder
        .facet_candidates
        .iter()
        .all(|fc| !matches!(fc, FacetCandidate::Tag { .. })));
}

// === Unicode Byte Boundary Tests ===

#[test]
fn test_facet_ranges_valid_utf8_boundaries() {
    // All detected facet ranges must be valid UTF-8 boundaries
    let text = "Hey @alice.bsky.social check 你好 #tag🔥 and https://example.com/测试";
    let builder = RichText::parse(text);

    for fc in &builder.facet_candidates {
        let range = match fc {
            FacetCandidate::Mention { range, .. } => range,
            FacetCandidate::Link { range } => range,
            FacetCandidate::Tag { range } => range,
            FacetCandidate::MarkdownLink { display_range, .. } => display_range,
        };

        // This will panic if range is not on UTF-8 char boundary
        // Use builder.text (sanitized) not original text
        let slice = &builder.text[range.clone()];
        // Verify it's valid UTF-8
        assert!(std::str::from_utf8(slice.as_bytes()).is_ok());
    }
}

#[test]
fn test_emoji_grapheme_clusters() {
    // Family emoji with ZWJ sequences: "👨‍👩‍👧‍👧" is 25 bytes but 1 grapheme
    let text = "Hello 👨‍👩‍👧‍👧 @alice.bsky.social";
    let builder = RichText::parse(text);

    // Should still detect the mention after the emoji
    assert!(builder
        .facet_candidates
        .iter()
        .any(|fc| matches!(fc, FacetCandidate::Mention { .. })));

    // Verify all ranges are valid against the sanitized text
    for fc in &builder.facet_candidates {
        if let FacetCandidate::Mention { range, .. } = fc {
            let _ = &builder.text[range.clone()]; // Shouldn't panic
        }
    }
}

#[test]
fn test_tag_with_emoji() {
    // Tags can contain emoji
    let text = "This is #cool🔥";
    let builder = RichText::parse(text);

    let tags: Vec<_> = builder
        .facet_candidates
        .iter()
        .filter_map(|fc| match fc {
            FacetCandidate::Tag { range } => Some(&builder.text[range.clone()]),
            _ => None,
        })
        .collect();

    // Should include emoji in tag
    assert!(tags.iter().any(|t| t.contains("🔥")));
}

#[test]
fn test_sanitize_newlines_with_emoji() {
    // Newlines with emoji should still collapse correctly
    let text = "Hello 🎉\n\n\n\nWorld 🌍";
    let builder = RichText::parse(text);

    assert_eq!(builder.text, "Hello 🎉\n\nWorld 🌍");
}
