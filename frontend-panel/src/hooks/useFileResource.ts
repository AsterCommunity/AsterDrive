import { useEffect, useMemo, useRef, useState } from "react";
import type {
	FileResourceDeliveryMode,
	ReadyFileResourceHandle,
} from "@/lib/resourceRequest";
import { fileService } from "@/services/fileService";
import type {
	FileResourceHandleRequest,
	FileResourceRepresentation,
} from "@/types/api";

export type ResolveFileResourceHandleRequest = FileResourceHandleRequest;

export type ResolveFileResourceHandle = (
	fileId: number,
	request: ResolveFileResourceHandleRequest,
) => Promise<ReadyFileResourceHandle>;

type FileResourceState = {
	fileId: number;
	requestKey: string;
	resource: ReadyFileResourceHandle | null;
	status: "idle" | "loading" | "ready" | "failed";
};

interface UseFileContentResourceOptions {
	deliveryMode?: FileResourceDeliveryMode;
	downloadPath: string;
	enabled?: boolean;
	fileId: number;
	mimeType?: string;
	open: boolean;
	representation?: FileResourceRepresentation;
	resolveResourceHandle?: ResolveFileResourceHandle;
}

function fileResourceRequestKey({
	deliveryMode,
	downloadPath,
	fileId,
	mimeType,
	representation,
}: Pick<
	UseFileContentResourceOptions,
	"downloadPath" | "fileId" | "mimeType" | "representation"
> & {
	deliveryMode: FileResourceDeliveryMode;
}) {
	return [
		fileId,
		downloadPath,
		deliveryMode,
		representation ?? "auto",
		mimeType ?? "",
	].join("\u0000");
}

export function useFileContentResource({
	deliveryMode = "blob_url",
	downloadPath,
	enabled = true,
	fileId,
	mimeType,
	open,
	representation = "auto",
	resolveResourceHandle = fileService.resolveResourceHandle,
}: UseFileContentResourceOptions): ReadyFileResourceHandle | null {
	const requestKey = useMemo(
		() =>
			fileResourceRequestKey({
				deliveryMode,
				downloadPath,
				fileId,
				mimeType,
				representation,
			}),
		[deliveryMode, downloadPath, fileId, mimeType, representation],
	);
	const requestRef = useRef<{
		promise: Promise<ReadyFileResourceHandle>;
		requestKey: string;
	} | null>(null);
	const [state, setState] = useState<FileResourceState>(() => ({
		fileId,
		requestKey,
		resource: null,
		status: "idle",
	}));

	useEffect(() => {
		if (!open || !enabled) {
			requestRef.current = null;
			setState((current) =>
				current.fileId === fileId &&
				current.requestKey === requestKey &&
				current.status === "idle" &&
				current.resource === null
					? current
					: {
							fileId,
							requestKey,
							resource: null,
							status: "idle",
						},
			);
			return;
		}

		let cancelled = false;
		setState((current) => {
			if (
				current.fileId === fileId &&
				current.requestKey === requestKey &&
				(current.status === "ready" || current.status === "loading")
			) {
				return current;
			}
			return {
				fileId,
				requestKey,
				resource: null,
				status: "loading",
			};
		});

		let request = requestRef.current;
		if (!request || request.requestKey !== requestKey) {
			request = {
				promise: resolveResourceHandle(fileId, {
					delivery_mode: deliveryMode,
					purpose: "preview",
					representation,
				}),
				requestKey,
			};
			requestRef.current = request;
		}

		request.promise
			.then((resource) => {
				if (cancelled || requestRef.current !== request) return;
				setState({
					fileId,
					requestKey,
					resource,
					status: "ready",
				});
			})
			.catch(() => {
				if (cancelled || requestRef.current !== request) return;
				setState({
					fileId,
					requestKey,
					resource: null,
					status: "failed",
				});
			});

		return () => {
			cancelled = true;
		};
	}, [
		deliveryMode,
		enabled,
		fileId,
		open,
		representation,
		requestKey,
		resolveResourceHandle,
	]);

	if (!open || !enabled) return null;
	if (state.fileId !== fileId || state.requestKey !== requestKey) return null;
	return state.status === "ready" ? state.resource : null;
}
