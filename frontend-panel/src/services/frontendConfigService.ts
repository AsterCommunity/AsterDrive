import { api } from "@/services/http";
import type { PublicFrontendConfig } from "@/types/api";

export const frontendConfigService = {
	get: () => api.get<PublicFrontendConfig>("/public/frontend-config"),
};
