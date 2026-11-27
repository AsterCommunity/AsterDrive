import { withQuery } from "@/lib/queryParams";
import type {
	CreateWebdavAccountRequest,
	TeamWebdavAccountListQuery,
	TestWebdavConnectionRequest,
	WebdavAccessMode,
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
		api.get<WebdavAccountPage>(withQuery("/webdav-accounts", params)),

	listForTeam: (teamId: number, params?: TeamWebdavAccountListQuery) =>
		api.get<WebdavAccountPage>(
			withQuery(`/teams/${teamId}/webdav-accounts`, params),
		),

	create: (data: CreateWebdavAccountRequest) =>
		api.post<WebdavAccountCreated>("/webdav-accounts", data),

	createForTeam: (teamId: number, data: CreateWebdavAccountRequest) =>
		api.post<WebdavAccountCreated>(`/teams/${teamId}/webdav-accounts`, data),

	delete: (id: number) => api.delete<void>(`/webdav-accounts/${id}`),

	deleteForTeam: (teamId: number, id: number) =>
		api.delete<void>(`/teams/${teamId}/webdav-accounts/${id}`),

	toggle: (id: number) =>
		api.post<WebdavAccountInfo>(`/webdav-accounts/${id}/toggle`),

	toggleForTeam: (teamId: number, id: number) =>
		api.post<WebdavAccountInfo>(
			`/teams/${teamId}/webdav-accounts/${id}/toggle`,
		),

	setAccessMode: (id: number, accessMode: WebdavAccessMode) =>
		api.post<WebdavAccountInfo>(`/webdav-accounts/${id}/access-mode`, {
			access_mode: accessMode,
		}),

	setAccessModeForTeam: (
		teamId: number,
		id: number,
		accessMode: WebdavAccessMode,
	) =>
		api.post<WebdavAccountInfo>(
			`/teams/${teamId}/webdav-accounts/${id}/access-mode`,
			{ access_mode: accessMode },
		),

	test: (data: TestWebdavConnectionRequest) =>
		api.post<void>("/webdav-accounts/test", data),
};
