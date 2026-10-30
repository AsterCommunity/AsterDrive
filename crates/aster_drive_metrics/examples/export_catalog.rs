//! Generate the released schema from the actual descriptors, never a handwritten list.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = aster_forge_metrics::MetricsProvider::builder().with_resource(
        aster_forge_metrics::MetricsResource {
            service_name: "asterdrive".into(),
            service_version: env!("CARGO_PKG_VERSION").into(),
            environment: "catalog".into(),
            role: "catalog".into(),
            instance_id: "catalog".into(),
        },
    )?;
    let provider = provider
        .with_profile("aster", "aster-metrics/v1")?
        .with_prometheus()?
        .build()?;
    let _recorder = aster_drive_metrics::create_metrics_recorder(provider.clone())?;
    let instruments:Vec<_>=provider.descriptors().iter().map(|d|serde_json::json!({
        "name":if d.subsystem.is_empty(){d.name.to_string()}else{format!("{}.{}",d.subsystem,d.name)},
        "prometheus":d.prometheus_name(),
        "kind":format!("{:?}",d.kind),
        "unit":d.unit,
        "attributes":d.labels,
        "buckets":d.buckets,
        "description":d.help
    })).collect();
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"schema":"aster-metrics/v1","semantic_conventions":"1.44.0","instruments":instruments})
        )?
    );
    Ok(())
}
