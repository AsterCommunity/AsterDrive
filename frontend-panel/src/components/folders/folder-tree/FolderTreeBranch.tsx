import { FolderTreeNodeRow } from "./FolderTreeNode";
import type { TreeNodeProps } from "./types";

interface FolderTreeBranchProps
	extends Omit<TreeNodeProps, "children" | "nodeId"> {
	depth: number;
	ids: number[];
}

export function FolderTreeBranch({
	depth,
	ids,
	...nodeProps
}: FolderTreeBranchProps) {
	return ids.map((id) => {
		const node = nodeProps.nodeMap.get(id);
		const childIds = node?.childIds ?? [];

		return (
			<FolderTreeNodeRow key={id} {...nodeProps} depth={depth} nodeId={id}>
				<FolderTreeBranch {...nodeProps} depth={depth + 1} ids={childIds} />
			</FolderTreeNodeRow>
		);
	});
}
