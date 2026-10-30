import { withQuery } from "@/lib/queryParams";
import { api } from "@/services/http";
import type {
	AdminContentAccessInfo,
	AdminContentAccessStatus,
	AdminDownloadInfo,
	AdminDownloadInitRequest,
	AdminWorkspaceContents,
	CreateAdminContentAccess,
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
	initDownload: (fileId: number, request: AdminDownloadInitRequest) =>
		api.post<AdminDownloadInfo>(
			`/admin/files/${fileId}/download-init`,
			request,
		),
};
