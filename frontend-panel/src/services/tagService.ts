import { buildWorkspacePath, type Workspace } from "@/lib/workspace";
import { type ApiRequestConfig, api } from "@/services/http";
import { bindWorkspaceService } from "@/stores/workspaceStore";
import type {
	BatchTagBindingRequest,
	CreateTagRequest,
	EntityTags,
	EntityType,
	PatchTagRequest,
	ReplaceEntityTagsRequest,
	TagInfo,
	TagListParams,
	TagPage,
	TagSummary,
} from "@/types/api";

type TagRequestOptions = Pick<ApiRequestConfig, "signal">;
type TagListOptions = TagRequestOptions & {
	params?: TagListParams;
};

function entityPath(entityType: EntityType, entityId: number) {
	return `${entityType}/${entityId}`;
}

export function createTagService(workspace: Workspace) {
	return {
		listTags: (options?: TagListOptions) =>
			api.get<TagPage>(buildWorkspacePath(workspace, "/tags"), options),

		createTag: (request: CreateTagRequest) =>
			api.post<TagInfo>(buildWorkspacePath(workspace, "/tags"), request),

		patchTag: (tagId: number, request: PatchTagRequest) =>
			api.patch<TagInfo>(
				buildWorkspacePath(workspace, `/tags/${tagId}`),
				request,
			),

		deleteTag: (tagId: number) =>
			api.delete<void>(buildWorkspacePath(workspace, `/tags/${tagId}`)),

		listEntityTags: (
			entityType: EntityType,
			entityId: number,
			options?: TagRequestOptions,
		) =>
			api.get<EntityTags>(
				buildWorkspacePath(
					workspace,
					`/tags/${entityPath(entityType, entityId)}`,
				),
				options,
			),

		replaceEntityTags: (
			entityType: EntityType,
			entityId: number,
			tagIds: number[],
		) =>
			api.put<EntityTags>(
				buildWorkspacePath(
					workspace,
					`/tags/${entityPath(entityType, entityId)}`,
				),
				{ tag_ids: tagIds } satisfies ReplaceEntityTagsRequest,
			),

		attachTag: (tagId: number, entityType: EntityType, entityId: number) =>
			api.put<TagSummary>(
				buildWorkspacePath(
					workspace,
					`/tags/${tagId}/${entityPath(entityType, entityId)}`,
				),
			),

		detachTag: (tagId: number, entityType: EntityType, entityId: number) =>
			api.delete<void>(
				buildWorkspacePath(
					workspace,
					`/tags/${tagId}/${entityPath(entityType, entityId)}`,
				),
			),

		batchAttachTag: (tagId: number, request: BatchTagBindingRequest) =>
			api.put<void>(
				buildWorkspacePath(workspace, `/tags/${tagId}/batch`),
				request,
			),

		batchDetachTag: (tagId: number, request: BatchTagBindingRequest) =>
			api.delete<void>(buildWorkspacePath(workspace, `/tags/${tagId}/batch`), {
				data: request,
			}),
	};
}

export const tagService = bindWorkspaceService(createTagService);
