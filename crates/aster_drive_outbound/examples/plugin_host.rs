//! Host-side example; the guest receives neither PluginTransport nor a client/builder.
use aster_drive_outbound::{NetworkGrant, PluginHttpRequest, PluginLimits, PluginTransport};
use tokio_util::sync::CancellationToken;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // First check the manifest's network:https://hooks.example.com capability and audit it.
    // Host-owned credentials may be inserted only for this approved origin; never default headers.
    let grant = NetworkGrant::approved_origin("https://hooks.example.com")?;
    let invocation = CancellationToken::new();
    let transport = PluginTransport::new(grant, PluginLimits::default(), invocation.clone())?;
    if let Some(url) = std::env::args().nth(1) {
        let response = transport
            .execute(PluginHttpRequest {
                method: reqwest::Method::POST,
                url,
                headers: Default::default(),
                body: b"{}".to_vec(),
            })
            .await?;
        println!("{}", response.status);
    }
    invocation.cancel();
    Ok(())
}
