//! A provider owns credentials/signing/response interpretation, not pool defaults.
use aster_drive_outbound::{ClientConfig, Profile};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = ClientConfig::new(Profile::ProviderControl).build()?;
    // Hold this client in the driver. A clone shares its pool.
    if let Some(url) = std::env::args().nth(1) {
        let request = client.get(url).build()?;
        // Provider signing modifies this request before execute, without altering policy.
        let response = client.execute(request).await?;
        println!("{}", response.status());
    }
    Ok(())
}
