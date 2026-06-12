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
		expect(i18n.getResource("zh", "login", "passkey_sign_in")).toBe(
			"使用 Passkey 登录",
		);
		expect(i18n.getResource("zh", "login", "back_to_sign_in")).toBe("返回登录");
		expect(i18n.getResource("zh", "auth", "login_success")).toBeUndefined();
		expect(
			i18n.getResource("zh", "admin", "overview_total_users"),
		).toBeUndefined();
		expect(
			i18n.getResource("zh", "settings", "settings_passkeys_section"),
		).toBeUndefined();
		expect(i18n.getResource("zh", "files", "upload_success")).toBeUndefined();
		expect(i18n.getResource("zh", "share", "my_shares_title")).toBeUndefined();
		expect(i18n.getResource("zh", "tasks", "title")).toBeUndefined();
	});

	it("loads all namespaces before resolving a language switch", async () => {
		localStorage.setItem("aster-language", "zh");
		const i18n = await loadModule();

		i18n.removeResourceBundle("en", "settings");
		i18n.removeResourceBundle("en", "files");
		i18n.removeResourceBundle("en", "admin");

		await i18n.changeLanguage("en");

		expect(i18n.language).toBe("en");
		expect(i18n.hasResourceBundle("en", "settings")).toBe(true);
		expect(i18n.hasResourceBundle("en", "files")).toBe(true);
		expect(i18n.hasResourceBundle("en", "admin")).toBe(true);
	});

	it("loads all namespaces on demand", async () => {
		localStorage.setItem("aster-language", "zh");
		const module = await loadI18nModule();
		const i18n = module.default;

		expect(i18n.getResource("zh", "files", "upload_success")).toBeUndefined();
		expect(
			i18n.getResource("zh", "admin", "overview_total_users"),
		).toBeUndefined();
		expect(i18n.getResource("zh", "share", "my_shares_title")).toBeUndefined();

		await module.ensureAllI18nNamespaces("zh");

		expect(i18n.t("files:upload_success")).toBe("上传完成");
		expect(i18n.t("admin:overview_total_users")).toBe("总用户数");
		expect(i18n.t("share:my_shares_title")).toBe("我的分享");
	});

	it("loads the authenticated shell namespaces without pulling admin settings", async () => {
		localStorage.setItem("aster-language", "zh");
		const module = await loadI18nModule();
		const i18n = module.default;

		await module.ensureAuthenticatedShellI18nNamespaces("zh");

		expect(i18n.t("files:upload_success")).toBe("上传完成");
		expect(i18n.t("tasks:title")).toBe("任务中心");
		expect(i18n.t("share:my_shares_title")).toBe("我的分享");
		expect(i18n.t("search:placeholder")).toBe("搜索文件和文件夹...");
		expect(
			i18n.getResource("zh", "admin", "overview_total_users"),
		).toBeUndefined();
		expect(
			i18n.getResource("zh", "settings", "settings_passkeys_section"),
		).toBeUndefined();
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

	it.each([
		"en",
		"zh",
	] as const)("includes translated error messages for granular backend API codes in %s", async (language) => {
		localStorage.setItem("aster-language", language);
		const module = await loadI18nModule();
		const i18n = module.default;

		await module.ensureI18nNamespaces(["errors"], language);

		const granularCodes = [
			ApiErrorCode.ConfigPublicSiteUrlRequired,
			ApiErrorCode.ConfigPublicSiteUrlInvalid,
			ApiErrorCode.ExternalAuthCallbackRedirectUriRequired,
			ApiErrorCode.PolicyStorageAccessKeyRequired,
			ApiErrorCode.PolicyStorageSecretKeyRequired,
			ApiErrorCode.PolicyStorageBucketRequired,
			ApiErrorCode.PolicyStorageEndpointInvalid,
			ApiErrorCode.PolicyRemoteNodeRequired,
			ApiErrorCode.PolicyRemoteNodeUnexpected,
			ApiErrorCode.PolicyRemoteNodeDisabled,
			ApiErrorCode.PolicyRemoteNodeBaseUrlRequired,
			ApiErrorCode.PolicyRemoteNodeTransferStrategyUnsupported,
			ApiErrorCode.PolicyNativeThumbnailUnsupported,
			ApiErrorCode.PolicyPromotionSourceUnsupported,
			ApiErrorCode.PolicyPromotionTargetUnsupported,
			ApiErrorCode.PolicyPromotionBucketChangeDenied,
			ApiErrorCode.ArchiveDownloadUserDisabled,
			ApiErrorCode.ArchiveDownloadShareDisabled,
			ApiErrorCode.TaskRetryStatusConflict,
			ApiErrorCode.TaskRetryNotAllowed,
			ApiErrorCode.SearchQueryEmpty,
			ApiErrorCode.SearchTypeInvalid,
			ApiErrorCode.SearchTagMatchInvalid,
			ApiErrorCode.SearchSizeRangeInvalid,
			ApiErrorCode.SearchFileFilterTypeConflict,
			ApiErrorCode.SearchMimeTypeEmpty,
			ApiErrorCode.SearchCategoryInvalid,
			ApiErrorCode.SearchExtensionsInvalid,
			ApiErrorCode.SearchTagIdsInvalid,
			ApiErrorCode.SearchDateInvalid,
			ApiErrorCode.SearchDateRangeInvalid,
			ApiErrorCode.InternalStorageRangeLengthInvalid,
			ApiErrorCode.InternalStorageRangeEmptyObject,
			ApiErrorCode.InternalStorageRangeOffsetOutOfBounds,
			ApiErrorCode.InternalStorageRangeHeaderInvalid,
			ApiErrorCode.InternalStorageRangeMultipleUnsupported,
			ApiErrorCode.InternalStorageRangeBoundsInvalid,
			ApiErrorCode.InternalStorageContentLengthRequired,
			ApiErrorCode.InternalStorageContentLengthInvalid,
			ApiErrorCode.InternalStorageComposePartsRequired,
			ApiErrorCode.InternalStorageComposeExpectedSizeInvalid,
		] satisfies readonly ApiErrorCode[];

		for (const code of granularCodes) {
			const key = `errors:${code.replaceAll(".", "_")}`;
			expect(i18n.exists(key), key).toBe(true);
		}
	});
});
