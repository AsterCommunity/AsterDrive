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

#[test]
fn odd_multi_range_preserves_configured_total() {
    benchmark::contract_odd_multi_range_accounting();
}

#[tokio::test]
async fn failed_benchmark_cleans_provider_fixture() {
    benchmark::contract_failed_benchmark_cleans_fixture().await;
}
