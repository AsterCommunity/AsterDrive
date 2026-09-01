import { createResumableUploadRunners } from "./uploadAreaResumableUploadRunners";
import { createSimpleUploadRunners } from "./uploadAreaSimpleUploadRunners";
import type {
	UploadTransportRunnerContext,
	UploadTransportRunners,
} from "./uploadAreaUploadRunnerShared";

export type {
	UploadTransportRunnerContext,
	UploadTransportRunners,
} from "./uploadAreaUploadRunnerShared";

export function createUploadTransportRunners(
	context: UploadTransportRunnerContext,
): UploadTransportRunners {
	return {
		...createSimpleUploadRunners(context),
		...createResumableUploadRunners(context),
	};
}
