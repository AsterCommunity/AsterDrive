function snapshotMetric(data, name) {
	const metric = data.metrics[name];
	if (!metric || !metric.values) {
		return null;
	}

	const values = metric.values;
	return {
		count: values.count ?? null,
		rate: values.rate ?? null,
		avg: values.avg ?? null,
		min: values.min ?? null,
		med: values.med ?? null,
		p90: values["p(90)"] ?? null,
		p95: values["p(95)"] ?? null,
		p99: values["p(99)"] ?? null,
		p999: values["p(99.9)"] ?? null,
		max: values.max ?? null,
	};
}

export function createSummary(scriptName, extraMetrics = []) {
	return function handleSummary(data) {
		const metrics = {};
		for (const name of [
			"http_reqs",
			"http_req_duration",
			"http_req_failed",
			...extraMetrics,
		]) {
			const snapshot = snapshotMetric(data, name);
			if (snapshot) {
				metrics[name] = snapshot;
			}
		}

		const summary = {
			script: scriptName,
			generated_at: new Date().toISOString(),
			root_group_duration: snapshotMetric(data, "group_duration"),
			metrics,
		};

		const outputs = {
			stdout: `${scriptName}\n${JSON.stringify(summary, null, 2)}\n`,
		};
		if (__ENV.ASTER_BENCH_SUMMARY_DIR) {
			outputs[
				`${__ENV.ASTER_BENCH_SUMMARY_DIR}/${scriptName}.summary.json`
			] = JSON.stringify(summary, null, 2);
		}

		return outputs;
	};
}
