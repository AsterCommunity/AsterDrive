import {
	resolveApiResourceUrl,
	shouldSendResourceCredentials,
} from "@/lib/apiUrl";
import { ensureZipExtension } from "@/lib/downloadFilenames";
import type { Workspace } from "@/lib/workspace";
import { createBatchService } from "@/services/batchService";
import { createFileService } from "@/services/fileService";
import { ApiError, isRequestCanceled } from "@/services/http";
import {
	DOWNLOAD_TASK_STATUS,
	type DownloadSelection,
	type DownloadTask,
	type DownloadTaskItem,
	useDownloadStore,
} from "@/stores/downloadStore";
import type { FolderContents } from "@/types/api";
import { isApiErrorCode } from "@/types/api-helpers";

interface WritableStreamLike {
	write(data: Uint8Array): Promise<void>;
	close(): Promise<void>;
	abort?(reason?: unknown): Promise<void>;
}

interface FileHandleLike {
	createWritable(): Promise<WritableStreamLike>;
}

interface DirectoryHandleLike {
	getDirectoryHandle(
		name: string,
		options?: { create?: boolean },
	): Promise<DirectoryHandleLike>;
	getFileHandle(
		name: string,
		options?: { create?: boolean },
	): Promise<FileHandleLike>;
}

type FilePickerWindow = Window & {
	showDirectoryPicker?: (options?: {
		mode?: "read" | "readwrite";
	}) => Promise<DirectoryHandleLike>;
	showSaveFilePicker?: (options?: {
		suggestedName?: string;
		types?: Array<{ description?: string; accept: Record<string, string[]> }>;
	}) => Promise<FileHandleLike>;
};

interface TransferProgress {
	bytesReceived: number;
	totalBytes: number | null;
	speedBps: number | null;
}

interface DownloadSource {
	url: string;
	credentials: RequestCredentials;
}

interface DirectoryFile {
	id: number;
	name: string;
	relativePath: string;
	size: number;
}

interface ExpandedSelection {
	directories: string[];
	files: DirectoryFile[];
}

const activeControllers = new Map<string, AbortController>();
const retryActions = new Map<string, () => void>();
const DIRECTORY_CONCURRENCY = 3;
const LIST_PAGE_SIZE = 1000;

function taskId() {
	return crypto.randomUUID();
}

function newTask(
	kind: DownloadTask["kind"],
	name: string,
	totalItems = 1,
): DownloadTask {
	return {
		id: taskId(),
		kind,
		name,
		status: DOWNLOAD_TASK_STATUS.queued,
		createdAt: Date.now(),
		bytesReceived: 0,
		totalBytes: null,
		speedBps: null,
		completedItems: 0,
		failedItems: 0,
		totalItems,
		items: [],
	};
}

function updateTask(id: string, patch: Partial<DownloadTask>) {
	useDownloadStore.getState().updateTask(id, patch);
}

function getTask(id: string) {
	return useDownloadStore.getState().tasks.find((task) => task.id === id);
}

function errorMessage(error: unknown) {
	return error instanceof Error ? error.message : String(error);
}

function isPickerCanceled(error: unknown) {
	return (
		isRequestCanceled(error) ||
		(error instanceof DOMException && error.name === "AbortError")
	);
}

function finishWithError(task: DownloadTask, error: unknown) {
	const canceled =
		isPickerCanceled(error) || activeControllers.get(task.id)?.signal.aborted;
	updateTask(task.id, {
		status: canceled
			? DOWNLOAD_TASK_STATUS.canceled
			: DOWNLOAD_TASK_STATUS.failed,
		error: canceled ? undefined : errorMessage(error),
		speedBps: null,
	});
}

function decodeContentDispositionValue(value: string) {
	try {
		return decodeURIComponent(value);
	} catch {
		return value;
	}
}

export function filenameFromContentDisposition(value: string | null) {
	if (!value) return null;
	const extended = value.match(/(?:^|;)\s*filename\*\s*=\s*([^;]+)/i)?.[1];
	if (extended) {
		const normalized = extended.trim().replace(/^"|"$/g, "");
		const encoded = normalized.match(/^[^']*'[^']*'(.*)$/)?.[1] ?? normalized;
		const decoded = decodeContentDispositionValue(encoded).trim();
		if (decoded) return decoded.split(/[\\/]/).pop() ?? null;
	}
	const basic = value.match(/(?:^|;)\s*filename\s*=\s*(?:"([^"]*)"|([^;]+))/i);
	const decoded = (basic?.[1] ?? basic?.[2]?.trim() ?? "").trim();
	return decoded ? (decoded.split(/[\\/]/).pop() ?? null) : null;
}

async function parseDownloadError(response: Response) {
	const contentType = response.headers.get("content-type") ?? "";
	if (contentType.includes("application/json")) {
		try {
			const payload = (await response.json()) as unknown;
			if (typeof payload === "object" && payload !== null) {
				const code = "code" in payload ? payload.code : null;
				const message = "msg" in payload ? payload.msg : null;
				if (
					typeof code === "string" &&
					isApiErrorCode(code) &&
					typeof message === "string"
				) {
					return new ApiError(code, message, { status: response.status });
				}
			}
		} catch {
			// Fall through to the HTTP status when the response is not a valid envelope.
		}
	}
	return new Error(`Download request failed with HTTP ${response.status}`);
}

async function fetchDownload(source: DownloadSource, signal: AbortSignal) {
	const response = await fetch(resolveApiResourceUrl(source.url), {
		credentials: source.credentials,
		redirect: "follow",
		signal,
	});
	if (!response.ok) throw await parseDownloadError(response);
	return response;
}

async function streamResponse(
	response: Response,
	sink: WritableStreamLike,
	signal: AbortSignal,
	onProgress: (progress: TransferProgress) => void,
) {
	const totalHeader = response.headers.get("content-length");
	const parsedTotal = totalHeader === null ? Number.NaN : Number(totalHeader);
	const totalBytes =
		Number.isFinite(parsedTotal) && parsedTotal >= 0 ? parsedTotal : null;
	let bytesReceived = 0;
	let sampleBytes = 0;
	let sampleStartedAt = performance.now();

	try {
		if (!response.body) {
			const chunk = new Uint8Array(await response.arrayBuffer());
			await sink.write(chunk);
			bytesReceived = chunk.byteLength;
			onProgress({ bytesReceived, totalBytes, speedBps: null });
			await sink.close();
			return;
		}

		const reader = response.body.getReader();
		while (true) {
			if (signal.aborted)
				throw new DOMException("Download canceled", "AbortError");
			const { done, value } = await reader.read();
			if (done) break;
			await sink.write(value);
			bytesReceived += value.byteLength;
			sampleBytes += value.byteLength;
			const now = performance.now();
			const elapsedMs = now - sampleStartedAt;
			const speedBps =
				elapsedMs >= 500 ? (sampleBytes * 1000) / elapsedMs : null;
			if (speedBps !== null) {
				sampleBytes = 0;
				sampleStartedAt = now;
			}
			onProgress({ bytesReceived, totalBytes, speedBps });
		}
		if (totalBytes !== null && bytesReceived !== totalBytes) {
			throw new Error(
				`Download size mismatch: expected ${totalBytes}, received ${bytesReceived}`,
			);
		}
		await sink.close();
	} catch (error) {
		await sink.abort?.(error).catch(() => undefined);
		throw error;
	}
}

function blobSink() {
	const chunks: BlobPart[] = [];
	return {
		sink: {
			write: async (data: Uint8Array) => {
				chunks.push(data.slice());
			},
			close: async () => undefined,
		},
		blob: (type?: string | null) =>
			new Blob(chunks, { type: type ?? undefined }),
	};
}

function triggerBlobDownload(blob: Blob, name: string) {
	const url = URL.createObjectURL(blob);
	const anchor = document.createElement("a");
	anchor.href = url;
	anchor.download = name;
	document.body.append(anchor);
	anchor.click();
	anchor.remove();
	window.setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

async function chooseFileSink(name: string, zip: boolean) {
	const pickerWindow = window as FilePickerWindow;
	if (!pickerWindow.showSaveFilePicker) return null;
	const handle = await pickerWindow.showSaveFilePicker({
		suggestedName: name,
		...(zip
			? {
					types: [
						{
							description: "ZIP archive",
							accept: { "application/zip": [".zip"] },
						},
					],
				}
			: {}),
	});
	return handle.createWritable();
}

function throwIfCanceled(signal: AbortSignal) {
	if (signal.aborted) {
		throw new DOMException("Download canceled", "AbortError");
	}
}

async function resolveFileSource(workspace: Workspace, fileId: number) {
	const handle = await createFileService(workspace).resolveResourceHandle(
		fileId,
		{
			purpose: "download",
			delivery_mode: "blob_url",
			representation: "original",
		},
	);
	return {
		url: handle.request.url,
		credentials: handle.request.credentials,
	} satisfies DownloadSource;
}

export async function startProxyFileDownload(
	workspace: Workspace,
	file: { id: number; name: string; size?: number },
) {
	const task = newTask("file", file.name);
	useDownloadStore.getState().upsertTask(task);
	const retry = () => {
		void runProxyFileDownload(task, workspace, file);
	};
	retryActions.set(task.id, retry);
	await runProxyFileDownload(task, workspace, file);
	return task.id;
}

async function runProxyFileDownload(
	task: DownloadTask,
	workspace: Workspace,
	file: { id: number; name: string; size?: number },
) {
	const controller = new AbortController();
	activeControllers.set(task.id, controller);

	try {
		const writable = await chooseFileSink(file.name, false);
		throwIfCanceled(controller.signal);
		updateTask(task.id, {
			status: DOWNLOAD_TASK_STATUS.preparing,
			totalBytes: file.size ?? null,
			warning: writable ? undefined : "download_memory_fallback",
			error: undefined,
			bytesReceived: 0,
			speedBps: null,
			completedItems: 0,
		});
		const source = await resolveFileSource(workspace, file.id);
		const response = await fetchDownload(source, controller.signal);
		const responseName =
			filenameFromContentDisposition(
				response.headers.get("content-disposition"),
			) ?? file.name;
		const fallback = writable ? null : blobSink();
		updateTask(task.id, {
			name: responseName,
			status: DOWNLOAD_TASK_STATUS.downloading,
		});
		await streamResponse(
			response,
			writable ?? fallback?.sink ?? blobSink().sink,
			controller.signal,
			(progress) => updateTask(task.id, progress),
		);
		if (fallback)
			triggerBlobDownload(
				fallback.blob(response.headers.get("content-type")),
				responseName,
			);
		updateTask(task.id, {
			status: DOWNLOAD_TASK_STATUS.completed,
			completedItems: 1,
			speedBps: null,
		});
	} catch (error) {
		finishWithError(task, error);
	} finally {
		activeControllers.delete(task.id);
	}
}

export async function startProxyArchiveDownload(
	selection: DownloadSelection,
	archiveName?: string,
) {
	const name = ensureZipExtension(
		archiveName ?? suggestedArchiveName(selection),
	);
	const task = newTask("archive", name);
	useDownloadStore.getState().upsertTask(task);
	const retry = () => {
		void runProxyArchiveDownload(task, selection, name);
	};
	retryActions.set(task.id, retry);
	await runProxyArchiveDownload(task, selection, name);
	return task.id;
}

async function runProxyArchiveDownload(
	task: DownloadTask,
	selection: DownloadSelection,
	name: string,
) {
	const controller = new AbortController();
	activeControllers.set(task.id, controller);

	try {
		const writable = await chooseFileSink(name, true);
		throwIfCanceled(controller.signal);
		updateTask(task.id, {
			status: DOWNLOAD_TASK_STATUS.preparing,
			warning: writable ? undefined : "download_memory_fallback",
			error: undefined,
			bytesReceived: 0,
			totalBytes: null,
			speedBps: null,
			completedItems: 0,
		});
		const batch = createBatchService(selection.workspace);
		const ticket = await batch.createArchiveDownloadTicket(
			selection.files.map((file) => file.id),
			selection.folders.map((folder) => folder.id),
			name,
		);
		const archiveUrl = batch.archiveDownloadUrl(ticket);
		const response = await fetchDownload(
			{
				url: archiveUrl,
				credentials: shouldSendResourceCredentials(archiveUrl)
					? "include"
					: "omit",
			},
			controller.signal,
		);
		const responseName = ensureZipExtension(
			filenameFromContentDisposition(
				response.headers.get("content-disposition"),
			) ?? name,
		);
		const fallback = writable ? null : blobSink();
		updateTask(task.id, {
			name: responseName,
			status: DOWNLOAD_TASK_STATUS.downloading,
		});
		await streamResponse(
			response,
			writable ?? fallback?.sink ?? blobSink().sink,
			controller.signal,
			(progress) => updateTask(task.id, progress),
		);
		if (fallback)
			triggerBlobDownload(fallback.blob("application/zip"), responseName);
		updateTask(task.id, {
			status: DOWNLOAD_TASK_STATUS.completed,
			completedItems: 1,
			speedBps: null,
		});
	} catch (error) {
		finishWithError(task, error);
	} finally {
		activeControllers.delete(task.id);
	}
}

function suggestedArchiveName(selection: DownloadSelection) {
	if (selection.files.length + selection.folders.length === 1) {
		return `${selection.files[0]?.name ?? selection.folders[0]?.name ?? "download"}.zip`;
	}
	return "asterdrive-download.zip";
}

async function listAllFolderContents(
	workspace: Workspace,
	folderId: number,
): Promise<FolderContents> {
	const service = createFileService(workspace);
	const folders: FolderContents["folders"] = [];
	const files: FolderContents["files"] = [];

	for (let offset = 0; ; offset += LIST_PAGE_SIZE) {
		const page = await service.listFolder(folderId, {
			folder_limit: LIST_PAGE_SIZE,
			folder_offset: offset,
			file_limit: 0,
			sort_by: "name",
			sort_order: "asc",
		});
		folders.push(...page.folders);
		if (folders.length >= page.folders_total || page.folders.length === 0)
			break;
	}

	let cursor: FolderContents["next_file_cursor"] = null;
	do {
		const page = await service.listFolder(folderId, {
			folder_limit: 0,
			file_limit: LIST_PAGE_SIZE,
			file_after_id: cursor?.id,
			file_after_value: cursor?.value,
			sort_by: "name",
			sort_order: "asc",
		});
		files.push(...page.files);
		cursor = page.next_file_cursor;
	} while (cursor);

	return {
		folders,
		files,
		folders_total: folders.length,
		files_total: files.length,
		next_file_cursor: null,
	};
}

async function expandFolder(
	workspace: Workspace,
	folderId: number,
	basePath: string,
	output: ExpandedSelection,
) {
	output.directories.push(basePath);
	const contents = await listAllFolderContents(workspace, folderId);
	for (const file of contents.files) {
		output.files.push({
			id: file.id,
			name: file.name,
			relativePath: `${basePath}/${file.name}`,
			size: file.size,
		});
	}
	for (const folder of contents.folders) {
		await expandFolder(
			workspace,
			folder.id,
			`${basePath}/${folder.name}`,
			output,
		);
	}
}

async function expandSelection(selection: DownloadSelection) {
	const output: ExpandedSelection = {
		directories: [],
		files: selection.files.map((file) => ({
			id: file.id,
			name: file.name,
			relativePath: file.name,
			size: file.size ?? 0,
		})),
	};
	for (const folder of selection.folders) {
		await expandFolder(selection.workspace, folder.id, folder.name, output);
	}
	return output;
}

function splitRelativePath(relativePath: string) {
	return relativePath.split("/").filter(Boolean);
}

async function handleExists(
	getHandle: () => Promise<FileHandleLike | DirectoryHandleLike>,
) {
	try {
		await getHandle();
		return true;
	} catch (error) {
		if (error instanceof DOMException && error.name === "TypeMismatchError")
			return true;
		if (error instanceof DOMException && error.name === "NotFoundError")
			return false;
		throw error;
	}
}

async function entryExists(directory: DirectoryHandleLike, name: string) {
	if (await handleExists(() => directory.getFileHandle(name))) return true;
	return handleExists(() => directory.getDirectoryHandle(name));
}

function splitExtension(name: string) {
	const index = name.lastIndexOf(".");
	return index > 0 ? [name.slice(0, index), name.slice(index)] : [name, ""];
}

async function uniqueFileName(
	directory: DirectoryHandleLike,
	requested: string,
	reserved: Set<string>,
) {
	if (!reserved.has(requested) && !(await entryExists(directory, requested))) {
		reserved.add(requested);
		return requested;
	}
	const [base, extension] = splitExtension(requested);
	for (let index = 1; ; index += 1) {
		const candidate = `${base} (${index})${extension}`;
		if (
			!reserved.has(candidate) &&
			!(await entryExists(directory, candidate))
		) {
			reserved.add(candidate);
			return candidate;
		}
	}
}

async function uniqueDirectoryName(
	directory: DirectoryHandleLike,
	requested: string,
	reserved: Set<string>,
) {
	if (!reserved.has(requested) && !(await entryExists(directory, requested))) {
		reserved.add(requested);
		return requested;
	}
	for (let index = 1; ; index += 1) {
		const candidate = `${requested} (${index})`;
		if (
			!reserved.has(candidate) &&
			!(await entryExists(directory, candidate))
		) {
			reserved.add(candidate);
			return candidate;
		}
	}
}

async function planSelectionRootNames(
	directory: DirectoryHandleLike,
	selection: DownloadSelection,
) {
	const reserved = new Set<string>();
	const folders = [];
	for (const folder of selection.folders) {
		const name = await uniqueDirectoryName(directory, folder.name, reserved);
		await directory.getDirectoryHandle(name, { create: true });
		folders.push({ ...folder, name });
	}
	const files = [];
	for (const file of selection.files) {
		files.push({
			...file,
			name: await uniqueFileName(directory, file.name, reserved),
		});
	}
	return { ...selection, files, folders } satisfies DownloadSelection;
}

async function createDirectoryPath(
	root: DirectoryHandleLike,
	relativePath: string,
) {
	let directory = root;
	for (const segment of splitRelativePath(relativePath)) {
		directory = await directory.getDirectoryHandle(segment, { create: true });
	}
}

async function createDirectoryPaths(
	root: DirectoryHandleLike,
	paths: string[],
) {
	const ordered = [...new Set(paths)].sort(
		(left, right) =>
			splitRelativePath(left).length - splitRelativePath(right).length,
	);
	for (const path of ordered) await createDirectoryPath(root, path);
}

async function createFileSink(root: DirectoryHandleLike, relativePath: string) {
	const segments = splitRelativePath(relativePath);
	const requestedName = segments.pop();
	if (!requestedName) throw new Error("Invalid empty download path");
	let directory = root;
	for (const segment of segments) {
		directory = await directory.getDirectoryHandle(segment, { create: true });
	}
	return (
		await directory.getFileHandle(requestedName, { create: true })
	).createWritable();
}

function directoryTaskItems(files: DirectoryFile[]): DownloadTaskItem[] {
	return files.map((file) => ({
		id: `${file.id}:${file.relativePath}`,
		name: file.name,
		relativePath: file.relativePath,
		status: DOWNLOAD_TASK_STATUS.queued,
		bytesReceived: 0,
		totalBytes: file.size,
		speedBps: null,
	}));
}

function updateDirectoryItem(
	taskId: string,
	itemId: string,
	patch: Partial<DownloadTaskItem>,
) {
	const task = getTask(taskId);
	if (!task) return;
	const items = task.items.map((item) =>
		item.id === itemId ? { ...item, ...patch } : item,
	);
	updateTask(taskId, {
		items,
		bytesReceived: items.reduce((sum, item) => sum + item.bytesReceived, 0),
		speedBps: items.reduce((sum, item) => sum + (item.speedBps ?? 0), 0),
		completedItems: items.filter(
			(item) => item.status === DOWNLOAD_TASK_STATUS.completed,
		).length,
		failedItems: items.filter(
			(item) => item.status === DOWNLOAD_TASK_STATUS.failed,
		).length,
	});
}

async function runDirectoryDownload(
	task: DownloadTask,
	selection: DownloadSelection,
	directory: DirectoryHandleLike,
	files: DirectoryFile[],
) {
	const controller = new AbortController();
	activeControllers.set(task.id, controller);
	updateTask(task.id, {
		status: DOWNLOAD_TASK_STATUS.downloading,
		items: getTask(task.id)?.items.length
			? getTask(task.id)?.items
			: directoryTaskItems(files),
		totalItems: files.length,
		totalBytes: files.reduce((sum, file) => sum + file.size, 0),
		error: undefined,
		speedBps: null,
	});

	let nextIndex = 0;
	const worker = async () => {
		while (nextIndex < files.length && !controller.signal.aborted) {
			const file = files[nextIndex++];
			const itemId = `${file.id}:${file.relativePath}`;
			const existing = getTask(task.id)?.items.find(
				(item) => item.id === itemId,
			);
			if (existing?.status === DOWNLOAD_TASK_STATUS.completed) continue;
			updateDirectoryItem(task.id, itemId, {
				status: DOWNLOAD_TASK_STATUS.preparing,
				error: undefined,
				speedBps: null,
			});
			try {
				const [source, sink] = await Promise.all([
					resolveFileSource(selection.workspace, file.id),
					createFileSink(directory, file.relativePath),
				]);
				const response = await fetchDownload(source, controller.signal);
				updateDirectoryItem(task.id, itemId, {
					status: DOWNLOAD_TASK_STATUS.downloading,
				});
				await streamResponse(response, sink, controller.signal, (progress) =>
					updateDirectoryItem(task.id, itemId, progress),
				);
				updateDirectoryItem(task.id, itemId, {
					status: DOWNLOAD_TASK_STATUS.completed,
					error: undefined,
					speedBps: null,
				});
			} catch (error) {
				if (controller.signal.aborted) break;
				updateDirectoryItem(task.id, itemId, {
					status: DOWNLOAD_TASK_STATUS.failed,
					error: errorMessage(error),
					speedBps: null,
				});
			}
		}
	};

	await Promise.all(
		Array.from(
			{ length: Math.min(DIRECTORY_CONCURRENCY, files.length) },
			worker,
		),
	);
	const current = getTask(task.id);
	if (controller.signal.aborted) {
		updateTask(task.id, {
			status: DOWNLOAD_TASK_STATUS.canceled,
			speedBps: null,
		});
	} else if ((current?.failedItems ?? 0) > 0) {
		updateTask(task.id, {
			status: DOWNLOAD_TASK_STATUS.failed,
			error: "download_items_failed",
			speedBps: null,
		});
	} else {
		updateTask(task.id, {
			status: DOWNLOAD_TASK_STATUS.completed,
			speedBps: null,
		});
	}
	activeControllers.delete(task.id);
}

export async function startDirectoryDownload(selection: DownloadSelection) {
	const pickerWindow = window as FilePickerWindow;
	const task = newTask("directory", "download_to_folder", 0);
	useDownloadStore.getState().upsertTask(task);
	const preparationController = new AbortController();
	activeControllers.set(task.id, preparationController);
	try {
		if (!pickerWindow.showDirectoryPicker) {
			throw new Error("download_directory_unsupported");
		}
		const directory = await pickerWindow.showDirectoryPicker({
			mode: "readwrite",
		});
		throwIfCanceled(preparationController.signal);
		updateTask(task.id, { status: DOWNLOAD_TASK_STATUS.preparing });
		const plannedSelection = await planSelectionRootNames(directory, selection);
		const expanded = await expandSelection(plannedSelection);
		await createDirectoryPaths(directory, expanded.directories);
		throwIfCanceled(preparationController.signal);
		updateTask(task.id, {
			items: directoryTaskItems(expanded.files),
			totalItems: expanded.files.length,
			totalBytes: expanded.files.reduce((sum, file) => sum + file.size, 0),
		});
		const retry = () => {
			void runDirectoryDownload(
				task,
				plannedSelection,
				directory,
				expanded.files,
			);
		};
		retryActions.set(task.id, retry);
		activeControllers.delete(task.id);
		await runDirectoryDownload(
			task,
			plannedSelection,
			directory,
			expanded.files,
		);
	} catch (error) {
		finishWithError(task, error);
		activeControllers.delete(task.id);
	}
	return task.id;
}

export function cancelDownloadTask(id: string) {
	activeControllers.get(id)?.abort();
	updateTask(id, { status: DOWNLOAD_TASK_STATUS.canceled, speedBps: null });
}

export function retryDownloadTask(id: string) {
	retryActions.get(id)?.();
}

export function supportsDirectoryDownload() {
	return typeof (window as FilePickerWindow).showDirectoryPicker === "function";
}
