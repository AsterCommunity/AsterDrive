import { withQuery } from "@/lib/queryParams";
import type {
	WebdavAccountCreated,
	WebdavAccountInfo,
	WebdavAccountListQuery,
	WebdavAccountPage,
	WebdavSettingsInfo,
} from "@/types/api";
import { api } from "./http";

export const webdavAccountService = {
	settings: () => api.get<WebdavSettingsInfo>("/webdav-accounts/settings"),

	list: (params?: WebdavAccountListQuery) =>
		api.get<WebdavAccountPage>(
			withQuery("/webdav-accounts", {
				limit: params?.limit,
				offset: params?.offset,
			}),
		),

	create: (username: string, password?: string, rootFolderId?: number) =>
		api.post<WebdavAccountCreated>("/webdav-accounts", {
			username,
			password,
			root_folder_id: rootFolderId ?? null,
		}),

	delete: (id: number) => api.delete<void>(`/webdav-accounts/${id}`),

	toggle: (id: number) =>
		api.post<WebdavAccountInfo>(`/webdav-accounts/${id}/toggle`),

	test: (username: string, password: string) =>
		api.post<void>("/webdav-accounts/test", { username, password }),
};
