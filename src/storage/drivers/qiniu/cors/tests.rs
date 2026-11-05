use super::*;

fn origins() -> Vec<String> {
    vec!["https://drive.test".into()]
}

#[test]
fn native_rule_covers_actual_browser_methods_headers_and_uses_no_rule_id() {
    let rule = asterdrive_rule(&origins());
    let wire = serde_json::to_value(&rule).unwrap();
    assert!(wire.get("id").is_none());
    assert_eq!(
        wire["allowed_method"],
        serde_json::json!(["POST", "PUT", "GET", "HEAD"])
    );
    assert_eq!(
        wire["allowed_header"],
        serde_json::json!(["Authorization", "Content-Type", "Range"])
    );
    assert!(rule.exposed_header.contains(&"ETag".into()));
    assert!(!needs_rule(&[rule], &origins()).unwrap());
    for value in [b"[]".as_slice(), b"null".as_slice()] {
        assert!(parse_rules(value).unwrap().is_empty());
    }
}

#[test]
fn keeps_unrelated_and_same_origin_rules_without_claiming_ownership() {
    let unrelated = asterdrive_rule(&["https://other.test".into()]);
    assert!(needs_rule(std::slice::from_ref(&unrelated), &origins()).unwrap());
    let mut same_origin = asterdrive_rule(&origins());
    same_origin.allowed_method = vec!["POST".into()];
    same_origin.exposed_header.clear();
    assert!(needs_rule(&[unrelated, same_origin], &origins()).unwrap());
    let mut equivalent = asterdrive_rule(&origins());
    equivalent.allowed_method.reverse();
    equivalent.allowed_header = vec!["*".into()];
    equivalent.exposed_header = equivalent
        .exposed_header
        .iter()
        .map(|h| h.to_ascii_lowercase())
        .collect();
    assert!(!needs_rule(&[equivalent], &origins()).unwrap());
}

#[test]
fn first_matching_rule_is_authoritative_for_simple_reads_and_preflight() {
    let complete = asterdrive_rule(&origins());
    let mut shadow = complete.clone();
    shadow.allowed_origin = vec!["https://*.test".into()];
    shadow.allowed_method = vec!["GET".into()];
    shadow.exposed_header.clear();
    assert_eq!(
        needs_rule(&[shadow.clone(), complete.clone()], &origins())
            .unwrap_err()
            .kind(),
        StorageErrorKind::Precondition
    );
    assert!(!needs_rule(&[complete, shadow], &origins()).unwrap());
    for (pattern, value, expected) in [
        ("*", "https://drive.test", true),
        ("https://*.test", "https://drive.test", true),
        ("https://drive.test*", "https://drive.test:8443", true),
        ("https://*.test", "https://drive.test.evil", false),
        ("https://*.test", "https://DRIVE.TEST", false),
        ("x*x", "x", false),
        ("*.*", "drive.test", false),
    ] {
        assert_eq!(
            wildcard_matches(pattern, value),
            expected,
            "{pattern} {value}"
        );
    }
}

#[test]
fn header_matching_uses_only_full_wildcard_and_respects_cors_safelisted_response_headers() {
    let mut rule = asterdrive_rule(&origins());
    rule.allowed_header = vec!["author*".into(), "content-type".into(), "range".into()];
    assert!(needs_rule(&[rule.clone()], &origins()).unwrap());
    rule.allowed_header[0] = "AUTHORIZATION".into();
    rule.exposed_header
        .retain(|h| !matches!(h.as_str(), "Content-Length" | "X-Reqid" | "X-Log"));
    assert!(!needs_rule(&[rule], &origins()).unwrap());
}

#[test]
fn refuses_unknown_fields_malformed_models_and_rule_overflow_instead_of_dropping_data() {
    let rule = asterdrive_rule(&origins());
    let mut unknown = serde_json::to_value(&rule).unwrap();
    unknown["future_setting"] = serde_json::json!({"keep":"me"});
    assert!(parse_rules(&serde_json::to_vec(&vec![unknown]).unwrap()).is_err());
    for invalid in [
        "",
        "{}",
        "[",
        "[{\"allowed_origin\":[],\"allowed_method\":[\"GET\"]}]",
        "[{\"allowed_origin\":[\"https://drive.test\"],\"allowed_method\":[],\"max_age\":-1}]",
    ] {
        assert!(parse_rules(invalid.as_bytes()).is_err());
    }
    assert_eq!(
        parse_rules(&serde_json::to_vec(&vec![rule.clone(); 10]).unwrap())
            .unwrap()
            .len(),
        10
    );
    assert!(parse_rules(&serde_json::to_vec(&vec![rule; 11]).unwrap()).is_err());
}

#[test]
fn origin_boundaries_reject_broad_or_non_browser_inputs_before_io() {
    for value in [
        "*",
        "https://*.test",
        "https://drive.test:443",
        "https://drive.test/path",
        "https://u:p@drive.test",
        "https://drive.test?q=1",
        "https://drive.test#fragment",
        "file:///tmp/file",
    ] {
        assert!(validate_origins(&[value.into()]).is_err(), "{value}");
    }
    assert!(validate_origins(&[]).is_err());
    validate_origins(&["https://drive.test".into(), "https://[::1]:8443".into()]).unwrap();
}

#[test]
fn readback_preserves_rule_order_but_allows_provider_case_and_field_order_normalization() {
    let a = asterdrive_rule(&origins());
    let b = asterdrive_rule(&["https://other.test".into()]);
    let mut changed = a.clone();
    changed.allowed_method.reverse();
    changed.exposed_header = changed
        .exposed_header
        .iter()
        .map(|v| v.to_ascii_lowercase())
        .collect();
    assert!(rules_equivalent(std::slice::from_ref(&a), &[changed]));
    assert!(!rules_equivalent(&[a.clone(), b.clone()], &[b, a]));
}

#[test]
fn json_management_signature_matches_an_independent_python_hmac_vector() {
    let body = r#"[{"allowed_origin":["https://drive.test"],"allowed_method":["GET"]}]"#;
    let request = reqwest::Client::new()
        .post("https://uc.qiniuapi.com/corsRules/set/archive")
        .header("content-type", "application/json")
        .body(body)
        .build()
        .unwrap();
    let payload = super::super::auth::management_signing_payload(
        &request,
        "20261104T120000Z",
        "application/json",
    )
    .unwrap();
    assert_eq!(
        payload,
        format!(
            "POST /corsRules/set/archive\nHost: uc.qiniuapi.com\nContent-Type: application/json\nX-Qiniu-Date: 20261104T120000Z\n\n{body}"
        )
    );
    let driver = super::super::tests::driver();
    // Python stdlib hmac.new(SK, payload, hashlib.sha1), urlsafe_b64encode.
    assert_eq!(
        driver.sign(payload.as_bytes()).unwrap(),
        "MY_ACCESS_KEY:44Fbk50gqFkW7D5KtE9gZZf-Y5U="
    );
}
