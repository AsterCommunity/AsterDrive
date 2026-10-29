import type {
	BrowserUploadInstruction,
	BrowserUploadReport,
	BrowserUploadStep,
} from "@/types/api";
import {
	type createUploadService,
	isRetryableUploadError,
	UploadRequestError,
} from "./uploadService";

type Adapter = Pick<
	ReturnType<typeof createUploadService>,
	"coordinateBrowserUpload" | "executeBrowserUpload"
>;

/** Coordinates a provider state machine without knowing its protocol or wire response format. */
export async function runBrowserUpload(
	service: Adapter,
	uploadId: string,
	file: File,
	initial: BrowserUploadStep,
	options: {
		isAborted: () => boolean;
		onProgress: (confirmedOffset: number) => void;
		execute: (
			instruction: BrowserUploadInstruction,
		) => ReturnType<Adapter["executeBrowserUpload"]>;
	},
): Promise<void> {
	let previous = 0;
	let previousRevision = 0;
	const checkAbort = () => {
		if (options.isAborted())
			throw new UploadRequestError("upload aborted", { isAborted: true });
	};
	const validate = (step: BrowserUploadStep) => {
		if (
			step.executor_version !== 1 ||
			!Number.isSafeInteger(step.progress_revision) ||
			step.progress_revision < previousRevision ||
			!Number.isSafeInteger(step.confirmed_offset) ||
			step.confirmed_offset < previous ||
			step.confirmed_offset > file.size ||
			(step.provider_completed &&
				(step.confirmed_offset !== file.size || step.instruction)) ||
			step.protocol.id !== initial.protocol.id ||
			step.protocol.version !== initial.protocol.version
		) {
			throw new UploadRequestError("invalid coordinated upload state");
		}
		previous = step.confirmed_offset;
		previousRevision = step.progress_revision;
		options.onProgress(previous);
	};
	let step = initial;
	validate(step);
	let stagnant = 0;
	let partials = 0;
	let checkpoint = previous;
	const coordinate = async (report: BrowserUploadReport) => {
		for (let attempt = 0; ; attempt++) {
			checkAbort();
			try {
				const next = await service.coordinateBrowserUpload(uploadId, report);
				checkAbort();
				validate(next);
				return next;
			} catch (error) {
				checkAbort();
				if (!isRetryableUploadError(error) || attempt >= 2) throw error;
				// Retry the same request ID/report. The backend reconciles an already consumed ID.
				await new Promise((resolve) =>
					setTimeout(resolve, 1000 * 2 ** attempt),
				);
			}
		}
	};
	// Persisted instructions can be stale after a lost response. Reconcile once
	// at the beginning of every transfer, before sending any file bytes.
	if (!step.provider_completed) step = await coordinate({});
	const firstRange = step.instruction?.request.body;
	const fragmentBudget = firstRange
		? firstRange.end - firstRange.start
		: file.size;
	while (!step.provider_completed) {
		checkAbort();
		const instruction = step.instruction;
		if (!instruction)
			throw new UploadRequestError("missing browser upload instruction");
		const before = step.confirmed_offset;
		const beforeRevision = step.progress_revision;
		const observation = await options.execute(instruction);
		checkAbort();
		step = await coordinate({
			request_id: instruction.request_id,
			observation,
		});
		if (
			step.confirmed_offset === before &&
			step.progress_revision === beforeRevision &&
			!step.provider_completed
		) {
			if (++stagnant >= 3)
				throw new UploadRequestError("provider upload made no progress", {
					retryable: true,
				});
			await new Promise((resolve) =>
				setTimeout(resolve, 1000 * 2 ** (stagnant - 1)),
			);
		} else stagnant = 0;
		// Bound pathological partial acknowledgements per planned fragment, not the whole file.
		if (step.confirmed_offset - checkpoint >= fragmentBudget) {
			checkpoint = step.confirmed_offset;
			partials = 0;
		} else if (step.confirmed_offset > before && ++partials > 32)
			throw new UploadRequestError(
				"provider acknowledged too many partial fragments",
				{ retryable: true },
			);
	}
	checkAbort();
}
