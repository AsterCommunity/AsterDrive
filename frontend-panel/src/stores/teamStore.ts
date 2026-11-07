import { create } from "zustand";
import { logger } from "@/lib/logger";
import type { TeamInfo } from "@/types/api";

let inFlightEnsure: Promise<void> | null = null;
let requestedUserId: number | null = null;
// clear 与换号同时使已发送请求失效；仅清空数组不能阻止迟到响应回填。
let requestGeneration = 0;

interface TeamState {
	teams: TeamInfo[];
	loading: boolean;
	loadedForUserId: number | null;
	ensureLoaded: (userId: number | null) => Promise<void>;
	reload: (userId: number | null) => Promise<void>;
	invalidate: () => void;
	clear: () => void;
}

export const useTeamStore = create<TeamState>((set, get) => ({
	teams: [],
	loading: false,
	loadedForUserId: null,
	reload: async (userId) => {
		if (userId == null) {
			get().clear();
			return;
		}
		if (inFlightEnsure && requestedUserId === userId) {
			const queuedGeneration = requestGeneration;
			await inFlightEnsure;
			// reload 自己也可能排在旧请求后等待；清理后不得重新启动旧身份的请求。
			if (queuedGeneration !== requestGeneration) return;
		}
		const generation = ++requestGeneration;
		requestedUserId = userId;

		inFlightEnsure = (async () => {
			set({ teams: [], loadedForUserId: null, loading: true });
			try {
				const { teamService } = await import("@/services/teamService");
				if (generation !== requestGeneration) return;
				const teams = await teamService.list();
				if (generation === requestGeneration) {
					set({ teams, loading: false, loadedForUserId: userId });
				}
			} catch (error) {
				if (generation !== requestGeneration) return;
				logger.warn("Failed to load teams", error);
				set({ teams: [], loading: false, loadedForUserId: null });
				throw error;
			} finally {
				// 旧请求 finally 不能拆掉新账号正在使用的去重 promise。
				if (generation === requestGeneration) {
					inFlightEnsure = null;
				}
			}
		})();

		return inFlightEnsure;
	},
	ensureLoaded: async (userId) => {
		if (userId == null) {
			get().clear();
			return;
		}
		if (get().loadedForUserId === userId) return;
		if (inFlightEnsure && requestedUserId === userId) return inFlightEnsure;
		return get().reload(userId);
	},
	invalidate: () => {
		set({ loadedForUserId: null });
	},
	clear: () => {
		requestGeneration += 1;
		requestedUserId = null;
		inFlightEnsure = null;
		set({ teams: [], loading: false, loadedForUserId: null });
	},
}));
