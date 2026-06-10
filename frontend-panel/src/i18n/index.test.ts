import type { ReportNamespaces } from "react-i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiErrorCode } from "@/types/api-helpers";

async function loadModule() {
	vi.resetModules();
	return (await import("@/i18n")).default;
}

async function loadI18nModule() {
	vi.resetModules();
	return import("@/i18n");
}

describe("i18n", () => {
	beforeEach(() => {
		localStorage.clear();
	});

	it("binds resource store additions so async bundles can refresh current pages", async () => {
		const i18n = await loadModule();

		expect(i18n.options.react?.bindI18nStore).toBe("added");
	});

	it("keeps non-login namespaces out of the startup locale graph", async () => {
		localStorage.setItem("aster-language", "zh");
		const i18n = await loadModule();

		expect(i18n.hasResourceBundle("zh", "core")).toBe(true);
		expect(i18n.hasResourceBundle("zh", "login")).toBe(true);
		expect(i18n.hasResourceBundle("zh", "auth")).toBe(false);
		expect(i18n.getResource("zh", "login", "passkey_sign_in")).toBe(
			"使用 Passkey 登录",
		);
		expect(i18n.hasResourceBundle("zh", "admin")).toBe(false);
		expect(i18n.hasResourceBundle("zh", "settings")).toBe(false);
		expect(i18n.hasResourceBundle("zh", "files")).toBe(false);
		expect(i18n.hasResourceBundle("zh", "share")).toBe(false);
		expect(i18n.hasResourceBundle("zh", "tasks")).toBe(false);
	});

	it("loads already used namespaces before resolving a language switch", async () => {
		localStorage.setItem("aster-language", "zh");
		const i18n = await loadModule();

		i18n.reportNamespaces = {
			addUsedNamespaces: () => undefined,
			getUsedNamespaces: () => ["settings"],
		} satisfies ReportNamespaces;
		i18n.removeResourceBundle("en", "settings");

		await i18n.changeLanguage("en");

		expect(i18n.language).toBe("en");
		expect(i18n.hasResourceBundle("en", "settings")).toBe(true);
	});

	it("merges split locale files into their original namespaces", async () => {
		localStorage.setItem("aster-language", "zh");
		const module = await loadI18nModule();
		const i18n = module.default;

		await module.ensureI18nNamespaces(["admin", "files", "settings"], "zh");

		expect(i18n.t("files:upload_success")).toBe("上传完成");
		expect(i18n.t("files:archive_preview_title")).toBe("压缩包内容");
		expect(i18n.t("settings:settings_passkeys_section")).toBe("Passkey");
		expect(i18n.t("admin:overview_total_users")).toBe("总用户数");
		expect(i18n.t("admin:preview_apps_provider_archive")).toBe("压缩包");
		expect(i18n.exists("errors:auth_registration_disabled")).toBe(true);
		expect(i18n.t("errors:auth_registration_disabled")).toBe(
			"当前系统已关闭公开注册",
		);
	});

	it("keeps unsplit locale files loadable", async () => {
		localStorage.setItem("aster-language", "en");
		const module = await loadI18nModule();
		const i18n = module.default;

		await module.ensureI18nNamespaces(["webdav"], "en");

		expect(i18n.t("webdav:webdav_endpoint")).toBe("WebDAV Endpoint");
	});

	it.each([
		"en",
		"zh",
	] as const)("includes translated error messages for auth API codes in %s", async (language) => {
		localStorage.setItem("aster-language", language);
		const module = await loadI18nModule();
		const i18n = module.default;

		for (const code of Object.values(ApiErrorCode)) {
			if (!code.startsWith("auth.")) continue;

			const key = `errors:${code.replaceAll(".", "_")}`;
			expect(i18n.exists(key), key).toBe(true);
		}
	});
});
