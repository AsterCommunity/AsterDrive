//! Scheduled archive coverage uses the product policy APIs, including credential encryption.

use aster_drive_model::entities::file_blob;
use sea_orm::{DatabaseConnection, EntityTrait};
use serde_json::json;
use testcontainers::{ContainerAsync, GenericImage, ImageExt, runners::AsyncRunner};

use super::scenarios::{Server, data};

const ACCESS: &str = "reliability522";
pub const SECRET: &str = "reliability522-minio-private-value";

pub struct Minio {
    _container: ContainerAsync<GenericImage>,
    client: aws_sdk_s3::Client,
    endpoint: String,
    bucket: String,
    policy_id: Option<i64>,
}

impl Minio {
    pub async fn start() -> Self {
        let container = GenericImage::new("asterdrive-reliability-minio", "07c3a429bfed")
            .with_exposed_port(testcontainers::core::IntoContainerPort::tcp(9000))
            .with_env_var("MINIO_ROOT_USER", ACCESS)
            .with_env_var("MINIO_ROOT_PASSWORD", SECRET)
            .with_cmd(["server", "/data"])
            .start()
            .await
            .expect("start MinIO fixture");
        let endpoint = format!(
            "http://127.0.0.1:{}",
            container.get_host_port_ipv4(9000).await.unwrap()
        );
        let client = aws_sdk_s3::Client::from_conf(
            aws_sdk_s3::Config::builder()
                .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
                .region(aws_sdk_s3::config::Region::new("us-east-1"))
                .credentials_provider(aws_credential_types::Credentials::new(
                    ACCESS, SECRET, None, None, "fixture",
                ))
                .endpoint_url(&endpoint)
                .force_path_style(true)
                .build(),
        );
        let bucket = format!("reliability-{}", uuid::Uuid::new_v4().simple());
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            loop {
                if client.create_bucket().bucket(&bucket).send().await.is_ok() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("MinIO bucket readiness timeout");
        Self {
            _container: container,
            client,
            endpoint,
            bucket,
            policy_id: None,
        }
    }

    pub async fn route_archives(
        &mut self,
        client: &aster_forge_http::HttpClient,
        server: &Server,
        token: &str,
    ) {
        let config = aster_drive_storage::ConnectorConfigEnvelope::new(
            aster_drive_storage::ConnectorId::declared("asterdrive.storage.s3"),
            1,
            json!({"endpoint": self.endpoint, "bucket": self.bucket, "base_path":"",
                "s3_path_style":true,"s3_region":"us-east-1",
                "object_storage_upload_strategy":"relay_stream",
                "object_storage_download_strategy":"relay_stream"}),
        );
        let policy = data(
            client
                .post(format!("{}/api/v1/admin/policies", server.url))
                .bearer_auth(token)
                .json(&json!({"name":"Reliability MinIO", "connection":{
                "connector_config":config,"behavior":{},"credential":{"mode":"static","values":{
                    "s3_access_key_id":ACCESS,"s3_secret_access_key":SECRET}}
            },"max_file_size":0,"chunk_size":2048})),
        )
        .await;
        let policy_id = policy["id"].as_i64().unwrap();
        self.policy_id = Some(policy_id);
        let groups = data(
            client
                .get(format!("{}/api/v1/admin/policy-groups", server.url))
                .bearer_auth(token),
        )
        .await;
        let group = groups["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["is_default"] == true)
            .unwrap();
        // Only archive results change placement; the offset-staging source remains local.
        let mut rules = group["rules"].as_array().unwrap().clone();
        for rule in &mut rules {
            rule["priority"] = json!(rule["priority"].as_i64().unwrap() + 1);
        }
        rules.insert(
            0,
            json!({"name":"archive destination","priority":1,"matcher":{"extensions":["zip"]},
            "targets":[{"policy_id":policy_id,"weight":1}]}),
        );
        for rule in &mut rules {
            rule.as_object_mut().unwrap().remove("id");
            for target in rule["targets"].as_array_mut().unwrap() {
                target.as_object_mut().unwrap().remove("id");
                target.as_object_mut().unwrap().remove("policy");
            }
        }
        data(
            client
                .patch(format!(
                    "{}/api/v1/admin/policy-groups/{}",
                    server.url, group["id"]
                ))
                .bearer_auth(token)
                .json(&json!({"rules":rules})),
        )
        .await;
    }

    pub async fn assert_objects_tracked(&self, db: &DatabaseConnection) {
        let blobs = file_blob::Entity::find().all(db).await.unwrap();
        let objects = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .send()
            .await
            .expect("list MinIO fixture objects");
        let mut live = 0;
        for object in objects.contents() {
            let key = object.key().unwrap();
            let blob = blobs
                .iter()
                .find(|b| {
                    Some(b.policy_id) == self.policy_id && b.storage_path.as_deref() == Some(key)
                })
                .expect("provider object must have a live blob or durable orphan-cleanup record");
            assert_eq!(object.size(), Some(blob.size));
            if blob.ref_count > 0 {
                live += 1;
            } else {
                assert!(
                    blob.cleanup_pending_since.is_some(),
                    "abandoned attempt must be scheduled for cleanup"
                );
            }
        }
        assert_eq!(live, 1, "one durable archive object");
    }
}
