import type { ApiResponse, FolderContents, MeResponse } from "@/types/api";
import { ErrorCode } from "@/types/api-helpers";

export function apiResponse<T>(data: T, message = "ok"): ApiResponse<T> {
	return {
		code: ErrorCode.Success,
		msg: message,
		data,
	};
}

export function createMeResponse(
	overrides: Partial<MeResponse> = {},
): MeResponse {
	return {
		id: 1,
		username: "alice",
		email: "alice@example.com",
		email_verified: true,
		pending_email: null,
		role: "user",
		status: "active",
		policy_group_id: null,
		storage_used: 0,
		storage_quota: 0,
		access_token_expires_at: Math.floor(Date.now() / 1000) + 900,
		created_at: "2026-04-08T00:00:00Z",
		updated_at: "2026-04-08T00:00:00Z",
		profile: {
			avatar: {
				source: "none",
				url_512: null,
				url_1024: null,
				version: 0,
			},
		},
		preferences: {
			theme_mode: "dark",
			color_preset: "#f97316",
			view_mode: "grid",
			browser_open_mode: "double_click",
			sort_by: "updated_at",
			sort_order: "desc",
			language: "zh",
			display_time_zone: "Asia/Shanghai",
			storage_event_stream_enabled: true,
		},
		...overrides,
	} as MeResponse;
}

export function createFolderContents(
	overrides: Partial<FolderContents> = {},
): FolderContents {
	return {
		folders: [],
		files: [],
		folders_total: 0,
		files_total: 0,
		next_file_cursor: null,
		...overrides,
	} as FolderContents;
}
