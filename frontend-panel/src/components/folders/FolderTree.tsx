import { SkeletonTree } from "@/components/common/SkeletonTree";
import { SIDEBAR_SECTION_PADDING_CLASS } from "@/lib/constants";
import { AnimatedTreeGroup } from "./folder-tree/AnimatedTreeGroup";
import { FolderTreeBranch } from "./folder-tree/FolderTreeBranch";
import { FolderTreeRootRow } from "./folder-tree/FolderTreeRootRow";
import type { FolderTreeProps } from "./folder-tree/types";
import { useFolderTreeController } from "./folder-tree/useFolderTreeController";

export function FolderTree({ onMoveToFolder }: FolderTreeProps = {}) {
	const { branchProps, rootExpanded, rootLoaded, rootProps, visibleRootIds } =
		useFolderTreeController({ onMoveToFolder });

	return (
		<div className={`${SIDEBAR_SECTION_PADDING_CLASS} py-2 space-y-0.5`}>
			{!rootLoaded ? (
				<SkeletonTree count={4} />
			) : (
				<>
					<FolderTreeRootRow {...rootProps} />
					<AnimatedTreeGroup open={rootExpanded && visibleRootIds.length > 0}>
						<FolderTreeBranch {...branchProps} depth={1} ids={visibleRootIds} />
					</AnimatedTreeGroup>
				</>
			)}
		</div>
	);
}
