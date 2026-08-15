use super::*;

#[test]
fn digest_is_stable_and_hex_encoded() {
    assert_eq!(
        digest("bundle"),
        "1e6ed65d77d6364eeaed5a745ba5c4985ae2b700dd85d7cf7f027bdf294a33fc"
    );
}

#[test]
fn response_identity_and_size_are_validated() {
    let request = BundleRequest {
        protocol_version: BUNDLER_PROTOCOL_VERSION,
        transformer: BundleTransformer::Esbuild,
        profile: BundleProfile::Web,
        entry: "main.js".into(),
        language: "javascript".into(),
        minified: false,
        target: BundleTarget::Es5,
        files: vec![AdapterFile {
            path: "main.js".into(),
            language: "javascript".into(),
            source: "var value = 1;".into(),
        }],
    };
    let response = serde_json::to_vec(&serde_json::json!({
        "protocol_version": BUNDLER_PROTOCOL_VERSION,
        "transformer": "vite",
        "transformer_version": "vite@6.3.5",
        "profile": "web",
        "generated_source": "var value = 1;"
    }))
    .unwrap();
    assert!(decode_response(&response, &request).is_err());
}
