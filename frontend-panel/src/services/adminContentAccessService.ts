import { withQuery } from "@/lib/queryParams";
import { api } from "@/services/http";
import type {
	AdminContentAccessInfo,
	AdminContentAccessStatus,
	AdminDownloadInfo,
	AdminDownloadInitRequest,
	AdminWorkspaceContents,
	CreateAdminContentAccess,
	FileInfo,
	FileVersion,
	operations,
} from "@/types/api";

export type AdminWorkspaceParams = NonNullable<
	operations["admin_workspace_contents"]["parameters"]["query"]
>;
export const adminContentAccessService = {
	status: () =>
		api.get<AdminContentAccessStatus>("/admin/content-access/status"),
	create: (request: CreateAdminContentAccess) =>
		api.post<AdminContentAccessInfo>("/admin/content-access", request),
	list: (params: AdminWorkspaceParams) =>
		api.get<AdminWorkspaceContents>(
			withQuery("/admin/content-access/workspace", params),
		),
	getFile: (accessId: string, fileId: number) =>
		api.get<FileInfo>(
			withQuery(`/admin/content-access/files/${fileId}`, {
				access_id: accessId,
			}),
		),
	listVersions: async (
		accessId: string,
		fileId: number,
		afterSequence?: number,
	) => {
		const items = await api.get<FileVersion[]>(
			withQuery(`/admin/content-access/files/${fileId}/versions`, {
				access_id: accessId,
				after_sequence: afterSequence,
				limit: 100,
			}),
		);
		return {
			items,
			nextAfterSequence:
				items.length === 100 ? (items.at(-1)?.version ?? null) : null,
		};
	},
	initDownload: (fileId: number, request: AdminDownloadInitRequest) =>
		api.post<AdminDownloadInfo>(
			`/admin/files/${fileId}/download-init`,
			request,
		),
};
