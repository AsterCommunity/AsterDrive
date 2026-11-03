use super::*;

fn config() -> CdnConfig {
    CdnConfig {
        domain: validated_cdn_domain("https://cdn.example.test").unwrap(),
        auth_key: "huaweicloud12345".into(),
        validity_seconds: 300,
        auth_param: "auth_key".into(),
        base_path: String::new(),
    }
}

#[test]
fn type_a_sha256_matches_documented_string_and_independent_digest() {
    // Huawei's Type A example input, independently hashed with Python hashlib.
    // The final URL in the vendor page contradicts its own input: use the
    // documented Filename-Timestamp-rand-uid-PrivateKey formula instead.
    let result = config()
        .download_request_at(
            "T128_2_1_0_sdk/0210/M00/82/3E/test.mp3",
            Duration::from_secs(300),
            &Default::default(),
            1498752000,
            "0",
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        result.url,
        concat!(
            "https://cdn.example.test/T128_2_1_0_sdk/0210/M00/82/3E/test.mp3?auth_key=1498752000-0-0-",
            "5694e98862185889e6944defeebd48bb014c7472d228b92b120c1728062c7ca0",
        )
    );
    assert_eq!(result.expires_at.unwrap().timestamp(), 1498752300);
    assert_eq!(
        result.credentials,
        aster_drive_storage::DirectDownloadCredentials::Omit
    );
}

#[test]
fn type_a_signs_the_encoded_wire_path_including_prefix_and_custom_parameter() {
    let mut config = config();
    config.base_path = "Tenant/Prefix".into();
    config.auth_param = "Download_Auth".into();
    let result = config
        .download_request_at(
            "报告 +#%2F/UPPER.txt",
            Duration::from_secs(300),
            &Default::default(),
            1700000000,
            "abc",
        )
        .unwrap()
        .unwrap();
    let url = Url::parse(&result.url).unwrap();
    assert_eq!(
        url.path(),
        "/Tenant/Prefix/%E6%8A%A5%E5%91%8A%20+%23%252F/UPPER.txt"
    );
    let query = url.query_pairs().collect::<Vec<_>>();
    assert_eq!(query.len(), 1);
    assert_eq!(query[0].0, "Download_Auth");
    assert_eq!(
        query[0].1,
        "1700000000-abc-0-fc6fc826dcdddbb2f2d14cd036d0d44286c0de84c8638c4217cef47bddc63803"
    );
}

#[test]
fn type_a_bounds_expiration_by_both_caller_and_console() {
    let config = config();
    for requested in [1, 60, 300, 301, 3600, u64::MAX] {
        let result = config
            .download_request_at(
                "file",
                Duration::from_secs(requested),
                &Default::default(),
                1700000000,
                "0",
            )
            .unwrap()
            .unwrap();
        let url = Url::parse(&result.url).unwrap();
        let auth = url.query_pairs().next().unwrap().1;
        let timestamp: u64 = auth.split('-').next().unwrap().parse().unwrap();
        let deadline = 1700000000 + requested.min(300);
        assert!(timestamp <= 1700000000);
        assert_eq!(timestamp + 300, deadline);
        assert_eq!(
            u64::try_from(result.expires_at.unwrap().timestamp()).unwrap(),
            deadline
        );
    }
}

#[test]
fn type_a_rejects_invalid_time_and_overflow() {
    for (duration, now) in [
        (Duration::ZERO, 1700000000),
        (Duration::from_millis(500), 1700000000),
        (Duration::from_secs(1), 1),
        (Duration::from_secs(300), u64::MAX),
        (Duration::from_secs(300), u64::try_from(i64::MAX).unwrap()),
    ] {
        assert!(
            config()
                .download_request_at("file", duration, &Default::default(), now, "0")
                .is_err()
        );
    }
}

#[test]
fn type_a_uses_uuid_nonce_and_omits_origin_authentication() {
    let result = config()
        .download_request("file", Duration::from_secs(300), &Default::default())
        .unwrap()
        .unwrap();
    let url = Url::parse(&result.url).unwrap();
    let pairs = url.query_pairs().collect::<Vec<_>>();
    assert_eq!(pairs.len(), 1);
    let parts = pairs[0].1.split('-').collect::<Vec<_>>();
    assert_eq!(parts.len(), 4);
    assert_eq!(parts[1].len(), 32);
    assert!(parts[1].bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(parts[2], "0");
    assert_eq!(parts[3].len(), 64);
    assert!(
        parts[3]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    assert!(!result.url.contains("AccessKeyId"));
    assert!(!result.url.contains("Signature="));
}

#[test]
fn type_a_rejects_traversal_controls_and_empty_objects() {
    for key in [
        "",
        "/",
        ".",
        "..",
        "folder/../file",
        "folder/./file",
        "file\nname",
        "file\0name",
        "file\\name",
    ] {
        assert!(
            config()
                .download_request_at(
                    key,
                    Duration::from_secs(300),
                    &Default::default(),
                    1700000000,
                    "0"
                )
                .is_err(),
            "{key:?}"
        );
    }
    let mut config = config();
    config.base_path = "prefix/../private".into();
    assert!(
        config
            .download_request_at(
                "file",
                Duration::from_secs(300),
                &Default::default(),
                1700000000,
                "0"
            )
            .is_err()
    );
}

#[test]
fn type_a_preserves_literal_percent_encoded_segments() {
    let result = config()
        .download_request_at(
            "%2e%2e/%2F.txt",
            Duration::from_secs(300),
            &Default::default(),
            1700000000,
            "0",
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        Url::parse(&result.url).unwrap().path(),
        "/%252e%252e/%252F.txt"
    );
}

#[test]
fn strict_response_headers_and_exact_names_use_relay() {
    for options in [
        DirectDownloadOptions {
            response_cache_control: Some("private, no-store".into()),
            ..Default::default()
        },
        DirectDownloadOptions {
            response_content_type: Some("text/plain".into()),
            ..Default::default()
        },
        DirectDownloadOptions {
            response_content_disposition: Some("attachment".into()),
            ..Default::default()
        },
        DirectDownloadOptions {
            require_download_name_match: true,
            download_name: Some("renamed.txt".into()),
            allow_provider_response_headers: true,
            ..Default::default()
        },
    ] {
        assert!(
            config()
                .download_request_at("file", Duration::from_secs(300), &options, 1700000000, "0")
                .unwrap()
                .is_none()
        );
    }
    let options = DirectDownloadOptions {
        allow_provider_response_headers: true,
        response_content_disposition: Some("inline".into()),
        response_content_type: Some("text/plain".into()),
        response_cache_control: Some("private, no-store".into()),
        ..Default::default()
    };
    let result = config()
        .download_request_at("file", Duration::from_secs(300), &options, 1700000000, "0")
        .unwrap()
        .unwrap();
    assert_eq!(Url::parse(&result.url).unwrap().query_pairs().count(), 1);
    assert!(!result.url.contains("response-"));
}

#[test]
fn cdn_domain_rejects_unsafe_or_ambiguous_urls_without_echoing_input() {
    for input in [
        "http://cdn.example.test",
        "https://@cdn.example.test",
        "https://private:secret@cdn.example.test",
        "https://cdn.example.test/prefix",
        "https://cdn.example.test/prefix/..",
        "https://cdn.example.test/%2e",
        "https://cdn.example.test/%2e%2e",
        "https://cdn.example.test?query=secret",
        "https://cdn.example.test#fragment",
        "https://cdn.example.test:8443",
        "https://cdn.example.test\\prefix",
        "https://cdn.example.test/\n",
        "\thttps://cdn.example.test",
        "https://localhost",
        "https://127.0.0.1",
        "https://[::1]",
        "cdn.example.test",
    ] {
        let error = validated_cdn_domain(input).unwrap_err();
        assert_eq!(error.kind(), StorageErrorKind::Misconfigured);
        assert!(!error.message().contains("secret"));
    }
    for input in [
        "https://cdn.example.test",
        "https://cdn.example.test/",
        "https://cdn.example.test:443",
    ] {
        validated_cdn_domain(input).unwrap();
    }
}

#[test]
fn cdn_credentials_and_parameter_follow_huawei_constraints() {
    for length in [6, 8, 16, 32] {
        validate_cdn_auth_key(&"a".repeat(length)).unwrap();
    }
    for key in [
        "",
        "abc12",
        &"a".repeat(33),
        "秘密123456",
        "secret-123",
        "secret\n123",
    ] {
        assert!(validate_cdn_auth_key(key).is_err());
    }
    for param in ["a", "auth_key", "Download_Auth123", &"a".repeat(100)] {
        validate_cdn_auth_param(param).unwrap();
    }
    for param in [
        "",
        "1auth",
        "_auth",
        "auth-key",
        "auth&key",
        "auth key",
        "认证",
        &"a".repeat(101),
    ] {
        assert!(validate_cdn_auth_param(param).is_err());
    }
    for validity in [1, 300, 31_536_000] {
        validate_cdn_auth_validity(validity).unwrap();
    }
    for validity in [0, 31_536_001, u32::MAX] {
        assert!(validate_cdn_auth_validity(validity).is_err());
    }
}
