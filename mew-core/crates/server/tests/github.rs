//! GithubClient JWT signing — the token must be a verifiable RS256 JWT
//! carrying the app ID as `iss` and a short expiry. Uses a committed
//! test-only key pair (fixtures/) so the test never touches real
//! credentials.

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use mewcode_server::github::GithubClient;

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

#[test]
fn jwt_is_verifiable_rs256_with_app_claims() {
    let client = GithubClient::new(
        424242,
        &format!("{FIXTURE_DIR}/test-github-app-key.pem"),
    )
    .expect("fixture key loads");
    let jwt = client.jwt().expect("jwt signs");

    let public_pem = std::fs::read_to_string(format!("{FIXTURE_DIR}/test-github-app-key.pub.pem"))
        .expect("public fixture key reads");
    let mut validation = Validation::new(Algorithm::RS256);
    validation.validate_exp = true;
    validation.required_spec_claims = ["exp", "iat"].into_iter().map(String::from).collect();
    let token = decode::<serde_json::Value>(
        &jwt,
        &DecodingKey::from_rsa_pem(public_pem.as_bytes()).expect("public key parses"),
        &validation,
    )
    .expect("jwt verifies against the public key");

    assert_eq!(token.claims["iss"], 424242);
    let exp = token.claims["exp"].as_i64().expect("exp is a number");
    let iat = token.claims["iat"].as_i64().expect("iat is a number");
    assert_eq!(exp - iat, 660);
}
