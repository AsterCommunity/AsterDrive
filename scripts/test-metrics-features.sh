#!/usr/bin/env bash
# Extra Cargo arguments (for example --config /path/to/local-forge.toml) apply
# consistently to every feature combination.
set -euo pipefail
for features in server server,metrics server,otlp-metrics server,metrics,otlp-metrics; do
    printf 'Drive observability features: %s\n' "$features"
    cargo "$@" check --locked --no-default-features --features "$features" --lib --bin aster_drive
    cargo "$@" test --locked --no-default-features --features "$features" --lib config::metrics::tests
    cargo "$@" test --locked --no-default-features --features "$features" --lib metrics_endpoint_requires_both_compiled_capability_and_explicit_enable
    if [[ "$features" == *metrics* ]]; then
        cargo "$@" test --locked --no-default-features --features "$features" --lib api::middleware::metrics_auth
    fi
done
otlp_tree=$(cargo "$@" tree --locked --no-default-features --features server,otlp-metrics -e normal)
if rg -q '(prometheus|opentelemetry-prometheus) v' <<< "$otlp_tree"; then
    printf 'Drive OTLP-only builds must not include a Prometheus exporter\n' >&2
    exit 1
fi
