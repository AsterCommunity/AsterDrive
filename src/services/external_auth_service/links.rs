use chrono::Utc;

use crate::db::repository::{
    external_auth_email_verification_flow_repo, external_auth_identity_repo,
    external_auth_login_flow_repo, external_auth_provider_repo,
};
use crate::entities::{external_auth_identity, external_auth_provider};
use crate::errors::Result;
use crate::runtime::SharedRuntimeState;

use super::ExternalAuthLinkInfo;

pub async fn list_links(
    state: &impl SharedRuntimeState,
    user_id: i64,
) -> Result<Vec<ExternalAuthLinkInfo>> {
    let identities = external_auth_identity_repo::list_for_user(state.writer_db(), user_id).await?;
    let providers = external_auth_provider_repo::find_all(state.writer_db()).await?;
    let provider_lookup = providers
        .into_iter()
        .map(|provider| (provider.id, provider))
        .collect::<std::collections::HashMap<_, _>>();
    Ok(identities
        .into_iter()
        .filter_map(|identity| {
            let provider = provider_lookup.get(&identity.provider_id)?;
            Some(link_to_info(identity, provider))
        })
        .collect())
}

fn link_to_info(
    identity: external_auth_identity::Model,
    provider: &external_auth_provider::Model,
) -> ExternalAuthLinkInfo {
    ExternalAuthLinkInfo {
        id: identity.id,
        provider_id: identity.provider_id,
        provider_key: provider.key.clone(),
        provider_kind: provider.provider_kind,
        provider_display_name: provider.display_name.clone(),
        provider_icon_url: provider.icon_url.clone(),
        issuer: identity.identity_namespace,
        subject: identity.subject,
        email_snapshot: identity.email_snapshot,
        display_name_snapshot: identity.display_name_snapshot,
        created_at: identity.created_at,
        updated_at: identity.updated_at,
        last_login_at: identity.last_login_at,
    }
}

pub async fn delete_link(
    state: &impl SharedRuntimeState,
    user_id: i64,
    id: i64,
) -> Result<bool> {
    external_auth_identity_repo::delete_for_user(state.writer_db(), id, user_id).await
}

pub async fn cleanup_expired_flows(state: &impl SharedRuntimeState) -> Result<u64> {
    let now = Utc::now();
    let login_flows =
        external_auth_login_flow_repo::cleanup_expired(state.writer_db(), now).await?;
    let email_flows =
        external_auth_email_verification_flow_repo::cleanup_expired(state.writer_db(), now).await?;
    Ok(login_flows + email_flows)
}
