use jacquard_lexicon::lexicon::LexStringFormat;
use jacquard_lexicon::schema::builder::LexiconDocBuilder;

#[test]
fn test_builder_simple_record() {
    let doc = LexiconDocBuilder::new("app.example.test")
        .description("Test record")
        .record()
        .key("tid")
        .field("text", |f| f.string().max_length(1000).required().build())
        .field("createdAt", |f| {
            f.string()
                .format(LexStringFormat::Datetime)
                .required()
                .build()
        })
        .build()
        .build();

    assert_eq!(doc.id.as_ref(), "app.example.test");
    assert_eq!(doc.defs.len(), 1);

    // Serialize and verify
    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    assert!(json.contains("\"type\": \"record\""));
    assert!(json.contains("\"maxLength\": 1000"));
}

#[test]
fn test_builder_query() {
    let doc = LexiconDocBuilder::new("app.example.getPost")
        .description("Get a post")
        .query()
        .description("Retrieve a post by URI")
        .param_string("uri", true)
        .build()
        .build();

    assert_eq!(doc.id.as_ref(), "app.example.getPost");
    assert_eq!(doc.defs.len(), 1);

    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    assert!(json.contains("\"type\": \"query\""));
}

#[test]
fn test_builder_object_with_ref() {
    let doc = LexiconDocBuilder::new("app.example.types")
        .object("post")
        .field("uri", |f| {
            f.string().format(LexStringFormat::AtUri).required().build()
        })
        .field("author", |f| f.ref_to("app.bsky.actor.defs#profileView"))
        .build()
        .build();

    assert_eq!(doc.id.as_ref(), "app.example.types");
    assert_eq!(doc.defs.len(), 1);

    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    assert!(json.contains("\"type\": \"ref\""));
    assert!(json.contains("app.bsky.actor.defs#profileView"));
}

#[test]
fn test_builder_array_field() {
    let doc = LexiconDocBuilder::new("app.example.list")
        .record()
        .field("items", |f| f.array(|a| a.string_items().max_length(100)))
        .build()
        .build();

    assert_eq!(doc.id.as_ref(), "app.example.list");

    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    assert!(json.contains("\"type\": \"array\""));
}
