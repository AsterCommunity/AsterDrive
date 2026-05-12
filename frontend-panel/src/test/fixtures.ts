import type { FolderContents, MeResponse } from "@/types/api";
import { type ApiResponse, ErrorCode } from "@/types/api-helpers";

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
		role: "user",
		status: "active",
		access_token_expires_at: Math.floor(Date.now() / 1000) + 900,
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
