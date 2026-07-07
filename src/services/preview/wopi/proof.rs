//! WOPI proof-key 验签。
//!
//! Microsoft 365 for the web 会用 discovery 暴露的 proof-key 对每个 WOPI 请求签名。
//! 这里把 proof 组包、时间戳窗口校验和 RSA 验签集中起来，避免这些协议细节散在路由里。

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Duration, Utc};
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, RsaPublicKeyComponents};

use crate::errors::{AsterError, Result};

const DOTNET_TICKS_AT_UNIX_EPOCH: i64 = 621_355_968_000_000_000;
const MAX_PROOF_AGE_MINUTES: i64 = 20;

#[derive(Debug, Clone)]
pub(crate) struct WopiProofKeySet {
    current: WopiProofPublicKey,
    old: Option<WopiProofPublicKey>,
}

#[derive(Debug, Clone)]
struct WopiProofPublicKey {
    modulus: Vec<u8>,
    exponent: Vec<u8>,
}

pub(crate) fn parse_wopi_proof_key_set(
    current_modulus: &str,
    current_exponent: &str,
    old_modulus: Option<&str>,
    old_exponent: Option<&str>,
) -> Result<WopiProofKeySet> {
    let current_modulus = parse_wopi_key_component(current_modulus, "modulus")?;
    let current_exponent = parse_wopi_key_component(current_exponent, "exponent")?;
    validate_wopi_rsa_public_key(&current_modulus, &current_exponent, "current")?;
    let current = WopiProofPublicKey {
        modulus: current_modulus,
        exponent: current_exponent,
    };
    let old = match (
        old_modulus.map(str::trim).filter(|value| !value.is_empty()),
        old_exponent
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    ) {
        (None, None) => None,
        (Some(modulus), Some(exponent)) => {
            let modulus = parse_wopi_key_component(modulus, "old modulus")?;
            let exponent = parse_wopi_key_component(exponent, "old exponent")?;
            validate_wopi_rsa_public_key(&modulus, &exponent, "old")?;
            Some(WopiProofPublicKey { modulus, exponent })
        }
        _ => {
            return Err(AsterError::validation_error(
                "WOPI proof-key old modulus/exponent must be provided together",
            ));
        }
    };

    Ok(WopiProofKeySet { current, old })
}

pub(crate) fn validate_wopi_proof(
    proof_keys: &WopiProofKeySet,
    access_token: &str,
    request_url: &str,
    proof: Option<&str>,
    proof_old: Option<&str>,
    timestamp: Option<&str>,
    now: DateTime<Utc>,
) -> Result<()> {
    let proof = proof.ok_or_else(|| {
        AsterError::internal_error("missing X-WOPI-Proof header for WOPI proof validation")
    })?;
    let timestamp = parse_wopi_timestamp(timestamp)?;
    ensure_wopi_timestamp_is_fresh(timestamp, now)?;

    let expected_proof = build_expected_proof(access_token, request_url, timestamp)?;
    let current_ok = verify_wopi_signature(&proof_keys.current, proof, &expected_proof)?;
    let proof_old_ok = proof_old
        .map(|proof_old| verify_wopi_signature(&proof_keys.current, proof_old, &expected_proof))
        .transpose()?
        .unwrap_or(false);
    let old_key_ok = proof_keys
        .old
        .as_ref()
        .map(|old_key| verify_wopi_signature(old_key, proof, &expected_proof))
        .transpose()?
        .unwrap_or(false);

    if current_ok || proof_old_ok || old_key_ok {
        return Ok(());
    }

    Err(AsterError::internal_error(
        "WOPI proof validation failed for the current request",
    ))
}

fn parse_wopi_key_component(encoded: &str, name: &str) -> Result<Vec<u8>> {
    let decoded = STANDARD.decode(encoded.trim()).map_err(|_| {
        AsterError::validation_error(format!("WOPI proof-key {name} must be valid base64"))
    })?;
    let first_nonzero = decoded
        .iter()
        .position(|value| *value != 0)
        .unwrap_or(decoded.len());
    let trimmed = decoded[first_nonzero..].to_vec();
    if trimmed.is_empty() {
        return Err(AsterError::validation_error(format!(
            "WOPI proof-key {name} must not be zero"
        )));
    }
    Ok(trimmed)
}

fn validate_wopi_rsa_public_key(modulus: &[u8], exponent: &[u8], key_name: &str) -> Result<()> {
    let modulus_bit_len = bit_len(modulus);
    if !(2048..=8192).contains(&modulus_bit_len) {
        return Err(AsterError::validation_error(format!(
            "WOPI proof-key {key_name} modulus must be 2048-8192 bits"
        )));
    }
    if modulus.last().is_some_and(|value| value % 2 == 0) {
        return Err(AsterError::validation_error(format!(
            "WOPI proof-key {key_name} modulus must be odd"
        )));
    }

    let exponent_value = parse_wopi_rsa_exponent(exponent, key_name)?;
    if exponent_value > ((1_u64 << 33) - 1) {
        return Err(AsterError::validation_error(format!(
            "WOPI proof-key {key_name} exponent is too large"
        )));
    }
    if exponent_value < 3 || exponent_value % 2 == 0 {
        return Err(AsterError::validation_error(format!(
            "WOPI proof-key {key_name} exponent must be an odd integer >= 3"
        )));
    }
    Ok(())
}

fn parse_wopi_rsa_exponent(exponent: &[u8], key_name: &str) -> Result<u64> {
    if exponent.len() > 8 {
        return Err(AsterError::validation_error(format!(
            "WOPI proof-key {key_name} exponent is too large"
        )));
    }

    let mut value = 0_u64;
    for byte in exponent {
        value = (value << 8) | u64::from(*byte);
    }
    Ok(value)
}

fn bit_len(bytes: &[u8]) -> usize {
    let Some(first) = bytes.first() else {
        return 0;
    };
    (bytes.len() - 1) * 8 + (u8::BITS as usize - first.leading_zeros() as usize)
}

fn parse_wopi_timestamp(timestamp: Option<&str>) -> Result<i64> {
    let timestamp = timestamp.ok_or_else(|| {
        AsterError::internal_error("missing X-WOPI-TimeStamp header for WOPI proof validation")
    })?;
    timestamp
        .trim()
        .parse::<i64>()
        .map_err(|_| AsterError::internal_error("X-WOPI-TimeStamp must be a valid i64 tick value"))
}

fn ensure_wopi_timestamp_is_fresh(timestamp: i64, now: DateTime<Utc>) -> Result<()> {
    let min_allowed = dotnet_ticks(now - Duration::minutes(MAX_PROOF_AGE_MINUTES));
    let max_allowed = dotnet_ticks(now + Duration::minutes(MAX_PROOF_AGE_MINUTES));
    if timestamp < min_allowed {
        return Err(AsterError::internal_error(
            "WOPI proof timestamp is older than the allowed replay window",
        ));
    }
    if timestamp > max_allowed {
        return Err(AsterError::internal_error(
            "WOPI proof timestamp is newer than the allowed replay window",
        ));
    }
    Ok(())
}

fn build_expected_proof(access_token: &str, request_url: &str, timestamp: i64) -> Result<Vec<u8>> {
    // WOPI proof payload uses the uppercase request URL and network byte order
    // for both the 4-byte length prefixes and the 8-byte timestamp value.
    let upper_request_url = request_url.to_ascii_uppercase();
    let mut payload = Vec::new();
    append_len_prefixed_bytes(&mut payload, access_token.as_bytes())?;
    append_len_prefixed_bytes(&mut payload, upper_request_url.as_bytes())?;
    append_len_prefixed_bytes(&mut payload, &timestamp.to_be_bytes())?;
    Ok(payload)
}

fn append_len_prefixed_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    let len = u32::try_from(bytes.len())
        .map_err(|_| AsterError::internal_error("WOPI proof payload component is too large"))?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

fn verify_wopi_signature(
    key: &WopiProofPublicKey,
    encoded_signature: &str,
    expected_proof: &[u8],
) -> Result<bool> {
    let decoded_signature = STANDARD
        .decode(encoded_signature.trim())
        .map_err(|_| AsterError::internal_error("WOPI proof signature must be valid base64"))?;
    let public_key = RsaPublicKeyComponents {
        n: key.modulus.as_slice(),
        e: key.exponent.as_slice(),
    };
    Ok(public_key
        .verify(
            &RSA_PKCS1_2048_8192_SHA256,
            expected_proof,
            &decoded_signature,
        )
        .is_ok())
}

fn dotnet_ticks(value: DateTime<Utc>) -> i64 {
    value.timestamp_millis() * 10_000 + DOTNET_TICKS_AT_UNIX_EPOCH
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use chrono::{Duration, Utc};
    use rand_08::rngs::OsRng;
    use ring::{
        rand::SystemRandom,
        signature::{RSA_PKCS1_SHA256, RsaKeyPair, RsaPublicKeyComponents},
    };
    use rsa::{RsaPrivateKey, pkcs1::EncodeRsaPrivateKey};

    use super::{
        WopiProofKeySet, build_expected_proof, dotnet_ticks, parse_wopi_proof_key_set,
        validate_wopi_proof,
    };

    static CURRENT_PRIVATE_KEY_DER: OnceLock<Vec<u8>> = OnceLock::new();
    static OLD_PRIVATE_KEY_DER: OnceLock<Vec<u8>> = OnceLock::new();

    fn build_test_keys() -> (RsaKeyPair, RsaKeyPair, WopiProofKeySet) {
        let current = load_private_key(current_private_key_der());
        let old = load_private_key(old_private_key_der());
        let current_public = public_components(&current);
        let old_public = public_components(&old);
        let proof_keys = parse_wopi_proof_key_set(
            &STANDARD.encode(&current_public.n),
            &STANDARD.encode(&current_public.e),
            Some(&STANDARD.encode(&old_public.n)),
            Some(&STANDARD.encode(&old_public.e)),
        )
        .unwrap();

        (current, old, proof_keys)
    }

    fn current_private_key_der() -> &'static [u8] {
        CURRENT_PRIVATE_KEY_DER
            .get_or_init(generate_test_rsa_private_der)
            .as_slice()
    }

    fn old_private_key_der() -> &'static [u8] {
        OLD_PRIVATE_KEY_DER
            .get_or_init(generate_test_rsa_private_der)
            .as_slice()
    }

    fn generate_test_rsa_private_der() -> Vec<u8> {
        let mut rng = OsRng;
        let key = RsaPrivateKey::new(&mut rng, 2048).unwrap();
        key.to_pkcs1_der().unwrap().as_bytes().to_vec()
    }

    fn load_private_key(private_der: &[u8]) -> RsaKeyPair {
        RsaKeyPair::from_der(private_der).unwrap()
    }

    fn public_components(key: &RsaKeyPair) -> RsaPublicKeyComponents<Vec<u8>> {
        RsaPublicKeyComponents::from(key.public())
    }

    fn sign(private_key: &RsaKeyPair, payload: &[u8]) -> String {
        let rng = SystemRandom::new();
        let mut signature = vec![0; private_key.public().modulus_len()];
        private_key
            .sign(&RSA_PKCS1_SHA256, &rng, payload, &mut signature)
            .unwrap();
        STANDARD.encode(signature)
    }

    fn valid_test_modulus() -> Vec<u8> {
        let mut modulus = vec![0_u8; 256];
        modulus[0] = 0x80;
        modulus[255] = 1;
        modulus
    }

    fn build_reference_payload(access_token: &str, request_url: &str, timestamp: i64) -> Vec<u8> {
        let upper_request_url = request_url.to_ascii_uppercase();
        let mut payload = Vec::new();

        let access_token = access_token.as_bytes();
        let access_token_len = u32::try_from(access_token.len()).unwrap_or(u32::MAX);
        payload.extend_from_slice(&access_token_len.to_be_bytes());
        payload.extend_from_slice(access_token);

        let request_url = upper_request_url.as_bytes();
        let request_url_len = u32::try_from(request_url.len()).unwrap_or(u32::MAX);
        payload.extend_from_slice(&request_url_len.to_be_bytes());
        payload.extend_from_slice(request_url);

        let timestamp = timestamp.to_be_bytes();
        let timestamp_len = u32::try_from(timestamp.len()).unwrap_or(u32::MAX);
        payload.extend_from_slice(&timestamp_len.to_be_bytes());
        payload.extend_from_slice(&timestamp);

        payload
    }

    #[test]
    fn build_expected_proof_uses_network_byte_order() {
        let payload = build_expected_proof("token", "https://drive.example.com/wopi", 123).unwrap();
        assert_eq!(
            payload,
            build_reference_payload("token", "https://drive.example.com/wopi", 123)
        );
    }

    #[test]
    fn validate_wopi_proof_accepts_current_signature() {
        let (current, _old, proof_keys) = build_test_keys();
        let now = Utc::now();
        let timestamp = dotnet_ticks(now);
        let payload = build_reference_payload(
            "wopi_token",
            "https://drive.example.com/api/v1/wopi/files/7?access_token=wopi_token",
            timestamp,
        );

        validate_wopi_proof(
            &proof_keys,
            "wopi_token",
            "https://drive.example.com/api/v1/wopi/files/7?access_token=wopi_token",
            Some(&sign(&current, &payload)),
            None,
            Some(&timestamp.to_string()),
            now,
        )
        .unwrap();
    }

    #[test]
    fn validate_wopi_proof_accepts_old_key_rotation_window() {
        let (_current, old, proof_keys) = build_test_keys();
        let now = Utc::now();
        let timestamp = dotnet_ticks(now);
        let payload = build_reference_payload(
            "wopi_token",
            "https://drive.example.com/api/v1/wopi/files/7?access_token=wopi_token",
            timestamp,
        );

        validate_wopi_proof(
            &proof_keys,
            "wopi_token",
            "https://drive.example.com/api/v1/wopi/files/7?access_token=wopi_token",
            Some(&sign(&old, &payload)),
            None,
            Some(&timestamp.to_string()),
            now,
        )
        .unwrap();
    }

    #[test]
    fn validate_wopi_proof_accepts_proof_old_signed_by_current_key() {
        let (current, _old, proof_keys) = build_test_keys();
        let now = Utc::now();
        let timestamp = dotnet_ticks(now);
        let payload = build_reference_payload(
            "wopi_token",
            "https://drive.example.com/api/v1/wopi/files/7?access_token=wopi_token",
            timestamp,
        );

        validate_wopi_proof(
            &proof_keys,
            "wopi_token",
            "https://drive.example.com/api/v1/wopi/files/7?access_token=wopi_token",
            Some(&STANDARD.encode([0_u8; 256])),
            Some(&sign(&current, &payload)),
            Some(&timestamp.to_string()),
            now,
        )
        .unwrap();
    }

    #[test]
    fn validate_wopi_proof_rejects_proof_old_signed_by_old_key() {
        let (_current, old, proof_keys) = build_test_keys();
        let now = Utc::now();
        let timestamp = dotnet_ticks(now);
        let payload = build_reference_payload(
            "wopi_token",
            "https://drive.example.com/api/v1/wopi/files/7?access_token=wopi_token",
            timestamp,
        );

        let err = validate_wopi_proof(
            &proof_keys,
            "wopi_token",
            "https://drive.example.com/api/v1/wopi/files/7?access_token=wopi_token",
            Some(&STANDARD.encode([0_u8; 256])),
            Some(&sign(&old, &payload)),
            Some(&timestamp.to_string()),
            now,
        )
        .unwrap_err();

        assert!(err.message().contains("WOPI proof validation failed"));
    }

    #[test]
    fn validate_wopi_proof_rejects_stale_timestamp() {
        let (current, _old, proof_keys) = build_test_keys();
        let now = Utc::now();
        let timestamp = dotnet_ticks(now - Duration::minutes(21));
        let payload = build_reference_payload(
            "wopi_token",
            "https://drive.example.com/api/v1/wopi/files/7?access_token=wopi_token",
            timestamp,
        );

        let err = validate_wopi_proof(
            &proof_keys,
            "wopi_token",
            "https://drive.example.com/api/v1/wopi/files/7?access_token=wopi_token",
            Some(&sign(&current, &payload)),
            None,
            Some(&timestamp.to_string()),
            now,
        )
        .unwrap_err();

        assert!(
            err.message()
                .contains("older than the allowed replay window")
        );
    }

    #[test]
    fn validate_wopi_proof_rejects_future_timestamp() {
        let (current, _old, proof_keys) = build_test_keys();
        let now = Utc::now();
        let timestamp = dotnet_ticks(now + Duration::minutes(21));
        let payload = build_reference_payload(
            "wopi_token",
            "https://drive.example.com/api/v1/wopi/files/7?access_token=wopi_token",
            timestamp,
        );

        let err = validate_wopi_proof(
            &proof_keys,
            "wopi_token",
            "https://drive.example.com/api/v1/wopi/files/7?access_token=wopi_token",
            Some(&sign(&current, &payload)),
            None,
            Some(&timestamp.to_string()),
            now,
        )
        .unwrap_err();

        assert!(
            err.message()
                .contains("newer than the allowed replay window")
        );
    }

    #[test]
    fn parse_wopi_proof_key_set_requires_old_pairs() {
        let modulus = valid_test_modulus();
        let err = parse_wopi_proof_key_set(
            &STANDARD.encode(modulus),
            &STANDARD.encode([1_u8, 0, 1]),
            Some("AQAB"),
            None,
        )
        .unwrap_err();
        assert!(err.message().contains("must be provided together"));
    }

    #[test]
    fn parse_wopi_proof_key_set_rejects_invalid_rsa_constraints() {
        let modulus = valid_test_modulus();

        let short_modulus_err = parse_wopi_proof_key_set(
            &STANDARD.encode([0x80_u8; 128]),
            &STANDARD.encode([1_u8, 0, 1]),
            None,
            None,
        )
        .unwrap_err();
        assert!(short_modulus_err.message().contains("2048-8192 bits"));

        let even_exponent_err = parse_wopi_proof_key_set(
            &STANDARD.encode(&modulus),
            &STANDARD.encode([2_u8]),
            None,
            None,
        )
        .unwrap_err();
        assert!(even_exponent_err.message().contains("odd integer >= 3"));

        let large_exponent_err = parse_wopi_proof_key_set(
            &STANDARD.encode(modulus),
            &STANDARD.encode([2_u8, 0, 0, 0, 0]),
            None,
            None,
        )
        .unwrap_err();
        assert!(large_exponent_err.message().contains("too large"));
    }
}
