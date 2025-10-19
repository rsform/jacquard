use std::collections::BTreeMap;

use crate::moderation::{
    Blur, LabelPref, LabelTarget, Labeled, LabelerDefs, Moderateable, ModerationPrefs, moderate,
    moderate_all,
};
use jacquard_api::app_bsky::feed::FeedViewPost;
use jacquard_api::app_bsky::labeler::get_services::GetServicesOutput;
use jacquard_api::com_atproto::label::{Label, LabelValueDefinition};
use jacquard_common::CowStr;
use jacquard_common::types::string::{Datetime, Did, Uri};
use serde::Deserialize;

const LABELER_SERVICES_JSON: &str = include_str!("labeler_services.json");
const POSTS_JSON: &str = include_str!("posts.json");

#[test]
fn test_parse_labeler_services() {
    let services: GetServicesOutput =
        serde_json::from_str(LABELER_SERVICES_JSON).expect("failed to parse labeler services");

    assert!(!services.views.is_empty(), "should have labeler views");
}

#[test]
fn test_build_labeler_defs_from_services() {
    let services: GetServicesOutput<'static> =
        serde_json::from_str(LABELER_SERVICES_JSON).expect("failed to parse");

    let mut defs = LabelerDefs::new();

    use jacquard_api::app_bsky::labeler::get_services::GetServicesOutputViewsItem;

    for view in services.views {
        if let GetServicesOutputViewsItem::LabelerViewDetailed(detailed) = view {
            if let Some(label_defs) = &detailed.policies.label_value_definitions {
                defs.insert(detailed.creator.did.clone(), label_defs.clone());
            }
        }
    }

    // Should have definitions from multiple labelers
    assert!(!defs.defs.is_empty(), "should have labeler definitions");
}

#[test]
fn test_moderate_with_default_hide() {
    // Test that a label with defaultSetting: "hide" actually filters content
    let mut defs = LabelerDefs::new();
    let labeler_did = Did::new_static("did:plc:ar7c4by46qjdydhdevvrndac").unwrap();

    // Create a label definition with defaultSetting: "hide"
    let spam_def = LabelValueDefinition {
        identifier: CowStr::from("spam"),
        blurs: CowStr::from("content"),
        severity: CowStr::from("inform"),
        default_setting: Some(CowStr::from("hide")),
        adult_only: Some(false),
        locales: vec![],
        extra_data: BTreeMap::new(),
    };

    defs.insert(labeler_did.clone(), vec![spam_def]);

    // Create a mock labeled item
    struct MockLabeled {
        labels: Vec<Label<'static>>,
    }

    impl<'a> Labeled<'a> for MockLabeled {
        fn labels(&self) -> &[Label<'a>] {
            &self.labels
        }
    }

    let item = MockLabeled {
        labels: vec![Label {
            src: labeler_did.clone(),
            uri: Uri::new_owned("at://did:plc:test/app.bsky.feed.post/abc123").unwrap(),
            cid: None,
            val: CowStr::from("spam"),
            neg: None,
            cts: Datetime::now(),
            exp: None,
            sig: None,
            ver: None,
            extra_data: Default::default(),
        }],
    };

    let prefs = ModerationPrefs::default();
    let decision = moderate(&item, &prefs, &defs, &[labeler_did]);

    assert!(decision.filter, "spam label should filter by default");
    assert_eq!(decision.causes.len(), 1);
    assert_eq!(decision.causes[0].label.as_str(), "spam");
}

#[test]
fn test_moderate_with_user_preference() {
    // Test that user preferences override default settings
    let mut defs = LabelerDefs::new();
    let labeler_did = Did::new_static("did:plc:test").unwrap();

    let def = LabelValueDefinition {
        identifier: CowStr::from("test-label"),
        blurs: CowStr::from("content"),
        severity: CowStr::from("alert"),
        default_setting: Some(CowStr::from("hide")),
        adult_only: Some(false),
        locales: vec![],
        extra_data: BTreeMap::new(),
    };

    defs.insert(labeler_did.clone(), vec![def]);

    struct MockLabeled {
        labels: Vec<Label<'static>>,
    }

    impl<'a> Labeled<'a> for MockLabeled {
        fn labels(&self) -> &[Label<'a>] {
            &self.labels
        }
    }

    let item = MockLabeled {
        labels: vec![Label {
            src: labeler_did.clone(),
            uri: Uri::new_owned("at://did:plc:test/app.bsky.feed.post/abc").unwrap(),
            val: CowStr::from("test-label"),
            neg: None,
            cts: Datetime::now(),
            exp: None,
            sig: None,
            cid: None,
            ver: None,
            extra_data: Default::default(),
        }],
    };

    // User explicitly ignores this label
    let mut prefs = ModerationPrefs::default();
    prefs
        .labels
        .insert(CowStr::from("test-label"), LabelPref::Ignore);

    let decision = moderate(&item, &prefs, &defs, &[labeler_did]);

    assert!(
        !decision.filter,
        "user preference should override default hide"
    );
    assert!(decision.causes.is_empty());
}

#[test]
fn test_label_target_detection() {
    let labeler_did = Did::new_static("did:plc:test").unwrap();

    struct MockLabeled {
        labels: Vec<Label<'static>>,
    }

    impl<'a> Labeled<'a> for MockLabeled {
        fn labels(&self) -> &[Label<'a>] {
            &self.labels
        }
    }

    // Account-level label (just DID)
    let account_item = MockLabeled {
        labels: vec![Label {
            src: labeler_did.clone(),
            uri: Uri::new_owned("did:plc:someuser").unwrap(),
            val: CowStr::from("test"),
            neg: None,
            cts: Datetime::now(),
            exp: None,
            sig: None,
            cid: None,
            ver: None,
            extra_data: Default::default(),
        }],
    };

    let defs = LabelerDefs::new();
    let prefs = ModerationPrefs::default();
    let decision = moderate(&account_item, &prefs, &defs, &[labeler_did.clone()]);

    if let Some(cause) = decision.causes.first() {
        assert_eq!(cause.target, LabelTarget::Account);
    }

    // Content-level label (at:// URI with collection/rkey)
    let content_item = MockLabeled {
        labels: vec![Label {
            src: labeler_did.clone(),
            uri: Uri::new_owned("at://did:plc:someuser/app.bsky.feed.post/abc123").unwrap(),
            val: CowStr::from("test"),
            neg: None,
            cts: Datetime::now(),
            exp: None,
            sig: None,
            cid: None,
            ver: None,
            extra_data: Default::default(),
        }],
    };

    let decision = moderate(&content_item, &prefs, &defs, &[labeler_did]);

    if let Some(cause) = decision.causes.first() {
        assert_eq!(cause.target, LabelTarget::Content);
    }
}

#[test]
fn test_blur_media_vs_content() {
    let mut defs = LabelerDefs::new();
    let labeler_did = Did::new_static("did:plc:test").unwrap();

    // Media blur
    let media_def = LabelValueDefinition {
        identifier: CowStr::from("media-label"),
        blurs: CowStr::from("media"),
        severity: CowStr::from("alert"),
        default_setting: Some(CowStr::from("warn")),
        adult_only: Some(false),
        locales: vec![],
        extra_data: BTreeMap::new(),
    };

    // Content blur
    let content_def = LabelValueDefinition {
        identifier: CowStr::from("content-label"),
        blurs: CowStr::from("content"),
        severity: CowStr::from("alert"),
        default_setting: Some(CowStr::from("warn")),
        adult_only: Some(false),
        locales: vec![],
        extra_data: BTreeMap::new(),
    };

    defs.insert(labeler_did.clone(), vec![media_def, content_def]);

    struct MockLabeled {
        labels: Vec<Label<'static>>,
    }

    impl<'a> Labeled<'a> for MockLabeled {
        fn labels(&self) -> &[Label<'a>] {
            &self.labels
        }
    }

    // Test media blur
    let media_item = MockLabeled {
        labels: vec![Label {
            src: labeler_did.clone(),
            uri: Uri::new_owned("at://did:plc:test/app.bsky.feed.post/abc").unwrap(),
            val: CowStr::from("media-label"),
            neg: None,
            cts: Datetime::now(),
            exp: None,
            sig: None,
            cid: None,
            ver: None,
            extra_data: Default::default(),
        }],
    };

    let prefs = ModerationPrefs::default();
    let decision = moderate(&media_item, &prefs, &defs, &[labeler_did.clone()]);

    assert_eq!(decision.blur, Blur::Media);

    // Test content blur
    let content_item = MockLabeled {
        labels: vec![Label {
            src: labeler_did.clone(),
            uri: Uri::new_owned("at://did:plc:test/app.bsky.feed.post/xyz").unwrap(),
            val: CowStr::from("content-label"),
            neg: None,
            cts: Datetime::now(),
            exp: None,
            sig: None,
            cid: None,
            ver: None,
            extra_data: Default::default(),
        }],
    };

    let decision = moderate(&content_item, &prefs, &defs, &[labeler_did]);

    assert_eq!(decision.blur, Blur::Content);
}

#[test]
fn test_adult_only_labels_require_adult_content_enabled() {
    let mut defs = LabelerDefs::new();
    let labeler_did = Did::new_static("did:plc:test").unwrap();

    let adult_def = LabelValueDefinition {
        identifier: CowStr::from("adult-label"),
        blurs: CowStr::from("content"),
        severity: CowStr::from("alert"),
        default_setting: Some(CowStr::from("warn")),
        adult_only: Some(true),
        locales: vec![],
        extra_data: BTreeMap::new(),
    };

    defs.insert(labeler_did.clone(), vec![adult_def]);

    struct MockLabeled {
        labels: Vec<Label<'static>>,
    }

    impl<'a> Labeled<'a> for MockLabeled {
        fn labels(&self) -> &[Label<'a>] {
            &self.labels
        }
    }

    let item = MockLabeled {
        labels: vec![Label {
            src: labeler_did.clone(),
            uri: Uri::new_owned("at://did:plc:test/app.bsky.feed.post/abc").unwrap(),
            val: CowStr::from("adult-label"),
            neg: None,
            cts: Datetime::now(),
            exp: None,
            sig: None,
            cid: None,
            ver: None,
            extra_data: Default::default(),
        }],
    };

    // With adult content disabled (default)
    let prefs = ModerationPrefs::default();
    let decision = moderate(&item, &prefs, &defs, &[labeler_did.clone()]);

    assert!(
        decision.filter,
        "adult-only label should filter when adult content disabled"
    );
    assert!(decision.no_override, "should not allow override");

    // With adult content enabled
    let mut prefs_enabled = ModerationPrefs::default();
    prefs_enabled.adult_content_enabled = true;

    let decision = moderate(&item, &prefs_enabled, &defs, &[labeler_did]);

    // Should still warn but not filter completely
    assert!(!decision.filter || decision.blur != Blur::None);
}

#[test]
fn test_negation_labels() {
    let labeler_did = Did::new_static("did:plc:test").unwrap();

    struct MockLabeled {
        labels: Vec<Label<'static>>,
    }

    impl<'a> Labeled<'a> for MockLabeled {
        fn labels(&self) -> &[Label<'a>] {
            &self.labels
        }
    }

    // Item with a label and its negation
    let item = MockLabeled {
        labels: vec![
            Label {
                src: labeler_did.clone(),
                uri: Uri::new_owned("at://did:plc:test/app.bsky.feed.post/abc").unwrap(),
                val: CowStr::from("test-label"),
                neg: None,
                cts: Datetime::now(),
                exp: None,
                sig: None,
                cid: None,
                ver: None,
                extra_data: Default::default(),
            },
            Label {
                src: labeler_did.clone(),
                uri: Uri::new_owned("at://did:plc:test/app.bsky.feed.post/abc").unwrap(),
                val: CowStr::from("test-label"),
                neg: Some(true), // negation
                cts: Datetime::now(),
                exp: None,
                sig: None,
                cid: None,
                ver: None,
                extra_data: Default::default(),
            },
        ],
    };

    let defs = LabelerDefs::new();
    let prefs = ModerationPrefs::default();
    let decision = moderate(&item, &prefs, &defs, &[labeler_did]);

    // Negation should cancel out the original label
    assert!(
        decision
            .causes
            .iter()
            .all(|c| c.label.as_str() != "test-label"),
        "negation should remove the label from causes"
    );
}

#[test]
fn test_moderate_all() {
    let labeler_did = Did::new_static("did:plc:test").unwrap();

    struct MockLabeled {
        labels: Vec<Label<'static>>,
    }

    impl<'a> Labeled<'a> for MockLabeled {
        fn labels(&self) -> &[Label<'a>] {
            &self.labels
        }
    }

    let items = vec![
        MockLabeled { labels: vec![] },
        MockLabeled {
            labels: vec![Label {
                src: labeler_did.clone(),
                uri: Uri::new_owned("at://did:plc:test/app.bsky.feed.post/abc").unwrap(),
                val: CowStr::from("porn"),
                neg: None,
                cts: Datetime::now(),
                exp: None,
                sig: None,
                cid: None,
                ver: None,
                extra_data: Default::default(),
            }],
        },
        MockLabeled { labels: vec![] },
    ];

    let prefs = ModerationPrefs::default();
    let defs = LabelerDefs::new();
    let results = moderate_all(&items, &prefs, &defs, &[labeler_did]);

    assert_eq!(results.len(), 3);
    assert!(!results[0].1.filter, "first item should not be filtered");
    assert!(
        results[1].1.filter,
        "second item with porn should be filtered"
    );
    assert!(!results[2].1.filter, "third item should not be filtered");
}

#[test]
fn test_end_to_end_feed_moderation() {
    // Parse labeler services and build definitions
    let services: GetServicesOutput<'static> =
        serde_json::from_str(LABELER_SERVICES_JSON).expect("failed to parse labeler services");

    let mut defs = LabelerDefs::new();
    let mut accepted_labelers = Vec::new();
    use jacquard_api::app_bsky::labeler::get_services::GetServicesOutputViewsItem;

    for view in services.views {
        if let GetServicesOutputViewsItem::LabelerViewDetailed(detailed) = view {
            accepted_labelers.push(detailed.creator.did.clone());
            if let Some(label_value_definitions) = &detailed.policies.label_value_definitions {
                defs.insert(
                    detailed.creator.did.clone(),
                    label_value_definitions.clone(),
                );
            }
        }
    }

    // Parse posts
    #[derive(Deserialize)]
    struct FeedResponse<'a> {
        #[serde(borrow)]
        feed: Vec<FeedViewPost<'a>>,
    }

    let feed_responses: Vec<FeedResponse<'static>> =
        serde_json::from_str(POSTS_JSON).expect("failed to parse posts");

    // Combine all feeds to test
    let all_posts: Vec<_> = feed_responses
        .iter()
        .flat_map(|response| &response.feed)
        .collect();

    let prefs = ModerationPrefs::default();

    // Apply moderation to all posts in the feed (post, author, and reply chain)
    let moderated: Vec<_> = all_posts
        .iter()
        .map(|feed_post| {
            use jacquard_api::app_bsky::feed::{ReplyRefParent, ReplyRefRoot};

            let mut all_decisions = vec![];

            // Moderate main post and author
            all_decisions.push(moderate(&feed_post.post, &prefs, &defs, &accepted_labelers));
            all_decisions.push(moderate(
                &feed_post.post.author,
                &prefs,
                &defs,
                &accepted_labelers,
            ));

            // Check reply parent/root if present
            if let Some(reply) = &feed_post.reply {
                if let ReplyRefParent::PostView(parent) = &reply.parent {
                    all_decisions.push(moderate(&**parent, &prefs, &defs, &accepted_labelers));
                    all_decisions.push(moderate(&parent.author, &prefs, &defs, &accepted_labelers));
                }
                if let ReplyRefRoot::PostView(root) = &reply.root {
                    all_decisions.push(moderate(&**root, &prefs, &defs, &accepted_labelers));
                    all_decisions.push(moderate(&root.author, &prefs, &defs, &accepted_labelers));
                }
                if let Some(grandparent_author) = &reply.grandparent_author {
                    all_decisions.push(moderate(
                        grandparent_author,
                        &prefs,
                        &defs,
                        &accepted_labelers,
                    ));
                }
            }

            (feed_post, all_decisions)
        })
        .collect();

    // Debug: check what labels exist
    let total_posts = all_posts.len();

    println!("Total feeds in response: {}", feed_responses.len());

    // Show which posts have labels
    for (i, feed_post) in all_posts.iter().enumerate() {
        if let Some(labels) = &feed_post.post.labels {
            if !labels.is_empty() {
                println!(
                    "Post {} has {} labels: {:?}",
                    i,
                    labels.len(),
                    labels.iter().map(|l| l.val.as_ref()).collect::<Vec<_>>()
                );
            }
        }
    }

    let posts_with_any_labels = all_posts
        .iter()
        .filter(|post| !post.post.labels().is_empty())
        .count();
    let authors_with_any_labels = all_posts
        .iter()
        .filter(|post| !post.post.author.labels().is_empty())
        .count();

    // Count how many posts have moderation decisions with causes
    let posts_with_causes = moderated
        .iter()
        .filter(|(_, decisions)| decisions.iter().any(|d| !d.causes.is_empty()))
        .count();

    // Summary output
    println!("Total posts: {}", total_posts);
    println!("Posts with labels: {}", posts_with_any_labels);
    println!("Authors with labels: {}", authors_with_any_labels);
    println!("Feed posts with moderation causes: {}", posts_with_causes);
    println!("Accepted labelers: {}", accepted_labelers.len());
    println!("Labeler definitions: {}", defs.defs.len());

    // Print all unique labels found and their default settings
    let mut all_labels_found = std::collections::HashSet::new();
    for feed_post in &all_posts {
        for label in feed_post.post.labels() {
            all_labels_found.insert((label.val.as_ref(), label.src.as_ref()));
        }
        for label in feed_post.post.author.labels() {
            all_labels_found.insert((label.val.as_ref(), label.src.as_ref()));
        }
        if let Some(reply) = &feed_post.reply {
            use jacquard_api::app_bsky::feed::{ReplyRefParent, ReplyRefRoot};
            if let ReplyRefParent::PostView(parent) = &reply.parent {
                for label in parent.labels() {
                    all_labels_found.insert((label.val.as_ref(), label.src.as_ref()));
                }
                for label in parent.author.labels() {
                    all_labels_found.insert((label.val.as_ref(), label.src.as_ref()));
                }
            }
            if let ReplyRefRoot::PostView(root) = &reply.root {
                for label in root.labels() {
                    all_labels_found.insert((label.val.as_ref(), label.src.as_ref()));
                }
                for label in root.author.labels() {
                    all_labels_found.insert((label.val.as_ref(), label.src.as_ref()));
                }
            }
            if let Some(grandparent) = &reply.grandparent_author {
                for label in grandparent.labels() {
                    all_labels_found.insert((label.val.as_ref(), label.src.as_ref()));
                }
            }
        }
    }

    println!("Unique labels found: {}", all_labels_found.len());

    // Count total moderation causes found
    let total_causes: usize = moderated
        .iter()
        .map(|(_, decisions)| decisions.iter().map(|d| d.causes.len()).sum::<usize>())
        .sum();

    println!("Total moderation causes: {}", total_causes);

    // Verify specific facts about the test data
    assert_eq!(
        posts_with_any_labels, 13,
        "should have 13 posts with labels"
    );
    assert!(
        all_labels_found.iter().any(|(val, _)| val == &"porn"),
        "should have porn labels"
    );
    assert!(
        all_labels_found.iter().any(|(val, _)| val == &"sexual"),
        "should have sexual labels"
    );
    assert!(
        all_labels_found.iter().any(|(val, _)| val == &"nudity"),
        "should have nudity labels"
    );

    // Verify end-to-end moderation worked
    assert!(
        posts_with_causes > 0,
        "should have posts with moderation causes"
    );
    assert!(total_causes > 0, "should have found moderation causes");
}

#[test]
fn test_moderatable_trait() {
    // Test the Moderatable trait on FeedViewPost
    let services: GetServicesOutput<'static> =
        serde_json::from_str(LABELER_SERVICES_JSON).expect("failed to parse labeler services");

    let mut defs = LabelerDefs::new();
    use jacquard_api::app_bsky::labeler::get_services::GetServicesOutputViewsItem;

    for view in services.views {
        if let GetServicesOutputViewsItem::LabelerViewDetailed(detailed) = view {
            if let Some(label_value_definitions) = &detailed.policies.label_value_definitions {
                defs.insert(
                    detailed.creator.did.clone(),
                    label_value_definitions.clone(),
                );
            }
        }
    }

    #[derive(Deserialize)]
    struct FeedResponse<'a> {
        #[serde(borrow)]
        feed: Vec<FeedViewPost<'a>>,
    }

    let feed_responses: Vec<FeedResponse<'static>> =
        serde_json::from_str(POSTS_JSON).expect("failed to parse posts");

    let prefs = ModerationPrefs::default();

    // Find a post with porn/sexual/nudity labels (we know these exist from earlier test)
    let labeled_post = feed_responses
        .iter()
        .flat_map(|r| &r.feed)
        .find(|p| {
            p.post.labels().iter().any(|l| {
                l.val.as_ref() == "porn" || l.val.as_ref() == "sexual" || l.val.as_ref() == "nudity"
            })
        })
        .expect("should find at least one porn/sexual/nudity labeled post");

    let post_labels = labeled_post.post.labels();
    println!("Testing post with {} labels:", post_labels.len());
    for label in post_labels {
        println!("  {} from {}", label.val.as_ref(), label.src.as_ref());
    }

    // Use the Moderateable trait with empty accepted_labelers to trust all labels
    let decisions = labeled_post.moderate_all(&prefs, &defs, &[]);

    println!("Moderateable decisions for labeled post:");
    for (tag, decision) in &decisions {
        if !decision.causes.is_empty() {
            println!(
                "  {}: filter={}, blur={:?}, causes={}",
                tag,
                decision.filter,
                decision.blur,
                decision.causes.len()
            );
        }
    }

    // Should have decisions for at least post and author
    assert!(
        decisions.iter().any(|(tag, _)| *tag == "post"),
        "should have post decision"
    );
    assert!(
        decisions.iter().any(|(tag, _)| *tag == "author"),
        "should have author decision"
    );

    // At least one decision should have causes (from the labeled post)
    assert!(
        decisions.iter().any(|(_, d)| !d.causes.is_empty()),
        "should have at least one decision with causes"
    );
}
