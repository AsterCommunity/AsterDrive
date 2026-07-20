import { beforeEach, describe, expect, it, vi } from "vitest";
import { frontendConfigService } from "@/services/frontendConfigService";

const apiGet = vi.hoisted(() => vi.fn());

vi.mock("@/services/http", () => ({
	api: {
		get: apiGet,
	},
}));

describe("frontendConfigService", () => {
	beforeEach(() => {
		apiGet.mockReset();
	});

	const frontendConfig = {
		branding: {
			allow_user_registration: true,
			description: "Private drive",
			favicon_url: "/favicon.ico",
			passkey_login_enabled: true,
			site_urls: ["https://drive.example"],
			title: "AsterDrive",
			wordmark_dark_url: "/wordmark-dark.svg",
			wordmark_light_url: "/wordmark-light.svg",
		},
		downloads: {
			archive_download_share_enabled: true,
			archive_download_user_enabled: true,
		},
		media: {
			image_preview_preference: "preview_first",
		},
		version: 2,
	};

	it("loads public frontend config from the public endpoint", async () => {
		apiGet.mockResolvedValue(frontendConfig);

		await expect(frontendConfigService.get()).resolves.toEqual(frontendConfig);

		expect(apiGet).toHaveBeenCalledWith("/public/frontend-config");
	});

	it("propagates network failures from the http client", async () => {
		const error = new Error("offline");
		apiGet.mockRejectedValue(error);

		await expect(frontendConfigService.get()).rejects.toThrow("offline");
		expect(apiGet).toHaveBeenCalledTimes(1);
	});

	it("cache-busts forced public config refreshes", async () => {
		apiGet.mockResolvedValue(frontendConfig);

		await frontendConfigService.get({ cacheBust: 1234 });

		expect(apiGet).toHaveBeenCalledWith("/public/frontend-config", {
			params: { _: 1234 },
		});
	});

	it("propagates non-success http responses as client rejections", async () => {
		const error = new Error("Forbidden");
		apiGet.mockRejectedValue(error);

		await expect(frontendConfigService.get()).rejects.toThrow("Forbidden");
		expect(apiGet).toHaveBeenCalledWith("/public/frontend-config");
	});

	it("does not cache or retry requests at the service layer", async () => {
		apiGet.mockResolvedValue(frontendConfig);

		await frontendConfigService.get();
		await frontendConfigService.get();

		expect(apiGet).toHaveBeenCalledTimes(2);
	});
});
