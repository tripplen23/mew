use mewcode_protocol::ProviderId;
use mewcode_server::provider_catalog::parse_models;

#[test]
fn catalog_parsing_is_tolerant_and_preserves_exact_ids() {
    let models = parse_models(
        r#"{"data":[
            {"id":"Vendor/Model:free","name":"Vendor\n Model\tName","context_length":131072,"future":{"x":1}},
            {"id":"openrouter::nested/id","name":"\n\t"},
            {"id":"line\nbreak","name":null},
            {"id":"","name":"ignored"}
        ],"future":"ignored"}"#,
        ProviderId::OpenRouter,
    )
    .expect("catalog should parse");

    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "Vendor/Model:free");
    assert_eq!(models[0].display_name, "Vendor Model Name");
    assert_eq!(models[0].context_length, Some(131_072));
    assert!(models[0].is_free);
    assert_eq!(models[1].id, "openrouter::nested/id");
    assert_eq!(models[1].display_name, "openrouter::nested/id");
    assert!(!models[1].is_free);
}

#[test]
fn malformed_or_incomplete_catalog_is_rejected() {
    for body in ["not json", r#"{}"#, r#"{"data":{}}"#] {
        assert!(
            parse_models(body, ProviderId::OpenRouter).is_err(),
            "accepted {body}"
        );
    }
}

#[test]
fn oversized_catalog_is_rejected_before_parsing() {
    let body = " ".repeat(mewcode_server::provider_catalog::MAX_CATALOG_BYTES + 1);
    assert!(
        parse_models(&body, ProviderId::OpenRouter)
            .unwrap_err()
            .contains("too large")
    );
}
