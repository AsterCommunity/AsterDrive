export interface UploadTransferMetrics {
	uploadedBytes: number;
	speedBps?: number;
}

const MIN_SPEED_SAMPLE_INTERVAL_MS = 250;
const SPEED_SMOOTHING_ALPHA = 0.3;

function normalizeBytes(value: number): number {
	return Number.isFinite(value) ? Math.max(0, value) : 0;
}

function roundedSpeed(speedBps: number | undefined): number | undefined {
	if (!speedBps || !Number.isFinite(speedBps) || speedBps <= 0) {
		return undefined;
	}
	return Math.round(speedBps);
}

export function createUploadSpeedTracker(
	initialUploadedBytes = 0,
	now: () => number = () => Date.now(),
) {
	let lastUploadedBytes = normalizeBytes(initialUploadedBytes);
	let lastSampleAt = now();
	let smoothedSpeedBps: number | undefined;

	const sample = (uploadedBytes: number): UploadTransferMetrics => {
		const normalizedUploadedBytes = normalizeBytes(uploadedBytes);
		const sampledAt = now();
		const elapsedMs = sampledAt - lastSampleAt;
		const deltaBytes = normalizedUploadedBytes - lastUploadedBytes;

		if (deltaBytes < 0) {
			lastUploadedBytes = normalizedUploadedBytes;
			lastSampleAt = sampledAt;
			smoothedSpeedBps = undefined;
			return {
				uploadedBytes: normalizedUploadedBytes,
			};
		}

		if (elapsedMs >= MIN_SPEED_SAMPLE_INTERVAL_MS) {
			const instantSpeedBps = (deltaBytes * 1000) / elapsedMs;
			if (Number.isFinite(instantSpeedBps) && instantSpeedBps > 0) {
				smoothedSpeedBps =
					smoothedSpeedBps === undefined
						? instantSpeedBps
						: smoothedSpeedBps * (1 - SPEED_SMOOTHING_ALPHA) +
							instantSpeedBps * SPEED_SMOOTHING_ALPHA;
			}
			lastUploadedBytes = normalizedUploadedBytes;
			lastSampleAt = sampledAt;
		}

		return {
			uploadedBytes: normalizedUploadedBytes,
			speedBps: roundedSpeed(smoothedSpeedBps),
		};
	};

	const stop = (uploadedBytes = lastUploadedBytes): UploadTransferMetrics => {
		lastUploadedBytes = normalizeBytes(uploadedBytes);
		smoothedSpeedBps = undefined;
		return {
			uploadedBytes: lastUploadedBytes,
		};
	};

	return {
		sample,
		stop,
	};
}
