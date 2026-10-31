#!/usr/bin/env python3
"""Exercise role scaling and rolling shutdown in an isolated Helm test namespace.

Run after installing this checkout's image with split.enabled=true. Requires an
explicit Kubernetes context and namespace; leaves API and worker at two replicas.
Business recovery, storage and cancellation are covered by multi_primary tests.
"""

import argparse
import contextlib
import json
import socket
import subprocess
import time
import urllib.error
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--context", required=True)
    parser.add_argument("--namespace", required=True)
    parser.add_argument("--release", default="asterdrive")
    args = parser.parse_args()
    base = ["kubectl", "--context", args.context, "-n", args.namespace]
    selector = f"app.kubernetes.io/instance={args.release}"

    def kubectl(*command):
        return subprocess.check_output([*base, *command], text=True)

    def objects(kind, role=None):
        labels = selector + (f",app.kubernetes.io/component={role}" if role else "")
        return json.loads(kubectl("get", kind, "-l", labels, "-o", "json"))["items"]

    def await_condition(description, predicate, seconds=180):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if predicate():
                print(description, flush=True)
                return
            time.sleep(0.25)
        raise AssertionError(f"timed out: {description}")

    workloads = objects("statefulsets")
    roles = {item["spec"]["template"]["metadata"]["labels"]["app.kubernetes.io/component"]: item for item in workloads}
    assert set(roles) == {"api", "worker"}, "install split.enabled=true in an isolated namespace"
    names = {role: item["metadata"]["name"] for role, item in roles.items()}
    assert not objects("hpa"), "disable HPA for this explicit scale test"
    services = [item for item in objects("services") if item["spec"].get("clusterIP") != "None" and item["spec"].get("selector", {}).get("app.kubernetes.io/component") == "api"]
    assert len(services) == 1, "business Service must select API only"
    business = services[0]["metadata"]["name"]

    def ready(role, count):
        pods = objects("pods", role)
        return len(pods) == count and all(
            not pod["metadata"].get("deletionTimestamp")
            and any(condition["type"] == "Ready" and condition["status"] == "True" for condition in pod["status"].get("conditions", []))
            for pod in pods
        )

    def identities(role):
        return {pod["metadata"]["name"]: (pod["metadata"]["uid"], [entry["restartCount"] for entry in pod["status"].get("containerStatuses", [])]) for pod in objects("pods", role)}

    def scale(role, count):
        kubectl("scale", f"statefulset/{names[role]}", f"--replicas={count}")
        await_condition(f"{role}: {count} Ready replicas", lambda: ready(role, count))

    @contextlib.contextmanager
    def forward(resource, remote_port=3000):
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        process = subprocess.Popen([*base, "port-forward", resource, f"{port}:{remote_port}"], stdout=subprocess.DEVNULL)
        url = f"http://127.0.0.1:{port}"
        try:
            await_condition(f"forward {resource}", lambda: status(url, "/health") == 200, 30)
            yield url
        finally:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()

    def status(url, path):
        try:
            with urllib.request.urlopen(url + path, timeout=2) as response:
                return response.status
        except urllib.error.HTTPError as error:
            return error.code
        except (urllib.error.URLError, TimeoutError, ConnectionError):
            return None

    for role in roles:
        scale(role, 2)
        pod = objects("pods", role)[0]
        container = pod["spec"]["containers"][0]
        assert container["args"] == ["serve", role]
        assert container["readinessProbe"]["httpGet"]["path"] == "/health/ready"
        assert pod["spec"]["terminationGracePeriodSeconds"] >= 35
        with forward("pod/" + pod["metadata"]["name"]) as url:
            assert status(url, "/health/ready") == 200
            if role == "worker":
                for path in ["/", "/api/v1/tasks", "/api/v1/auth/login", "/webdav"]:
                    assert status(url, path) == 404, path

    api_before = identities("api")
    with forward("service/" + business, services[0]["spec"]["ports"][0]["port"]) as url:
        scale("worker", 0)
        assert status(url, "/health/ready") == 200, "API must remain available without workers"
        scale("worker", 2)
        assert identities("api") == api_before, "worker scaling restarted API"
        worker_before = identities("worker")
        scale("api", 3)
        scale("api", 2)
        assert identities("worker") == worker_before, "API scaling restarted workers"

    for role, opposite in [("worker", "api"), ("api", "worker")]:
        unchanged = identities(opposite)
        previous = identities(role)
        kubectl("rollout", "restart", "statefulset/" + names[role])
        kubectl("rollout", "status", "statefulset/" + names[role], "--timeout=180s")
        await_condition(f"{role} rolling update adopted new pods", lambda: ready(role, 2) and all(previous.get(name, (None,))[0] != identity[0] for name, identity in identities(role).items()))
        assert identities(opposite) == unchanged, "rolling one role restarted its peer"

    slices = json.loads(kubectl("get", "endpointslices", "-l", f"kubernetes.io/service-name={business}", "-o", "json"))["items"]
    api_names = set(identities("api"))
    endpoints = [endpoint for item in slices for endpoint in item["endpoints"] if endpoint.get("conditions", {}).get("ready")]
    assert endpoints and all(endpoint["targetRef"]["name"] in api_names for endpoint in endpoints)
    print("role runtime smoke passed: probes, routing, worker zero, independent scaling and rolling updates", flush=True)


if __name__ == "__main__":
    main()
