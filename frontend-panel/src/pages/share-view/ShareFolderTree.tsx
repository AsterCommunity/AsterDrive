import { useTranslation } from "react-i18next";
import { SkeletonTree } from "@/components/common/SkeletonTree";
import { AnimatedTreeGroup } from "@/components/folders/folder-tree/AnimatedTreeGroup";
import { FolderTreeItemContent } from "@/components/folders/folder-tree/FolderTreeItemContent";
import {
	FOLDER_TREE_INDENT_PX,
	FOLDER_TREE_ROW_OFFSET_PX,
	SIDEBAR_SECTION_PADDING_CLASS,
} from "@/lib/constants";
import { folderTreeRowClass } from "@/lib/utils";
import type { FolderContents } from "@/types/api";
import type { ShareBreadcrumbItem } from "./types";
import {
	type ShareFolderTreeNode,
	useShareFolderTree,
} from "./useShareFolderTree";

function ShareFolderTreeBranch({
	currentFolderId,
	depth,
	expandedKeys,
	loadedKeys,
	loadingKeys,
	nodeMap,
	onNavigate,
	onToggle,
	ids,
	toggleLabel,
}: {
	currentFolderId: number | null;
	depth: number;
	expandedKeys: Set<string>;
	loadedKeys: Set<string>;
	loadingKeys: Set<string>;
	nodeMap: Map<number, ShareFolderTreeNode>;
	onNavigate: (folderId: number, folderName: string) => void;
	onToggle: (folderId: number) => void;
	ids: number[];
	toggleLabel: (expanded: boolean) => string;
}) {
	return ids.map((id) => {
		const node = nodeMap.get(id);
		if (!node) return null;
		const key = String(id);
		const expanded = expandedKeys.has(key);
		const loading = loadingKeys.has(key);
		const showToggle =
			loading || !loadedKeys.has(key) || node.childIds.length > 0;
		return (
			<div key={id}>
				<div
					className={folderTreeRowClass(currentFolderId === id)}
					data-share-folder-tree-row={id}
					style={{
						paddingLeft: `${depth * FOLDER_TREE_INDENT_PX + FOLDER_TREE_ROW_OFFSET_PX}px`,
					}}
				>
					<FolderTreeItemContent
						expanded={expanded}
						label={node.folder.name}
						loading={loading}
						showToggle={showToggle}
						toggleLabel={toggleLabel(expanded)}
						onNavigate={() => onNavigate(id, node.folder.name)}
						onToggle={() => onToggle(id)}
					/>
				</div>
				<AnimatedTreeGroup open={expanded && node.childIds.length > 0}>
					<ShareFolderTreeBranch
						currentFolderId={currentFolderId}
						depth={depth + 1}
						expandedKeys={expandedKeys}
						loadedKeys={loadedKeys}
						loadingKeys={loadingKeys}
						nodeMap={nodeMap}
						onNavigate={onNavigate}
						onToggle={onToggle}
						ids={node.childIds}
						toggleLabel={toggleLabel}
					/>
				</AnimatedTreeGroup>
			</div>
		);
	});
}

export function ShareFolderTree({
	breadcrumb,
	folderContents,
	rootName,
	token,
	onNavigate,
}: {
	breadcrumb: ShareBreadcrumbItem[];
	folderContents: FolderContents | null;
	rootName: string;
	token: string;
	onNavigate: (folderId: number | null, folderName?: string) => void;
}) {
	const { t } = useTranslation("files");
	const tree = useShareFolderTree({ breadcrumb, folderContents, token });
	const rootExpanded = tree.expandedKeys.has("root");
	const rootLoading = tree.loadingKeys.has("root");
	const rootLoaded = tree.loadedKeys.has("root");
	const toggleLabel = (expanded: boolean) =>
		t(expanded ? "collapse_tree" : "expand_tree");

	if (breadcrumb.length === 0) {
		return <SkeletonTree count={5} />;
	}

	return (
		<div className={`${SIDEBAR_SECTION_PADDING_CLASS} space-y-0.5 py-2`}>
			<div className={folderTreeRowClass(tree.currentFolderId === null)}>
				<FolderTreeItemContent
					expanded={rootExpanded}
					label={rootName}
					loading={rootLoading}
					showToggle={rootLoading || !rootLoaded || tree.rootIds.length > 0}
					toggleLabel={toggleLabel(rootExpanded)}
					onNavigate={() => onNavigate(null)}
					onToggle={() => tree.toggle(null)}
				/>
			</div>
			<AnimatedTreeGroup open={rootExpanded && tree.rootIds.length > 0}>
				<ShareFolderTreeBranch
					currentFolderId={tree.currentFolderId}
					depth={1}
					expandedKeys={tree.expandedKeys}
					loadedKeys={tree.loadedKeys}
					loadingKeys={tree.loadingKeys}
					nodeMap={tree.nodeMap}
					onNavigate={(folderId, folderName) =>
						onNavigate(folderId, folderName)
					}
					onToggle={tree.toggle}
					ids={tree.rootIds}
					toggleLabel={toggleLabel}
				/>
			</AnimatedTreeGroup>
		</div>
	);
}
