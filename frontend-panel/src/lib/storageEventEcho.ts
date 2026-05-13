import type { Workspace } from "@/lib/workspace";
import { workspaceKey } from "@/lib/workspace";

const ECHO_TTL_MS = 5_000;

type StorageChangeKind =
	| "file.created"
	| "file.updated"
	| "file.deleted"
	| "file.restored"
	| "folder.created"
	| "folder.updated"
	| "folder.deleted"
	| "folder.restored"
	| "sync.required";

type StorageChangeWorkspace =
	| { kind: "personal" }
	| { kind: "team"; team_id: number };

export interface StorageChangeEventPayload {
	kind: StorageChangeKind;
	workspace?: StorageChangeWorkspace | null;
	file_ids: number[];
	folder_ids: number[];
	affected_parent_ids: number[];
	root_affected: boolean;
	at: string;
}

interface StorageEventEchoInput {
	kind: StorageChangeKind;
	workspace: Workspace;
	fileIds?: number[];
	folderIds?: number[];
}

interface StorageDeleteEchoInput {
	workspace: Workspace;
	fileIds?: number[];
	folderIds?: number[];
}

interface PendingStorageEventEcho {
	id: number;
	expiresAt: number;
	kind: StorageChangeKind;
	workspaceKey: string;
	fileIds: number[];
	folderIds: number[];
}

let nextEchoId = 1;
let pendingEchoes: PendingStorageEventEcho[] = [];

function normalizeIds(ids: number[] | undefined): number[] {
	return Array.from(new Set(ids ?? [])).sort((a, b) => a - b);
}

function sameIds(a: number[], b: number[]) {
	return a.length === b.length && a.every((id, index) => id === b[index]);
}

function eventWorkspaceKey(
	workspace: StorageChangeWorkspace | null | undefined,
) {
	if (!workspace) return null;
	return workspace.kind === "team" ? `team:${workspace.team_id}` : "personal";
}

function pruneExpiredEchoes(now = Date.now()) {
	pendingEchoes = pendingEchoes.filter((echo) => echo.expiresAt > now);
}

export function rememberStorageEventEcho(input: StorageEventEchoInput): number {
	pruneExpiredEchoes();
	const id = nextEchoId;
	nextEchoId += 1;
	pendingEchoes.push({
		id,
		expiresAt: Date.now() + ECHO_TTL_MS,
		kind: input.kind,
		workspaceKey: workspaceKey(input.workspace),
		fileIds: normalizeIds(input.fileIds),
		folderIds: normalizeIds(input.folderIds),
	});
	return id;
}

export function forgetStorageEventEcho(id: number) {
	pendingEchoes = pendingEchoes.filter((echo) => echo.id !== id);
}

export function forgetStorageEventEchoes(ids: number[]) {
	if (ids.length === 0) return;
	const idSet = new Set(ids);
	pendingEchoes = pendingEchoes.filter((echo) => !idSet.has(echo.id));
}

export function rememberStorageDeleteEchoes(input: StorageDeleteEchoInput) {
	const echoIds: number[] = [];
	const fileIds = normalizeIds(input.fileIds);
	const folderIds = normalizeIds(input.folderIds);

	if (fileIds.length > 0) {
		echoIds.push(
			rememberStorageEventEcho({
				kind: "file.deleted",
				workspace: input.workspace,
				fileIds,
			}),
		);
	}
	if (folderIds.length > 0) {
		echoIds.push(
			rememberStorageEventEcho({
				kind: "folder.deleted",
				workspace: input.workspace,
				folderIds,
			}),
		);
	}

	return echoIds;
}

export function consumeStorageEventEcho(event: StorageChangeEventPayload) {
	pruneExpiredEchoes();
	const workspace = eventWorkspaceKey(event.workspace);
	if (!workspace) return false;

	const fileIds = normalizeIds(event.file_ids);
	const folderIds = normalizeIds(event.folder_ids);
	const index = pendingEchoes.findIndex(
		(echo) =>
			echo.kind === event.kind &&
			echo.workspaceKey === workspace &&
			sameIds(echo.fileIds, fileIds) &&
			sameIds(echo.folderIds, folderIds),
	);
	if (index === -1) return false;

	pendingEchoes.splice(index, 1);
	return true;
}

export function clearStorageEventEchoes() {
	pendingEchoes = [];
}
