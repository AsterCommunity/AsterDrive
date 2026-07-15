import { useCallback, useEffect, useRef, useState } from "react";
import { FOLDER_LIMIT } from "@/lib/constants";
import { shareService } from "@/services/shareService";
import type { FolderContents, FolderListItem } from "@/types/api";
import type { ShareBreadcrumbItem } from "./types";

const ROOT_KEY = "root";

type ParentFolderId = number | null;

export interface ShareFolderTreeNode {
	childIds: number[];
	folder: FolderListItem;
	parentId: ParentFolderId;
}

function parentKey(parentId: ParentFolderId) {
	return parentId === null ? ROOT_KEY : String(parentId);
}

function breadcrumbFolder(item: ShareBreadcrumbItem): FolderListItem {
	return {
		id: item.id as number,
		is_locked: false,
		is_shared: false,
		name: item.name,
		tags: [],
		updated_at: "",
	};
}

function mergeChildren(
	current: Map<number, ShareFolderTreeNode>,
	parentId: ParentFolderId,
	folders: FolderListItem[],
) {
	const next = new Map(current);
	for (const folder of folders) {
		const existing = next.get(folder.id);
		next.set(folder.id, {
			childIds: existing?.childIds ?? [],
			folder,
			parentId,
		});
	}
	if (parentId !== null) {
		const parent = next.get(parentId);
		if (parent) {
			next.set(parentId, {
				...parent,
				childIds: folders.map((folder) => folder.id),
			});
		}
	}
	return next;
}

export function useShareFolderTree({
	breadcrumb,
	folderContents,
	token,
}: {
	breadcrumb: ShareBreadcrumbItem[];
	folderContents: FolderContents | null;
	token: string;
}) {
	const [nodeMap, setNodeMap] = useState(
		() => new Map<number, ShareFolderTreeNode>(),
	);
	const [rootIds, setRootIds] = useState<number[]>([]);
	const [expandedKeys, setExpandedKeys] = useState(
		() => new Set<string>([ROOT_KEY]),
	);
	const [loadedKeys, setLoadedKeys] = useState(() => new Set<string>());
	const [loadingKeys, setLoadingKeys] = useState(() => new Set<string>());
	const [failedKeys, setFailedKeys] = useState(() => new Set<string>());
	const generationRef = useRef(0);
	const loadedKeysRef = useRef(loadedKeys);
	const expandedKeysRef = useRef(expandedKeys);
	const failedKeysRef = useRef(failedKeys);
	const inFlightRef = useRef(new Map<string, Promise<void>>());
	loadedKeysRef.current = loadedKeys;
	expandedKeysRef.current = expandedKeys;
	failedKeysRef.current = failedKeys;

	const breadcrumbKey = JSON.stringify(breadcrumb);
	const stableBreadcrumbRef = useRef({ key: breadcrumbKey, value: breadcrumb });
	if (stableBreadcrumbRef.current.key !== breadcrumbKey) {
		stableBreadcrumbRef.current = { key: breadcrumbKey, value: breadcrumb };
	}
	const stableBreadcrumb = stableBreadcrumbRef.current.value;
	const folderContentsKey = folderContents
		? JSON.stringify(
				folderContents.folders.map((folder) => [
					folder.id,
					folder.name,
					folder.updated_at,
				]),
			)
		: "null";
	const stableFolderContentsRef = useRef({
		key: folderContentsKey,
		value: folderContents,
	});
	if (stableFolderContentsRef.current.key !== folderContentsKey) {
		stableFolderContentsRef.current = {
			key: folderContentsKey,
			value: folderContents,
		};
	}
	const stableFolderContents = stableFolderContentsRef.current.value;
	const currentFolderId =
		stableBreadcrumb[stableBreadcrumb.length - 1]?.id ?? null;
	useEffect(() => {
		if (token.length === 0) return;
		generationRef.current += 1;
		inFlightRef.current.clear();
		loadedKeysRef.current = new Set();
		expandedKeysRef.current = new Set([ROOT_KEY]);
		failedKeysRef.current = new Set();
		setNodeMap(new Map());
		setRootIds([]);
		setExpandedKeys(new Set([ROOT_KEY]));
		setLoadedKeys(new Set());
		setLoadingKeys(new Set());
		setFailedKeys(new Set());
	}, [token]);

	const commitChildren = useCallback(
		(parentId: ParentFolderId, folders: FolderListItem[]) => {
			const key = parentKey(parentId);
			setNodeMap((current) => mergeChildren(current, parentId, folders));
			if (parentId === null) {
				setRootIds(folders.map((folder) => folder.id));
			}
			const nextLoadedKeys = new Set(loadedKeysRef.current).add(key);
			loadedKeysRef.current = nextLoadedKeys;
			setLoadedKeys(nextLoadedKeys);
			setFailedKeys((current) => {
				if (!current.has(key)) return current;
				const next = new Set(current);
				next.delete(key);
				failedKeysRef.current = next;
				return next;
			});
		},
		[],
	);

	const loadChildren = useCallback(
		(parentId: ParentFolderId) => {
			const key = parentKey(parentId);
			if (loadedKeysRef.current.has(key)) return Promise.resolve();
			const inFlight = inFlightRef.current.get(key);
			if (inFlight && !failedKeysRef.current.has(key)) return inFlight;
			if (inFlight) inFlightRef.current.delete(key);

			const generation = generationRef.current;
			setLoadingKeys((current) => new Set(current).add(key));
			const request = (async () => {
				try {
					const params = {
						file_limit: 0,
						folder_limit: FOLDER_LIMIT,
						sort_by: "name" as const,
						sort_order: "asc" as const,
					};
					const contents =
						parentId === null
							? await shareService.listContent(token, params)
							: await shareService.listSubfolderContent(
									token,
									parentId,
									params,
								);
					if (generation !== generationRef.current) return;
					commitChildren(parentId, contents.folders);
				} catch (error) {
					if (generation === generationRef.current) {
						const nextFailedKeys = new Set(failedKeysRef.current).add(key);
						failedKeysRef.current = nextFailedKeys;
						setFailedKeys(nextFailedKeys);
					}
					throw error;
				} finally {
					if (generation === generationRef.current) {
						setLoadingKeys((current) => {
							const next = new Set(current);
							next.delete(key);
							return next;
						});
					}
				}
			})();
			inFlightRef.current.set(key, request);
			void request.then(
				() => {
					if (inFlightRef.current.get(key) === request) {
						inFlightRef.current.delete(key);
					}
				},
				() => {
					if (inFlightRef.current.get(key) === request) {
						inFlightRef.current.delete(key);
					}
				},
			);
			return request;
		},
		[commitChildren, token],
	);

	useEffect(() => {
		if (stableBreadcrumb.length <= 1) return;

		setNodeMap((current) => {
			const next = new Map(current);
			for (let index = 1; index < stableBreadcrumb.length; index += 1) {
				const item = stableBreadcrumb[index];
				if (item?.id == null) continue;
				const parentId = stableBreadcrumb[index - 1]?.id ?? null;
				const existing = next.get(item.id);
				next.set(item.id, {
					childIds: existing?.childIds ?? [],
					folder: existing?.folder ?? breadcrumbFolder(item),
					parentId,
				});
				if (parentId !== null) {
					const parent = next.get(parentId);
					if (parent && !parent.childIds.includes(item.id)) {
						next.set(parentId, {
							...parent,
							childIds: [...parent.childIds, item.id],
						});
					}
				}
			}
			return next;
		});

		const firstId = stableBreadcrumb[1]?.id;
		if (firstId != null) {
			setRootIds((current) =>
				current.includes(firstId) ? current : [...current, firstId],
			);
		}
		setExpandedKeys((current) => {
			const next = new Set(current).add(ROOT_KEY);
			for (const item of stableBreadcrumb.slice(1)) {
				if (item.id != null) next.add(String(item.id));
			}
			expandedKeysRef.current = next;
			return next;
		});
	}, [stableBreadcrumb]);

	useEffect(() => {
		if (!stableFolderContents || stableBreadcrumb.length === 0) return;
		commitChildren(currentFolderId, stableFolderContents.folders);
	}, [
		commitChildren,
		currentFolderId,
		stableBreadcrumb.length,
		stableFolderContents,
	]);

	useEffect(() => {
		if (stableBreadcrumb.length === 0) return;
		const pathParents: ParentFolderId[] = [
			null,
			...stableBreadcrumb
				.slice(1, -1)
				.map((item) => item.id)
				.filter((id): id is number => id !== null),
		];
		for (const parentId of pathParents) {
			void loadChildren(parentId).catch(() => undefined);
		}
	}, [loadChildren, stableBreadcrumb]);

	const toggle = useCallback(
		(parentId: ParentFolderId) => {
			const key = parentKey(parentId);
			const nextExpandedKeys = new Set(expandedKeysRef.current);
			const opening = !nextExpandedKeys.has(key);
			if (opening) nextExpandedKeys.add(key);
			else nextExpandedKeys.delete(key);
			expandedKeysRef.current = nextExpandedKeys;
			setExpandedKeys(nextExpandedKeys);
			if (
				opening &&
				(!loadedKeysRef.current.has(key) || failedKeysRef.current.has(key))
			) {
				void loadChildren(parentId).catch(() => undefined);
			}
		},
		[loadChildren],
	);

	return {
		currentFolderId,
		expandedKeys,
		failedKeys,
		loadedKeys,
		loadingKeys,
		nodeMap,
		rootIds,
		toggle,
	};
}
