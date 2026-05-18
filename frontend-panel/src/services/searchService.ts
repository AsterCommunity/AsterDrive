import { withQuery } from "@/lib/queryParams";
import {
	buildWorkspacePath,
	PERSONAL_WORKSPACE,
	type Workspace,
} from "@/lib/workspace";
import { type ApiRequestConfig, api } from "@/services/http";
import { bindWorkspaceService } from "@/stores/workspaceStore";
import type { FileCategory, SearchParams, SearchResults } from "@/types/api";

type SearchRequestOptions = Pick<ApiRequestConfig, "signal">;
export type FileSearchQuery = Omit<SearchParams, "category" | "extensions"> & {
	category?: FileCategory | null;
	extensions?: string[] | null;
};

type SearchRequestParams = SearchParams | FileSearchQuery;

function normalizeSearchParams(params: SearchRequestParams): SearchParams {
	const extensions = Array.isArray(params.extensions)
		? params.extensions
				.map((extension) => extension.trim().replace(/^\./, "").toLowerCase())
				.filter(Boolean)
				.join(",")
		: params.extensions;

	return {
		...params,
		extensions: extensions || null,
	};
}

export function createSearchService(workspace: Workspace = PERSONAL_WORKSPACE) {
	return {
		search: (params: SearchRequestParams, options?: SearchRequestOptions) =>
			api.get<SearchResults>(
				withQuery(
					buildWorkspacePath(workspace, "/search"),
					normalizeSearchParams(params),
				),
				options,
			),
	};
}

export const searchService = bindWorkspaceService(createSearchService);
