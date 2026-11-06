//! Migration-only S3 identity verification. Runtime Kodo never falls back to S3.
//! Native discovery follows pinned Go SDK /buckets and /v4/query io_src contracts.
use super::{
    QiniuDownloadDelivery, QiniuDriver, QiniuDriverConfig, QiniuStaticCredentials, http, invalid,
};
use aster_drive_storage::{Result, StorageDriver, StorageErrorKind, storage_driver_error};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use url::Url;

// Bound simultaneous region queries and nonce reads without serializing every
// bucket in large/shared accounts. The outer per-connection upgrade deadline
// still bounds the full scan, and dropping it cancels these owned futures.
const MAX_CONCURRENT_CANDIDATE_PROBES: usize = 16;

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyConfig {
    pub endpoint: String,
    pub bucket: String,
    pub base_path: String,
    pub s3_region: String,
    pub object_storage_upload_strategy: aster_drive_model::types::ObjectStorageUploadStrategy,
    pub object_storage_download_strategy: aster_drive_model::types::ObjectStorageDownloadStrategy,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeLocation {
    pub bucket: String,
    pub download_origin: String,
}

#[async_trait::async_trait]
pub(crate) trait NativeUpgradeResolver: Send + Sync {
    async fn resolve(
        &self,
        old: &LegacyConfig,
        credentials: QiniuStaticCredentials,
    ) -> Result<NativeLocation>;
}

pub(crate) struct OfficialResolver;

#[derive(Deserialize)]
struct Service {
    #[serde(default)]
    domains: Vec<String>,
}

#[derive(Deserialize)]
struct S3Region {
    region_alias: String,
}

#[derive(Deserialize)]
struct Region {
    io_src: Service,
    s3: S3Region,
}

#[derive(Deserialize)]
struct Query {
    hosts: Vec<Region>,
}

/// A temporary random marker confirms identity even for an empty bucket. It cannot
/// overwrite a user key, and is removed through the original authoritative namespace.
struct ProbeCleanup {
    driver: Arc<dyn StorageDriver>,
    key: Option<String>,
}

impl Drop for ProbeCleanup {
    fn drop(&mut self) {
        let Some(key) = self.key.take() else {
            return;
        };
        let driver = self.driver.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                if let Err(error) = driver.delete(&key).await {
                    tracing::warn!(
                        kind = error.kind().as_str(),
                        "Kodo migration probe cleanup failed"
                    );
                }
            });
        } else {
            tracing::warn!("Kodo migration runtime stopped before probe cleanup");
        }
    }
}

#[async_trait::async_trait]
impl NativeUpgradeResolver for OfficialResolver {
    async fn resolve(
        &self,
        old: &LegacyConfig,
        credentials: QiniuStaticCredentials,
    ) -> Result<NativeLocation> {
        let source = Arc::new(legacy_source_driver(old, &credentials)?);
        let signer = discovery_driver(
            old.bucket.clone(),
            old.base_path.clone(),
            "https://unused.invalid".into(),
            credentials.clone(),
        )?;
        let discovery = OfficialDiscovery {
            signer,
            credentials,
            bucket_url: Url::parse("https://uc.qiniuapi.com/buckets?shared=rd")
                .map_err(|_| invalid("invalid Kodo bucket endpoint"))?,
        };
        verify_namespace(old, source, &discovery).await
    }
}

// Keep legacy endpoint validation and S3 construction together. Namespace
// verification only receives a driver whose endpoint matches the saved region.
fn legacy_source_driver(
    old: &LegacyConfig,
    credentials: &QiniuStaticCredentials,
) -> Result<crate::storage::drivers::s3::S3Driver> {
    use crate::storage::drivers::s3::{
        S3Driver, S3DriverConfig, S3DriverOptions, S3StaticCredentials,
    };
    let endpoint =
        Url::parse(&old.endpoint).map_err(|_| invalid("invalid legacy Kodo endpoint"))?;
    let official = format!("s3.{}.qiniucs.com", old.s3_region);
    if endpoint.scheme() != "https"
        || !matches!(endpoint.host_str(),Some(host) if host==official || host==format!("{}.{}",old.bucket,official))
        || endpoint.port().is_some()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.path() != "/"
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(invalid(
            "legacy Kodo upgrade requires the configured official S3 endpoint and matching region",
        ));
    }
    S3Driver::new(
        S3DriverConfig {
            endpoint: format!("https://{official}"),
            bucket: old.bucket.clone(),
            base_path: old.base_path.clone(),
            region: old.s3_region.clone(),
            path_style: false,
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(20),
            operation_timeout: Duration::from_secs(30),
        },
        S3StaticCredentials {
            access_key: credentials.access_key.clone(),
            secret_key: credentials.secret_key.clone(),
        },
        S3DriverOptions::virtual_hosted_style(),
        |builder| {
            builder
                .request_checksum_calculation(
                    aws_sdk_s3::config::RequestChecksumCalculation::WhenRequired,
                )
                .response_checksum_validation(
                    aws_sdk_s3::config::ResponseChecksumValidation::WhenRequired,
                )
        },
    )
}

#[async_trait::async_trait]
trait UpgradeDiscovery: Send + Sync {
    async fn buckets(&self) -> Result<Vec<String>>;

    async fn source(&self, bucket: &str, region: &str) -> Result<Option<String>>;

    async fn contains(
        &self,
        bucket: &str,
        origin: &str,
        prefix: &str,
        key: &str,
        nonce: &[u8],
    ) -> Result<bool>;
}

struct OfficialDiscovery {
    signer: QiniuDriver,
    credentials: QiniuStaticCredentials,
    bucket_url: Url,
}

#[async_trait::async_trait]
impl UpgradeDiscovery for OfficialDiscovery {
    async fn buckets(&self) -> Result<Vec<String>> {
        http::decode(
            self.signer
                .management(reqwest::Method::GET, self.bucket_url.clone())
                .await?,
        )
        .await
    }

    async fn source(&self, bucket: &str, region: &str) -> Result<Option<String>> {
        let mut query = self.signer.region_query.clone();
        query
            .query_pairs_mut()
            .append_pair("ak", &self.credentials.access_key)
            .append_pair("bucket", bucket);
        let response: Query = http::decode(
            self.signer
                .control_client
                .get(query)
                .send()
                .await
                .map_err(http::transport_error)?,
        )
        .await?;
        let Some(region) = response
            .hosts
            .into_iter()
            .find(|host| host.s3.region_alias == region)
        else {
            return Ok(None);
        };
        let origin = match region.io_src.domains.first() {
            Some(origin) => origin,
            None => return Ok(None),
        };
        Ok(Some(
            super::download::validate_domain(&format!("https://{origin}"))?
                .as_str()
                .trim_end_matches('/')
                .to_string(),
        ))
    }

    async fn contains(
        &self,
        bucket: &str,
        origin: &str,
        prefix: &str,
        key: &str,
        nonce: &[u8],
    ) -> Result<bool> {
        let candidate = discovery_driver(
            bucket.to_string(),
            prefix.to_string(),
            origin.to_string(),
            self.credentials.clone(),
        )?;
        candidate.read_upgrade_probe(key, nonce).await
    }
}

async fn verify_namespace(
    old: &LegacyConfig,
    source: Arc<dyn StorageDriver>,
    discovery: &dyn UpgradeDiscovery,
) -> Result<NativeLocation> {
    let candidates = discovery
        .buckets()
        .await?
        .into_iter()
        .collect::<BTreeSet<_>>();
    if candidates.is_empty() || candidates.len() > 10_000 {
        return Err(invalid(
            "Kodo bucket enumeration returned an empty or excessive candidate set",
        ));
    }
    let key = format!("_aster-native-upgrade-{}", uuid::Uuid::new_v4());
    let nonce = uuid::Uuid::new_v4().as_bytes().to_vec();
    let mut cleanup = ProbeCleanup {
        driver: source.clone(),
        key: Some(key.clone()),
    };
    let result = async {
        use futures::{StreamExt, stream};
        source.put(&key, &nonce).await.map_err(|_| {
            invalid("legacy Kodo identity probe upload failed; old configuration retained")
        })?;
        // Never return at the first match: later candidates must still be checked
        // to reject ambiguous mappings. An unrelated failure is only diagnostic.
        let mut probes = stream::iter(candidates)
            .map(|bucket| {
                let key = &key;
                let nonce = &nonce;
                async move {
                    let Some(origin) = discovery.source(&bucket, &old.s3_region).await? else {
                        return Ok(None);
                    };
                    match discovery
                        .contains(&bucket, &origin, &old.base_path, key, nonce)
                        .await
                    {
                        Ok(true) => Ok(Some(NativeLocation {
                            bucket,
                            download_origin: origin,
                        })),
                        Ok(false) => Ok(None),
                        Err(error) if error.kind() == StorageErrorKind::NotFound => Ok(None),
                        Err(error) => Err(error),
                    }
                }
            })
            .buffer_unordered(MAX_CONCURRENT_CANDIDATE_PROBES);
        let mut matches = Vec::new();
        let mut first_error = None;
        while let Some(result) = probes.next().await {
            match result {
                Ok(Some(location)) => matches.push(location),
                Ok(None) => {}
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        // Only a positive nonce match proves identity. An unrelated candidate's
        // failure neither invalidates that proof nor excuses ambiguous matches.
        if matches.is_empty()
            && let Some(error) = first_error
        {
            return Err(error);
        }
        if matches.len() != 1 {
            return Err(storage_driver_error(
                StorageErrorKind::Precondition,
                "Kodo upgrade could not confirm exactly one native bucket for the legacy S3 namespace",
            ));
        }
        matches
            .pop()
            .ok_or_else(|| invalid("Kodo identity resolution failed"))
    }
    .await;
    // Configuration may only be converted once the probe is removed, even when
    // identification failed. Keep the drop guard armed if explicit cleanup fails.
    source
        .delete(&key)
        .await
        .map_err(|_| invalid("Kodo identity probe cleanup failed; no configuration was changed"))?;
    cleanup.key = None;
    result
}

fn discovery_driver(
    bucket: String,
    base_path: String,
    origin: String,
    credentials: QiniuStaticCredentials,
) -> Result<QiniuDriver> {
    QiniuDriver::new(
        QiniuDriverConfig {
            bucket,
            base_path,
            download_origin: origin,
            download_delivery: QiniuDownloadDelivery::Origin,
            download_base_url: None,
            download_auth_validity_seconds: 180,
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(15),
            operation_timeout: Duration::from_secs(20),
        },
        credentials,
    )
}

impl QiniuDriver {
    async fn read_upgrade_probe(&self, path: &str, expected: &[u8]) -> Result<bool> {
        use futures::StreamExt;
        // This tiny bounded GET is control traffic: a total timeout is intentional,
        // unlike normal object downloads which can keep streaming indefinitely.
        let url = self.private_url(
            &self.config.download_origin,
            path,
            chrono::Utc::now().timestamp() + 60,
            &[],
        )?;
        let response = http::checked(
            self.control_client
                .get(url)
                .header("accept-encoding", "identity")
                .send()
                .await
                .map_err(http::transport_error)?,
        )
        .await?;
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::with_capacity(expected.len());
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(http::transport_error)?;
            if bytes.len().saturating_add(chunk.len()) > expected.len() {
                return Ok(false);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes == expected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct Source {
        objects: Mutex<std::collections::BTreeMap<String, Vec<u8>>>,
        writes: AtomicUsize,
        deletes: AtomicUsize,
        fail_delete: bool,
    }

    #[async_trait::async_trait]
    impl StorageDriver for Source {
        async fn put(&self, key: &str, data: &[u8]) -> Result<String> {
            assert!(key.starts_with("_aster-native-upgrade-"));
            assert_eq!(data.len(), 16);
            self.writes.fetch_add(1, Ordering::SeqCst);
            self.objects.lock().unwrap().insert(key.into(), data.into());
            Ok(key.into())
        }

        async fn get(&self, key: &str) -> Result<Vec<u8>> {
            self.objects
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .ok_or_else(|| storage_driver_error(StorageErrorKind::NotFound, "absent probe"))
        }

        async fn get_stream(
            &self,
            _: &str,
        ) -> Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>> {
            Err(invalid("probe must not read user streams"))
        }

        async fn delete(&self, key: &str) -> Result<()> {
            self.deletes.fetch_add(1, Ordering::SeqCst);
            if self.fail_delete {
                return Err(invalid("injected cleanup failure"));
            }
            self.objects.lock().unwrap().remove(key);
            Ok(())
        }

        async fn exists(&self, key: &str) -> Result<bool> {
            Ok(self.objects.lock().unwrap().contains_key(key))
        }

        async fn metadata(&self, _: &str) -> Result<aster_drive_storage::BlobMetadata> {
            Err(invalid(
                "probe uses nonce equality, not metadata coincidence",
            ))
        }
    }

    struct Discovery {
        source: Arc<Source>,
        matching: Vec<&'static str>,
        outage: bool,
    }

    #[async_trait::async_trait]
    impl UpgradeDiscovery for Discovery {
        async fn buckets(&self) -> Result<Vec<String>> {
            Ok(vec!["candidate-a".into(), "candidate-b".into()])
        }

        async fn source(&self, bucket: &str, _: &str) -> Result<Option<String>> {
            Ok(Some(format!("https://{bucket}.provider.test")))
        }

        async fn contains(
            &self,
            bucket: &str,
            _: &str,
            prefix: &str,
            key: &str,
            nonce: &[u8],
        ) -> Result<bool> {
            assert_eq!(prefix, "existing/prefix");
            assert_eq!(self.source.get(key).await?, nonce);
            if self.outage {
                return Err(storage_driver_error(
                    StorageErrorKind::Transient,
                    "injected network failure",
                ));
            }
            Ok(self.matching.contains(&bucket))
        }
    }

    fn old() -> LegacyConfig {
        LegacyConfig {
            endpoint: "https://s3.cn-east-1.qiniucs.com".into(),
            bucket: "different-s3-alias".into(),
            base_path: "existing/prefix".into(),
            s3_region: "cn-east-1".into(),
            object_storage_upload_strategy:
                aster_drive_model::types::ObjectStorageUploadStrategy::Presigned,
            object_storage_download_strategy:
                aster_drive_model::types::ObjectStorageDownloadStrategy::RelayStream,
        }
    }

    #[tokio::test]
    async fn confirms_different_native_name_with_random_nonce_and_cleans_only_its_probe() {
        let source = Arc::new(Source::default());
        source
            .objects
            .lock()
            .unwrap()
            .insert("user-owned-object".into(), b"do-not-touch".to_vec());
        let discovery = Discovery {
            source: source.clone(),
            matching: vec!["candidate-b"],
            outage: false,
        };
        let location = verify_namespace(&old(), source.clone(), &discovery)
            .await
            .unwrap();
        assert_eq!(location.bucket, "candidate-b");
        assert_eq!(
            location.download_origin,
            "https://candidate-b.provider.test"
        );
        assert_eq!(source.writes.load(Ordering::SeqCst), 1);
        assert_eq!(source.deletes.load(Ordering::SeqCst), 1);
        assert_eq!(source.objects.lock().unwrap().len(), 1);
        assert_eq!(
            source.get("user-owned-object").await.unwrap(),
            b"do-not-touch"
        );
    }

    #[tokio::test]
    async fn no_mapping_multiple_mappings_and_outages_never_guess_and_always_clean() {
        for (matching, outage) in [
            (vec![], false),
            (vec!["candidate-a", "candidate-b"], false),
            (vec![], true),
        ] {
            let source = Arc::new(Source::default());
            let discovery = Discovery {
                source: source.clone(),
                matching,
                outage,
            };
            assert!(
                verify_namespace(&old(), source.clone(), &discovery)
                    .await
                    .is_err()
            );
            assert!(source.objects.lock().unwrap().is_empty());
            assert_eq!(source.deletes.load(Ordering::SeqCst), 1);
        }
    }

    #[tokio::test]
    async fn cleanup_failure_prevents_configuration_conversion() {
        let source = Arc::new(Source {
            fail_delete: true,
            ..Default::default()
        });
        let discovery = Discovery {
            source: source.clone(),
            matching: vec!["candidate-b"],
            outage: false,
        };
        assert!(verify_namespace(&old(), source, &discovery).await.is_err());
    }

    struct CandidateDiscovery {
        source: Arc<Source>,
        matching: Vec<String>,
        query_failures: Vec<String>,
        read_failures: Vec<String>,
        active: AtomicUsize,
        peak: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl UpgradeDiscovery for CandidateDiscovery {
        async fn buckets(&self) -> Result<Vec<String>> {
            Ok((0..48).map(|n| format!("bucket-{n:02}")).collect())
        }

        async fn source(&self, bucket: &str, _: &str) -> Result<Option<String>> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(active, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(5)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            if self.query_failures.iter().any(|name| name == bucket) {
                return Err(storage_driver_error(
                    StorageErrorKind::Permission,
                    "query denied",
                ));
            }
            Ok(Some(format!("https://{bucket}.provider.test")))
        }

        async fn contains(
            &self,
            bucket: &str,
            _: &str,
            _: &str,
            key: &str,
            nonce: &[u8],
        ) -> Result<bool> {
            assert_eq!(self.source.get(key).await?, nonce);
            if self.read_failures.iter().any(|name| name == bucket) {
                return Err(storage_driver_error(
                    StorageErrorKind::Transient,
                    "read failed",
                ));
            }
            Ok(self.matching.iter().any(|name| name == bucket))
        }
    }

    #[tokio::test]
    async fn candidate_errors_do_not_hide_unique_nonce_match_and_probes_are_bounded_parallel() {
        for query_failure in [false, true] {
            let source = Arc::new(Source::default());
            let failures = vec!["bucket-00".into(), "bucket-47".into()];
            let discovery = CandidateDiscovery {
                source: source.clone(),
                matching: vec!["bucket-24".into()],
                query_failures: if query_failure {
                    failures.clone()
                } else {
                    vec![]
                },
                read_failures: if query_failure { vec![] } else { failures },
                active: AtomicUsize::new(0),
                peak: AtomicUsize::new(0),
            };
            let result = verify_namespace(&old(), source.clone(), &discovery)
                .await
                .unwrap();
            assert_eq!(result.bucket, "bucket-24");
            let peak = discovery.peak.load(Ordering::SeqCst);
            assert!((2..=16).contains(&peak), "unexpected concurrency: {peak}");
            assert_eq!(source.deletes.load(Ordering::SeqCst), 1);
            assert!(source.objects.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn candidate_errors_propagate_without_match_and_never_allow_ambiguous_match() {
        for matching in [vec![], vec!["bucket-23".into(), "bucket-24".into()]] {
            let source = Arc::new(Source::default());
            let expected = if matching.is_empty() {
                StorageErrorKind::Permission
            } else {
                StorageErrorKind::Precondition
            };
            let discovery = CandidateDiscovery {
                source: source.clone(),
                matching,
                query_failures: vec!["bucket-00".into()],
                read_failures: vec![],
                active: AtomicUsize::new(0),
                peak: AtomicUsize::new(0),
            };
            let error = verify_namespace(&old(), source.clone(), &discovery)
                .await
                .err()
                .unwrap();
            assert_eq!(error.kind(), expected);
            assert_eq!(source.deletes.load(Ordering::SeqCst), 1);
            assert!(source.objects.lock().unwrap().is_empty());
        }
    }

    fn credentials() -> QiniuStaticCredentials {
        QiniuStaticCredentials {
            access_key: "AK".into(),
            secret_key: "SK".into(),
        }
    }

    #[tokio::test]
    async fn official_discovery_authenticates_buckets_and_selects_matching_https_source() {
        use crate::storage::test_support::{HttpFixture, HttpReply};
        let region = |alias: &str, domains: serde_json::Value| serde_json::json!({"io_src":{"domains":domains},"s3":{"region_alias":alias}});
        let peer=HttpFixture::new([
            HttpReply::json(200,serde_json::json!(["native-a","native-b"])),
            HttpReply::json(200,serde_json::json!({"hosts":[region("other",serde_json::json!(["wrong.example.test"])),region("cn-east-1",serde_json::json!(["source.example.test"]))]})),
            HttpReply::json(200,serde_json::json!({"hosts":[region("other",serde_json::json!(["wrong.example.test"]))]})),
            HttpReply::json(200,serde_json::json!({"hosts":[region("cn-east-1",serde_json::json!([]))]})),
            HttpReply::json(200,serde_json::json!({"hosts":[region("cn-east-1",serde_json::json!(["user:password@source.example.test"]))]})),
            HttpReply::text(503,"unavailable"),
        ]).await;
        let mut signer = discovery_driver(
            "old-alias".into(),
            "existing/prefix".into(),
            "https://unused.test".into(),
            credentials(),
        )
        .unwrap();
        signer.region_query = Url::parse(&format!("{}/v4/query", peer.base)).unwrap();
        let discovery = OfficialDiscovery {
            signer,
            credentials: credentials(),
            bucket_url: Url::parse(&format!("{}/buckets?shared=rd", peer.base)).unwrap(),
        };
        assert_eq!(discovery.buckets().await.unwrap(), ["native-a", "native-b"]);
        assert_eq!(
            discovery
                .source("native-a", "cn-east-1")
                .await
                .unwrap()
                .as_deref(),
            Some("https://source.example.test")
        );
        assert!(
            discovery
                .source("native-b", "cn-east-1")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            discovery
                .source("native-a", "cn-east-1")
                .await
                .unwrap()
                .is_none()
        );
        assert!(discovery.source("native-a", "cn-east-1").await.is_err());
        assert_eq!(
            discovery.buckets().await.unwrap_err().kind(),
            StorageErrorKind::Transient
        );
        let requests = peer.requests();
        assert_eq!(requests[0].path, "/buckets");
        assert_eq!(requests[0].query["shared"], "rd");
        assert!(requests[0].headers["authorization"].starts_with("Qiniu AK:"));
        let query = &requests[1].query;
        assert_eq!(query["ak"], "AK");
        assert_eq!(query["bucket"], "native-a");
        assert!(
            discovery
                .contains(
                    "native-a",
                    "https://user:password@source.test",
                    "existing/prefix",
                    "probe",
                    b"nonce"
                )
                .await
                .is_err()
        );
        peer.stop().await;
    }

    #[tokio::test]
    async fn native_identity_probe_requires_exact_bounded_content_and_redacts_errors() {
        use crate::storage::test_support::{HttpFixture, HttpReply};
        let peer = HttpFixture::new([
            HttpReply::text(200, "nonce"),
            HttpReply::text(200, "wrong"),
            HttpReply::text(200, "non"),
            HttpReply::text(200, "nonce-extra"),
            HttpReply::text(404, "absent"),
        ])
        .await;
        let mut driver = discovery_driver(
            "native".into(),
            "existing/prefix".into(),
            "https://source.example.test".into(),
            credentials(),
        )
        .unwrap();
        driver.config.download_origin = peer.base.clone();
        for expected in [true, false, false, false] {
            assert_eq!(
                driver.read_upgrade_probe("probe", b"nonce").await.unwrap(),
                expected
            );
        }
        assert_eq!(
            driver
                .read_upgrade_probe("probe", b"nonce")
                .await
                .unwrap_err()
                .kind(),
            StorageErrorKind::NotFound
        );
        for request in peer.requests() {
            assert!(request.path == "/existing/prefix/probe");
            assert!(request.query["token"].starts_with("AK:"));
            assert!(request.query.contains_key("e"));
            assert_eq!(request.headers["accept-encoding"], "identity");
        }
        peer.stop().await;
    }

    #[tokio::test]
    async fn official_upgrade_rejects_untrusted_legacy_endpoints_before_network() {
        for endpoint in [
            "bad-url",
            "http://s3.cn-east-1.qiniucs.com",
            "https://evil.test",
            "https://s3.cn-east-1.qiniucs.com:8443",
            "https://user:password@s3.cn-east-1.qiniucs.com",
            "https://s3.cn-east-1.qiniucs.com/path",
            "https://s3.cn-east-1.qiniucs.com?query=1",
            "https://s3.cn-east-1.qiniucs.com/#fragment",
        ] {
            let mut old = old();
            old.endpoint = endpoint.into();
            assert_eq!(
                OfficialResolver
                    .resolve(&old, credentials())
                    .await
                    .err()
                    .unwrap()
                    .kind(),
                StorageErrorKind::Misconfigured
            );
        }
    }

    #[tokio::test]
    async fn legacy_source_factory_validates_both_official_addressing_forms_without_network() {
        for endpoint in [
            "https://s3.cn-east-1.qiniucs.com",
            "https://different-s3-alias.s3.cn-east-1.qiniucs.com",
        ] {
            let mut old = old();
            old.endpoint = endpoint.into();
            let source = legacy_source_driver(&old, &credentials()).unwrap();
            assert!(source.supports_efficient_range());
        }
    }

    #[tokio::test]
    async fn dropped_probe_guard_cleans_owned_keys_even_after_cancellation_or_failure() {
        for fail in [false, true] {
            let source = Arc::new(Source {
                fail_delete: fail,
                ..Default::default()
            });
            source
                .objects
                .lock()
                .unwrap()
                .insert("owned".into(), b"nonce".to_vec());
            drop(ProbeCleanup {
                driver: source.clone(),
                key: Some("owned".into()),
            });
            tokio::time::timeout(Duration::from_secs(2), async {
                while source.deletes.load(Ordering::SeqCst) == 0 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(source.objects.lock().unwrap().contains_key("owned"), fail);
        }
    }

    #[test]
    fn dropping_probe_guard_without_runtime_does_not_panic() {
        let source = Arc::new(Source::default());
        drop(ProbeCleanup {
            driver: source.clone(),
            key: Some("owned".into()),
        });
        assert_eq!(source.deletes.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn fixed_go_sdk_region_contract_exposes_default_source_and_s3_region() {
        let query:Query=serde_json::from_value(serde_json::json!({"hosts":[{"io_src":{"domains":["bucket.kodo-cn-east-1.qiniucs.com"]},"s3":{"region_alias":"cn-east-1"}}]})).unwrap();
        assert_eq!(
            query.hosts[0].io_src.domains[0],
            "bucket.kodo-cn-east-1.qiniucs.com"
        );
        assert_eq!(query.hosts[0].s3.region_alias, "cn-east-1");
    }
}
