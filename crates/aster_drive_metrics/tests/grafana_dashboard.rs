//! Keep the shipped dashboard in sync with the actual registered metric schema.
#![cfg(feature = "prometheus")]

use std::collections::{BTreeMap, BTreeSet};

use aster_forge_metrics::{MetricKind, MetricsProvider, MetricsResource};
use serde_json::Value;

fn dashboard() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/public/grafana/asterdrive-overview.json"
    ))
    .unwrap()
}

fn expressions(dashboard: &Value) -> Vec<(&str, &str)> {
    dashboard["panels"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|panel| {
            panel["targets"]
                .as_array()
                .into_iter()
                .flatten()
                .map(move |target| {
                    (
                        panel["title"].as_str().unwrap(),
                        target["expr"].as_str().unwrap(),
                    )
                })
        })
        .collect()
}

// Every shipped query uses explicit selectors. This small extractor checks their
// names and label contract; it deliberately does not claim to parse PromQL.
fn selectors(expression: &str) -> Vec<(&str, Vec<&str>)> {
    expression
        .match_indices('{')
        .map(|(index, _)| {
            let prefix = &expression[..index];
            let name = prefix
                .rsplit(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                .next()
                .unwrap();
            let end = expression[index + 1..].find('}').unwrap() + index + 1;
            let labels = expression[index + 1..end]
                .split(',')
                .map(|matcher| matcher.trim().split(['=', '!']).next().unwrap().trim())
                .collect();
            (name, labels)
        })
        .collect()
}

fn aggregate_labels(expression: &str) -> BTreeSet<&str> {
    expression
        .match_indices(" by (")
        .filter_map(|(index, _)| {
            let start = index + 5;
            let end = expression[start..].find(')')? + start;
            Some(expression[start..end].split(',').map(str::trim))
        })
        .flatten()
        .collect()
}

fn legend_labels(legend: &str) -> Vec<&str> {
    legend
        .match_indices("{{")
        .filter_map(|(index, _)| {
            let start = index + 2;
            let end = legend[start..].find("}}")? + start;
            Some(legend[start..end].trim())
        })
        .collect()
}

#[test]
fn dashboard_selectors_use_registered_names_and_supported_labels() {
    let provider = MetricsProvider::builder()
        .with_prometheus()
        .unwrap()
        .build()
        .unwrap();
    let _recorder = aster_drive_metrics::create_metrics_recorder(provider.clone()).unwrap();
    let mut schema = BTreeMap::new();
    for descriptor in provider.descriptors() {
        let name = descriptor.prometheus_name();
        let mut labels: BTreeSet<String> = descriptor
            .labels
            .iter()
            .map(|label| label.replace('.', "_"))
            .collect();
        schema.insert(name.clone(), labels.clone());
        if descriptor.kind == MetricKind::Histogram {
            schema.insert(format!("{name}_count"), labels.clone());
            schema.insert(format!("{name}_sum"), labels.clone());
            labels.insert("le".to_string());
            schema.insert(format!("{name}_bucket"), labels);
        }
    }
    let common_labels = BTreeSet::from([
        "job",
        "instance",
        "service_name",
        "deployment_environment_name",
        "aster_role",
        "service_instance_id",
        "aster_metrics_schema_version",
    ]);
    let dashboard = dashboard();
    for (title, expression) in expressions(&dashboard) {
        assert!(!selectors(expression).is_empty(), "{title}");
        for (name, labels) in selectors(expression) {
            if name == "up" {
                assert!(
                    labels
                        .iter()
                        .all(|label| ["job", "instance"].contains(label))
                );
                continue;
            }
            // Exporter self-observation is absent in a pull-only binary. The
            // all-features run checks these selectors against real descriptors.
            #[cfg(not(feature = "otlp"))]
            if name.starts_with("aster_telemetry_") {
                continue;
            }
            let supported = schema
                .get(name)
                .unwrap_or_else(|| panic!("{title} queries unregistered metric {name}"));
            for label in labels {
                assert!(
                    supported.contains(label) || common_labels.contains(label),
                    "{title}: {name} has no {label} label"
                );
            }
            for filter in [
                "job=~\"$job\"",
                "instance=~\"$instance\"",
                "service_name=\"asterdrive\"",
                "deployment_environment_name=~\"$environment\"",
                "aster_role=~\"$role\"",
                "service_instance_id=~\"$runtime_instance\"",
            ] {
                assert!(expression.contains(filter), "{title} missing {filter}");
            }
        }
    }
    let schema_label_panels = BTreeSet::from([
        "Request Rate By Route",
        "HTTP Status Mix",
        "Slow Routes P95",
        "DB Queries",
        "DB Query Latency P95",
        "DB Errors",
        "DB QPS",
        "DB Average Latency",
    ]);
    for panel in dashboard["panels"].as_array().unwrap() {
        if !schema_label_panels.contains(panel["title"].as_str().unwrap()) {
            continue;
        }
        for target in panel["targets"].as_array().into_iter().flatten() {
            let legend = target["legendFormat"].as_str().unwrap_or_default();
            let labels = legend_labels(legend);
            if labels.is_empty() {
                continue;
            }
            let aggregates = aggregate_labels(target["expr"].as_str().unwrap());
            for label in labels {
                assert!(
                    aggregates.contains(label) || common_labels.contains(label) || label == "le",
                    "{} legend references {label}, absent from its target aggregation",
                    panel["title"]
                );
            }
        }
    }
}

#[tokio::test]
async fn dashboard_resource_filters_exist_in_actual_prometheus_output() {
    let provider = MetricsProvider::builder()
        .with_resource(MetricsResource {
            service_name: "asterdrive".into(),
            service_version: "test".into(),
            environment: "dashboard-test".into(),
            role: "primary".into(),
            instance_id: "test-boot-id".into(),
        })
        .unwrap()
        .with_profile("aster", "aster-metrics/v1")
        .unwrap()
        .with_prometheus()
        .unwrap()
        .build()
        .unwrap();
    let recorder = aster_drive_metrics::create_metrics_recorder(provider.clone()).unwrap();
    recorder.record_file_upload("stream", "success");
    let body = provider
        .export_prometheus(std::time::Duration::from_secs(1))
        .await
        .unwrap();
    let sample = body
        .lines()
        .find(|line| line.starts_with("aster_drive_file_uploads_total{"))
        .unwrap();
    for label in [
        "service_name=\"asterdrive\"",
        "deployment_environment_name=\"dashboard-test\"",
        "aster_role=\"primary\"",
        "service_instance_id=\"test-boot-id\"",
    ] {
        assert!(sample.contains(label), "missing {label}: {sample}");
    }
}

#[test]
fn dashboard_variables_and_layout_are_well_formed() {
    let dashboard = dashboard();
    let variables: BTreeSet<_> = dashboard["templating"]["list"]
        .as_array()
        .unwrap()
        .iter()
        .map(|variable| variable["name"].as_str().unwrap())
        .chain(["__rate_interval"])
        .collect();
    let mut ids = BTreeSet::new();
    let mut rectangles = Vec::new();
    for panel in dashboard["panels"].as_array().unwrap() {
        assert!(ids.insert(panel["id"].as_u64().unwrap()));
        let grid = &panel["gridPos"];
        let (x, y, w, h) = (
            grid["x"].as_u64().unwrap(),
            grid["y"].as_u64().unwrap(),
            grid["w"].as_u64().unwrap(),
            grid["h"].as_u64().unwrap(),
        );
        assert!(w > 0 && h > 0 && x + w <= 24);
        for &(other_x, other_y, other_w, other_h) in &rectangles {
            assert!(
                x + w <= other_x
                    || other_x + other_w <= x
                    || y + h <= other_y
                    || other_y + other_h <= y,
                "panel {} overlaps another panel",
                panel["title"]
            );
        }
        rectangles.push((x, y, w, h));
    }
    for (title, expression) in expressions(&dashboard) {
        for (index, _) in expression.match_indices('$') {
            let suffix = &expression[index + 1..];
            let variable = suffix
                .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                .next()
                .unwrap();
            assert!(variables.contains(variable), "{title}: unknown ${variable}");
        }
    }
}

#[test]
fn dashboard_preserves_unknown_capacity_and_snapshot_failure_boundaries() {
    let dashboard = dashboard();
    assert_eq!(
        dashboard["refresh"].as_str(),
        Some("10s"),
        "dashboard refresh must stay within the 164-second application budget"
    );
    for (title, expression) in expressions(&dashboard) {
        if selectors(expression)
            .iter()
            .any(|(metric, _)| *metric == "aster_drive_storage_capacity_bytes")
        {
            assert!(expression.contains("aster_drive_storage_capacity_known{"));
            assert!(expression.contains(" == 1"));
            assert!(
                !expression.contains("sum("),
                "{title}: capacity is not additive"
            );
            assert!(
                !expression.contains(" / "),
                "{title}: minima are independent"
            );
        }
        if selectors(expression).iter().any(|(metric, _)| {
            [
                "aster_drive_upload_sessions_active",
                "aster_drive_upload_session_oldest_age_seconds",
                "aster_drive_blob_cleanup_pending",
                "aster_drive_blob_cleanup_oldest_age_seconds",
                "aster_drive_remote_nodes",
                "aster_task_pending",
            ]
            .contains(metric)
        }) {
            assert!(expression.contains("aster_drive_observability_snapshot_valid{"));
            assert!(
                expression
                    .contains("aster_drive_observability_snapshot_last_success_timestamp_seconds{")
            );
            assert!(expression.contains(" < 180"));
            assert!(!expression.contains("sum("), "{title}: shared DB snapshot");
        }
        assert!(
            !expression.contains("or vector(0)"),
            "{title}: no data is not zero"
        );
    }
}

#[test]
fn all_jobs_expand_only_the_asterdrive_candidates_for_target_health() {
    let dashboard = dashboard();
    let job = dashboard["templating"]["list"]
        .as_array()
        .unwrap()
        .iter()
        .find(|variable| variable["name"] == "job")
        .unwrap();
    assert_eq!(job["includeAll"], true);
    assert!(
        job.get("allValue").is_none(),
        "custom wildcard bypasses the AsterDrive candidate query"
    );
    assert!(
        job["query"]["query"]
            .as_str()
            .unwrap()
            .contains("service_name=\"asterdrive\"")
    );
    let (_, target_health) = expressions(&dashboard)
        .into_iter()
        .find(|(title, _)| *title == "Target Health")
        .unwrap();
    assert!(target_health.contains("job=~\"$job\""));
}

#[test]
#[cfg(feature = "otlp")]
fn published_catalog_matches_actual_schema() {
    let catalog: Value = serde_json::from_str(include_str!(
        "../../../docs/public/metrics/asterdrive-v1.json"
    ))
    .unwrap();
    let provider = MetricsProvider::builder()
        .with_prometheus()
        .unwrap()
        .build()
        .unwrap();
    let _recorder = aster_drive_metrics::create_metrics_recorder(provider.clone()).unwrap();
    let actual = provider.descriptors();
    assert_eq!(catalog["schema"], "aster-metrics/v1");
    assert_eq!(
        catalog["instruments"].as_array().unwrap().len(),
        actual.len()
    );
    for descriptor in actual {
        let item = catalog["instruments"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["prometheus"] == descriptor.prometheus_name())
            .unwrap();
        assert_eq!(item["unit"], descriptor.unit);
        assert_eq!(item["attributes"], serde_json::json!(descriptor.labels));
        assert_eq!(
            item["integer_attributes"],
            serde_json::json!(descriptor.integer_labels)
        );
        assert_eq!(item["buckets"], serde_json::json!(descriptor.buckets));
    }
}
