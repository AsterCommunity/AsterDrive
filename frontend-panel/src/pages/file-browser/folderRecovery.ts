import { isApiErrorWithCode } from "@/services/http";
import type { BreadcrumbItem } from "@/stores/fileStore";
import { ApiErrorCode } from "@/types/api-helpers";

export function isFolderUnavailableError(error: unknown) {
	return isApiErrorWithCode(error, ApiErrorCode.FolderNotFound);
}

export function resolveFolderRecoveryTarget(
	breadcrumb: BreadcrumbItem[],
	unavailableFolderId: number,
): BreadcrumbItem {
	const unavailableIndex = breadcrumb.findIndex(
		(item) => item.id === unavailableFolderId,
	);
	const candidates =
		unavailableIndex >= 0 ? breadcrumb.slice(0, unavailableIndex) : breadcrumb;
	return candidates.at(-1) ?? { id: null, name: "Root" };
}
