#!/usr/bin/env python3
"""Keep frontend and pinned deployment defaults on the Rust workspace release.

Requires Python 3.11+ and the manifest workflow's existing PyYAML dependency.
Floating stable tags, image digests and the Helm chart's own version are independent.
"""

import json
import sys
import tomllib
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]


def main():
    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())
    expected = workspace["workspace"]["package"]["version"]
    frontend = json.loads((ROOT / "frontend-panel/package.json").read_text())
    chart = yaml.safe_load((ROOT / "deploy/helm/asterdrive/Chart.yaml").read_text())
    values = yaml.safe_load((ROOT / "deploy/helm/asterdrive/values.yaml").read_text())
    production = yaml.safe_load(
        (ROOT / "deploy/kubernetes/overlays/production-example/kustomization.yaml").read_text()
    )
    image = next(
        item for item in production["images"]
        if item["name"] == "ghcr.io/astercommunity/asterdrive"
    )
    versions = {
        "frontend-panel/package.json: version": frontend["version"],
        "deploy/helm/asterdrive/Chart.yaml: appVersion": chart["appVersion"],
        "deploy/helm/asterdrive/values.yaml: image.tag": values["image"]["tag"],
        "deploy/kubernetes/overlays/production-example/kustomization.yaml: newTag": image["newTag"],
    }
    mismatches = [
        f"{field}: found {actual!r}, expected {expected!r} from Cargo.toml"
        for field, actual in versions.items() if actual != expected
    ]
    if mismatches:
        print("Release versions are out of sync:\n" + "\n".join(mismatches), file=sys.stderr)
        return 1
    print(f"Release versions match Cargo.toml: {expected} ({len(versions)} references)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
