import { withQuery } from "@/lib/queryParams";
import { api } from "@/services/http";
import type {
	AddTeamMemberRequest,
	CreateTeamRequest,
	TeamAuditLogListQuery,
	TeamAuditPage,
	TeamInfo,
	TeamListQuery,
	TeamMemberListQuery,
	TeamMemberPage,
	UpdateTeamMemberRequest,
	UpdateTeamRequest,
} from "@/types/api";

export const teamService = {
	list: (params?: TeamListQuery) =>
		api.get<TeamInfo[]>(
			withQuery("/teams", {
				archived: params?.archived,
				keyword: params?.keyword,
				limit: params?.limit,
				offset: params?.offset,
			}),
		),
	get: (id: number) => api.get<TeamInfo>(`/teams/${id}`),
	create: (data: CreateTeamRequest) => api.post<TeamInfo>("/teams", data),
	update: (id: number, data: UpdateTeamRequest) =>
		api.patch<TeamInfo>(`/teams/${id}`, data),
	delete: (id: number) => api.delete<void>(`/teams/${id}`),
	restore: (id: number) => api.post<TeamInfo>(`/teams/${id}/restore`),
	listAuditLogs: (id: number, params: TeamAuditLogListQuery = {}) => {
		const { limit, offset, ...filters } = params;

		return api.get<TeamAuditPage>(
			withQuery(`/teams/${id}/audit-logs`, {
				limit,
				offset,
				...filters,
			}),
		);
	},
	listMembers: (id: number, params: TeamMemberListQuery = {}) => {
		const { limit, offset, ...filters } = params;

		return api.get<TeamMemberPage>(
			withQuery(`/teams/${id}/members`, {
				limit,
				offset,
				...filters,
			}),
		);
	},
	addMember: (id: number, data: AddTeamMemberRequest) =>
		api.post<TeamMemberPage["items"][number]>(`/teams/${id}/members`, data),
	updateMember: (
		id: number,
		memberUserId: number,
		data: UpdateTeamMemberRequest,
	) =>
		api.patch<TeamMemberPage["items"][number]>(
			`/teams/${id}/members/${memberUserId}`,
			data,
		),
	removeMember: (id: number, memberUserId: number) =>
		api.delete<void>(`/teams/${id}/members/${memberUserId}`),
};
