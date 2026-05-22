import { api } from "@/services/http";
import type { PublicMediaDataSupport } from "@/types/api";

export const mediaDataSupportService = {
	get: () => api.get<PublicMediaDataSupport>("/public/media-data-support"),
};
