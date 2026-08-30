use mewcode_server::openapi::ApiDoc;
use utoipa::OpenApi;

#[test]
fn model_ref_schema_matches_string_wire_format() {
    let document = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let schema = &document["components"]["schemas"]["ModelRef"];

    assert_eq!(schema["type"], "string");
    assert_eq!(
        serde_json::to_value(mewcode_protocol::ModelRef::openrouter("vendor/model").unwrap())
            .unwrap(),
        "openrouter::vendor/model"
    );
}
