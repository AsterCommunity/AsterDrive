#[expect(
    dead_code,
    reason = "the contract test includes the complete benchmark module but exercises only its accounting helpers"
)]
#[path = "../benches/webdav_provider_range.rs"]
mod benchmark;

#[tokio::test]
async fn multi_range_accounting_is_deterministic() {
    benchmark::contract_multi_range_accounting().await;
}

#[tokio::test]
async fn fallback_read_accounting_is_deterministic() {
    benchmark::contract_fallback_read_accounting().await;
}
