use std::future::Future;
use std::time::Duration;

use aster_forge_runtime::{RuntimeLeaseAcquire, RuntimeLeaseClaim, RuntimeLeaseStore};
use chrono::{DateTime, Utc};
use sea_orm::{EntityTrait, QueryFilter};

use crate::config::DeploymentConfig;
use crate::db::repository::remote_tunnel_owner_repo;
use crate::entities::remote_tunnel_owner;
use crate::errors::{AsterError, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};

pub const REMOTE_TUNNEL_OWNER_LEASE_TTL: Duration = Duration::from_secs(45);
pub const REMOTE_TUNNEL_OWNER_RENEW_INTERVAL: Duration = Duration::from_secs(15);
const REMOTE_TUNNEL_OWNER_LEASE_PREFIX: &str = "aster_drive.remote_tunnel";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTunnelOwnerLease {
    pub remote_node_id: i64,
    pub runtime_id: String,
    pub internal_endpoint: String,
    pub fencing_token: String,
    pub lease_expires_at: DateTime<Utc>,
}

impl From<remote_tunnel_owner::Model> for RemoteTunnelOwnerLease {
    fn from(value: remote_tunnel_owner::Model) -> Self {
        Self {
            remote_node_id: value.remote_node_id,
            runtime_id: value.runtime_id,
            internal_endpoint: value.internal_endpoint,
            fencing_token: value.fencing_token,
            lease_expires_at: value.lease_expires_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteTunnelOwnerClaim {
    Owned(RemoteTunnelOwnerLease),
    Standby(Option<RemoteTunnelOwnerLease>),
}

#[derive(Clone)]
pub struct RemoteTunnelOwnerDirectory {
    db: sea_orm::DatabaseConnection,
    runtime_id: String,
    internal_endpoint: String,
    fencing_token: String,
    proxy_secret: String,
}

impl RemoteTunnelOwnerDirectory {
    pub fn from_deployment(
        db: sea_orm::DatabaseConnection,
        deployment: &DeploymentConfig,
        runtime_id: impl Into<String>,
    ) -> Result<Option<Self>> {
        if !deployment.internal_proxy_enabled() {
            return Ok(None);
        }

        let mut endpoint =
            url::Url::parse(deployment.internal_endpoint.trim()).map_err(|error| {
                AsterError::config_error(format!(
                    "parse deployment.internal_endpoint for tunnel proxy: {error}"
                ))
            })?;
        endpoint.set_query(None);
        endpoint.set_fragment(None);
        let normalized_endpoint = endpoint.as_str().trim_end_matches('/').to_string();
        let runtime_id = runtime_id.into();
        if runtime_id.trim().is_empty() {
            return Err(AsterError::config_error(
                "reverse tunnel owner runtime_id must not be empty",
            ));
        }

        Ok(Some(Self {
            db,
            runtime_id,
            internal_endpoint: normalized_endpoint,
            fencing_token: aster_forge_utils::id::new_uuid(),
            proxy_secret: deployment.internal_proxy_secret.clone(),
        }))
    }

    pub fn runtime_id(&self) -> &str {
        &self.runtime_id
    }

    pub fn internal_endpoint(&self) -> &str {
        &self.internal_endpoint
    }

    pub fn fencing_token(&self) -> &str {
        &self.fencing_token
    }

    pub fn proxy_secret(&self) -> &str {
        &self.proxy_secret
    }

    pub fn is_local_owner(&self, owner: &RemoteTunnelOwnerLease) -> bool {
        owner.runtime_id == self.runtime_id && owner.fencing_token == self.fencing_token
    }

    pub async fn try_claim(&self, remote_node_id: i64) -> Result<RemoteTunnelOwnerClaim> {
        let now = Utc::now();
        let expires_at = lease_expires_at(now);
        let lease_id = owner_lease_id(remote_node_id);
        let store = aster_forge_db::RuntimeLeaseDbStore::new(self.db.clone());
        let claim = RuntimeLeaseClaim {
            lease_id: &lease_id,
            owner_id: &self.fencing_token,
            now,
            expires_at,
        };

        match store.try_acquire(claim).await.map_err(AsterError::from)? {
            RuntimeLeaseAcquire::Acquired => {
                let owner = self.persist_owner(remote_node_id, expires_at).await?;
                Ok(RemoteTunnelOwnerClaim::Owned(owner))
            }
            RuntimeLeaseAcquire::Standby { .. } => Ok(RemoteTunnelOwnerClaim::Standby(
                self.current_owner(remote_node_id).await?,
            )),
        }
    }

    pub async fn renew(&self, remote_node_id: i64) -> Result<bool> {
        let now = Utc::now();
        let expires_at = lease_expires_at(now);
        let store = aster_forge_db::RuntimeLeaseDbStore::new(self.db.clone());
        let renewed = store
            .renew(
                &owner_lease_id(remote_node_id),
                &self.fencing_token,
                now,
                expires_at,
            )
            .await
            .map_err(AsterError::from)?;
        if !renewed {
            return Ok(false);
        }

        self.persist_owner(remote_node_id, expires_at).await?;
        Ok(true)
    }

    pub async fn release(&self, remote_node_id: i64) -> Result<()> {
        let store = aster_forge_db::RuntimeLeaseDbStore::new(self.db.clone());
        store
            .release(&owner_lease_id(remote_node_id), &self.fencing_token)
            .await
            .map_err(AsterError::from)?;
        remote_tunnel_owner_repo::delete_if_fencing_token(
            &self.db,
            remote_node_id,
            &self.fencing_token,
        )
        .await?;
        Ok(())
    }

    pub async fn current_owner(
        &self,
        remote_node_id: i64,
    ) -> Result<Option<RemoteTunnelOwnerLease>> {
        use aster_forge_db::runtime_lease;
        use sea_orm::ColumnTrait;

        let now = Utc::now();
        let lease = runtime_lease::Entity::find()
            .filter(runtime_lease::Column::LeaseId.eq(owner_lease_id(remote_node_id)))
            .one(&self.db)
            .await
            .map_err(AsterError::from)?;
        let Some(lease) = lease.filter(|lease| lease.expires_at > now) else {
            return Ok(None);
        };
        let owner = remote_tunnel_owner_repo::find_by_remote_node_id(&self.db, remote_node_id)
            .await?
            .filter(|owner| {
                owner.fencing_token == lease.owner_id
                    && owner.lease_expires_at > now
                    && !owner.internal_endpoint.trim().is_empty()
            });
        Ok(owner.map(RemoteTunnelOwnerLease::from))
    }

    pub async fn verify_local_fencing(
        &self,
        remote_node_id: i64,
        fencing_token: &str,
    ) -> Result<bool> {
        Ok(self
            .current_owner(remote_node_id)
            .await?
            .is_some_and(|owner| {
                self.is_local_owner(&owner) && owner.fencing_token == fencing_token
            }))
    }

    pub async fn run_while_owned<T>(
        &self,
        remote_node_id: i64,
        operation: impl Future<Output = Result<T>>,
    ) -> Result<T> {
        if !self.renew(remote_node_id).await? {
            return Err(owner_fenced_error(remote_node_id));
        }

        let mut renewal = tokio::time::interval(REMOTE_TUNNEL_OWNER_RENEW_INTERVAL);
        renewal.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        renewal.tick().await;
        tokio::pin!(operation);

        loop {
            tokio::select! {
                result = &mut operation => return result,
                _ = renewal.tick() => {
                    if !self.renew(remote_node_id).await? {
                        return Err(owner_fenced_error(remote_node_id));
                    }
                }
            }
        }
    }

    async fn persist_owner(
        &self,
        remote_node_id: i64,
        expires_at: DateTime<Utc>,
    ) -> Result<RemoteTunnelOwnerLease> {
        let model = remote_tunnel_owner_repo::upsert(
            &self.db,
            remote_tunnel_owner::Model {
                remote_node_id,
                runtime_id: self.runtime_id.clone(),
                internal_endpoint: self.internal_endpoint.clone(),
                fencing_token: self.fencing_token.clone(),
                lease_expires_at: expires_at,
                updated_at: Utc::now(),
            },
        )
        .await?;
        Ok(model.into())
    }
}

fn owner_fenced_error(remote_node_id: i64) -> AsterError {
    storage_driver_error(
        StorageErrorKind::Transient,
        format!("reverse tunnel remote node #{remote_node_id} owner lease was fenced"),
    )
}

fn owner_lease_id(remote_node_id: i64) -> String {
    format!("{REMOTE_TUNNEL_OWNER_LEASE_PREFIX}.{remote_node_id}")
}

fn lease_expires_at(now: DateTime<Utc>) -> DateTime<Utc> {
    chrono::Duration::from_std(REMOTE_TUNNEL_OWNER_LEASE_TTL)
        .ok()
        .and_then(|ttl| now.checked_add_signed(ttl))
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, DeploymentProfile};
    use crate::entities::managed_follower;
    use migration::Migrator;
    use sea_orm::{ActiveModelTrait, Set};

    async fn setup() -> (sea_orm::DatabaseConnection, Config) {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics::NoopMetrics::arc(),
        )
        .await
        .expect("owner directory database should connect");
        Migrator::up(&db, None)
            .await
            .expect("owner directory migrations should run");
        managed_follower::ActiveModel {
            id: Set(7),
            name: Set("follower".to_string()),
            base_url: Set(String::new()),
            access_key: Set("access".to_string()),
            secret_key: Set("secret".to_string()),
            is_enabled: Set(true),
            transport_mode: Set(crate::types::RemoteNodeTransportMode::ReverseTunnel),
            last_capabilities: Set("{}".to_string()),
            last_error: Set(String::new()),
            last_checked_at: Set(None),
            tunnel_last_error: Set(String::new()),
            tunnel_last_seen_at: Set(None),
            created_at: Set(Utc::now()),
            updated_at: Set(Utc::now()),
        }
        .insert(&db)
        .await
        .expect("managed follower should insert");

        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;
        config.deployment.internal_endpoint = "http://primary-a:3000/".to_string();
        config.deployment.internal_proxy_secret =
            "cluster-secret-for-tests-at-least-32-bytes".to_string();
        (db, config)
    }

    #[tokio::test]
    async fn owner_claim_persists_structured_directory_and_renews() {
        let (db, config) = setup().await;
        let directory =
            RemoteTunnelOwnerDirectory::from_deployment(db, &config.deployment, "runtime-a")
                .unwrap()
                .unwrap();

        let RemoteTunnelOwnerClaim::Owned(owner) = directory.try_claim(7).await.unwrap() else {
            panic!("first claimant should own the tunnel");
        };
        assert_eq!(owner.runtime_id, "runtime-a");
        assert_eq!(owner.internal_endpoint, "http://primary-a:3000");
        assert!(
            directory
                .verify_local_fencing(7, &owner.fencing_token)
                .await
                .unwrap()
        );
        assert!(directory.renew(7).await.unwrap());
    }

    #[tokio::test]
    async fn second_runtime_observes_owner_without_overwriting_it() {
        let (db, config) = setup().await;
        let first = RemoteTunnelOwnerDirectory::from_deployment(
            db.clone(),
            &config.deployment,
            "runtime-a",
        )
        .unwrap()
        .unwrap();
        let second =
            RemoteTunnelOwnerDirectory::from_deployment(db, &config.deployment, "runtime-b")
                .unwrap()
                .unwrap();
        first.try_claim(7).await.unwrap();

        let RemoteTunnelOwnerClaim::Standby(Some(owner)) = second.try_claim(7).await.unwrap()
        else {
            panic!("second runtime should observe the active owner");
        };
        assert_eq!(owner.runtime_id, "runtime-a");
        assert!(
            !second
                .verify_local_fencing(7, &owner.fencing_token)
                .await
                .unwrap()
        );
        assert!(!second.renew(7).await.unwrap());
    }

    #[tokio::test]
    async fn release_is_fenced_and_allows_immediate_takeover() {
        let (db, config) = setup().await;
        let first = RemoteTunnelOwnerDirectory::from_deployment(
            db.clone(),
            &config.deployment,
            "runtime-a",
        )
        .unwrap()
        .unwrap();
        let second =
            RemoteTunnelOwnerDirectory::from_deployment(db, &config.deployment, "runtime-b")
                .unwrap()
                .unwrap();
        first.try_claim(7).await.unwrap();
        first.release(7).await.unwrap();

        assert!(matches!(
            second.try_claim(7).await.unwrap(),
            RemoteTunnelOwnerClaim::Owned(_)
        ));
        assert!(!first.renew(7).await.unwrap());
    }

    #[tokio::test]
    async fn owned_operation_completes_and_standby_is_fenced_before_running() {
        let (db, config) = setup().await;
        let owner = RemoteTunnelOwnerDirectory::from_deployment(
            db.clone(),
            &config.deployment,
            "runtime-a",
        )
        .unwrap()
        .unwrap();
        let standby =
            RemoteTunnelOwnerDirectory::from_deployment(db, &config.deployment, "runtime-b")
                .unwrap()
                .unwrap();
        owner.try_claim(7).await.unwrap();

        assert_eq!(
            owner
                .run_while_owned(7, async { Ok::<_, AsterError>("completed") })
                .await
                .unwrap(),
            "completed"
        );
        let ran = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ran_in_operation = ran.clone();
        let error = standby
            .run_while_owned(7, async move {
                ran_in_operation.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            })
            .await
            .expect_err("standby must be fenced before its operation starts");
        assert!(error.message().contains("owner lease was fenced"));
        assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn current_owner_rejects_directory_and_lease_fencing_mismatch() {
        let (db, config) = setup().await;
        let directory = RemoteTunnelOwnerDirectory::from_deployment(
            db.clone(),
            &config.deployment,
            "runtime-a",
        )
        .unwrap()
        .unwrap();
        directory.try_claim(7).await.unwrap();
        let owner = remote_tunnel_owner::Entity::find_by_id(7)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        let mut active: remote_tunnel_owner::ActiveModel = owner.into();
        active.fencing_token = Set("mismatched-token".to_string());
        active.update(&db).await.unwrap();

        assert!(directory.current_owner(7).await.unwrap().is_none());
        assert!(
            !directory
                .verify_local_fencing(7, directory.fencing_token())
                .await
                .unwrap()
        );
    }
}
