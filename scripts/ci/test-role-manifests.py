#!/usr/bin/env python3
"""Render role deployments and assert routing, scaling and isolation contracts.

Requires PyYAML 6.0.3, Helm and kubectl. Does not contact a Kubernetes cluster.
"""
import subprocess
import unittest
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
CHART = "deploy/helm/asterdrive"


def helm(*args):
    result = subprocess.run(
        ["helm", "template", "asterdrive", CHART, *args],
        cwd=ROOT, capture_output=True, text=True, check=True,
    )
    return [doc for doc in yaml.safe_load_all(result.stdout) if doc]


def check_split(test, docs):
    workloads = [doc for doc in docs if doc["kind"] == "StatefulSet"]
    test.assertEqual(len(workloads), 2)
    roles = {}
    for workload in workloads:
        pod = workload["spec"]["template"]
        role = pod["metadata"]["labels"]["app.kubernetes.io/component"]
        roles[role] = workload
        test.assertEqual(pod["spec"]["containers"][0]["args"], ["serve", role])
        test.assertEqual(workload["spec"]["selector"]["matchLabels"]["app.kubernetes.io/component"], role)
        test.assertEqual(pod["spec"]["containers"][0]["readinessProbe"]["httpGet"]["path"], "/health/ready")
    test.assertEqual(set(roles), {"api", "worker"})
    business = next(doc for doc in docs if doc["kind"] == "Service" and doc["metadata"]["name"] == "asterdrive")
    test.assertEqual(business["spec"]["selector"]["app.kubernetes.io/component"], "api")
    worker_labels = roles["worker"]["spec"]["template"]["metadata"]["labels"]
    test.assertFalse(all(worker_labels.get(key) == value for key, value in business["spec"]["selector"].items()))
    return roles


class RoleManifests(unittest.TestCase):
    def test_default_remains_all_in_one(self):
        docs = helm()
        workloads = [doc for doc in docs if doc["kind"] == "StatefulSet"]
        self.assertEqual(len(workloads), 1)
        self.assertEqual(workloads[0]["spec"]["template"]["spec"]["containers"][0]["args"], ["serve", "all"])

    def test_independent_resources_and_zero_workers(self):
        roles = check_split(self, helm("--set", "split.enabled=true", "--set", "split.api.replicaCount=3", "--set", "split.worker.replicaCount=0"))
        self.assertEqual(roles["api"]["spec"]["replicas"], 3)
        self.assertEqual(roles["worker"]["spec"]["replicas"], 0)
        resources = lambda role: roles[role]["spec"]["template"]["spec"]["containers"][0]["resources"]
        self.assertNotEqual(resources("api"), resources("worker"))

    def test_hpa_owns_replica_count_and_targets_the_right_role(self):
        docs = helm("--set", "split.enabled=true", "--set", "split.worker.autoscaling.enabled=true")
        roles = check_split(self, docs)
        self.assertNotIn("replicas", roles["worker"]["spec"])
        hpa = next(doc for doc in docs if doc["kind"] == "HorizontalPodAutoscaler")
        self.assertEqual(hpa["spec"]["scaleTargetRef"]["name"], roles["worker"]["metadata"]["name"])

    def test_role_names_respect_kubernetes_length_limits(self):
        docs = helm("--set", "split.enabled=true", "--set-string", "fullnameOverride=" + "a" * 63)
        for doc in docs:
            self.assertLessEqual(len(doc["metadata"]["name"]), 63)

    def test_invalid_role_configuration_fails_rendering(self):
        for setting in ["split.worker.replicaCount=-1", "split.worker.terminationGracePeriodSeconds=1", "split.worker.autoscaling.minReplicas=20,split.worker.autoscaling.maxReplicas=10,split.worker.autoscaling.enabled=true", "split.api.unknown=true"]:
            with self.subTest(setting=setting), self.assertRaises(subprocess.CalledProcessError):
                helm("--set", "split.enabled=true", "--set", setting)

    def test_kustomize_split_routes_only_to_api(self):
        rendered = subprocess.check_output(["kubectl", "kustomize", "deploy/kubernetes/overlays/split"], cwd=ROOT, text=True)
        check_split(self, [doc for doc in yaml.safe_load_all(rendered) if doc])


if __name__ == "__main__":
    unittest.main()
