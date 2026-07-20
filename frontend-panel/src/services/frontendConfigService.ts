import { api } from "@/services/http";
import type { PublicFrontendConfig } from "@/types/api";

export const frontendConfigService = {
	get: (options?: { cacheBust?: number }) => {
		if (options?.cacheBust === undefined) {
			return api.get<PublicFrontendConfig>("/public/frontend-config");
		}
		return api.get<PublicFrontendConfig>("/public/frontend-config", {
			params: { _: options.cacheBust },
		});
	},
};
