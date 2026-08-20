import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiErrorCode } from "@/types/api-helpers";

const FRONTEND_SOURCE_MODULES = import.meta.glob<string>(
	[
		"../**/*.ts",
		"../**/*.tsx",
		"!../**/*.test.ts",
		"!../**/*.test.tsx",
		"!../test/**",
	],
	{ eager: true, import: "default", query: "?raw" },
);

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
		expect(i18n.t("auth:go_to_login")).toBe("去登录");
		expect(i18n.t("auth:storage_setup_state_load_failed_title")).toBe(
			"初始化状态读取失败",
		);
		expect(i18n.t("auth:storage_setup_state_load_failed_desc")).toBe(
			"暂时没有取得系统初始化状态。检查网络连接后重新读取。",
		);
		expect(i18n.t("auth:storage_setup_retry_state")).toBe("重新读取状态");
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

	it("covers every literal frontend translation call", async () => {
		localStorage.setItem("aster-language", "en");
		const module = await loadI18nModule();
		const i18n = module.default;
		await module.ensureAllI18nNamespaces("en");

		const missing = new Set<string>();
		const literalTranslationCall = /\bt\(\s*["']([^"']+)["']/g;
		for (const [path, source] of Object.entries(FRONTEND_SOURCE_MODULES)) {
			for (const match of source.matchAll(literalTranslationCall)) {
				const key = match[1];
				const candidates = key.includes(":")
					? [key]
					: module.ALL_NAMESPACES.map((namespace) => `${namespace}:${key}`);
				const exists = candidates.some(
					(candidate) =>
						i18n.exists(candidate, { count: 1, lng: "en" }) ||
						i18n.exists(candidate, { count: 2, lng: "en" }),
				);
				if (!exists) missing.add(`${key} (${path})`);
			}
		}

		expect([...missing].sort()).toEqual([]);
	});

	it("keeps bundled locale key sets aligned", async () => {
		localStorage.setItem("aster-language", "en");
		const module = await loadI18nModule();
		const i18n = module.default;
		await Promise.all(
			module.BUNDLED_LOCALES.map((language) =>
				module.ensureAllI18nNamespaces(language),
			),
		);

		for (const namespace of module.ALL_NAMESPACES) {
			const englishKeys = Object.keys(
				i18n.getResourceBundle("en", namespace),
			).sort();
			const chineseKeys = Object.keys(
				i18n.getResourceBundle("zh", namespace),
			).sort();
			expect(chineseKeys, namespace).toEqual(englishKeys);
		}
	});

	it.each([
		{
			language: "en" as const,
			expected: {
				connectionPrompt:
					"The connection test failed. Save this policy anyway?",
				fieldRequired: "Remote node is required.",
				passwordReset:
					"Password reset successfully. You can sign in with your new password.",
				yes: "Yes",
			},
		},
		{
			language: "zh" as const,
			expected: {
				connectionPrompt: "连接测试失败，仍要保存此策略吗？",
				fieldRequired: "必须填写远程节点。",
				passwordReset: "密码已重置成功，现在可以使用新密码登录。",
				yes: "是",
			},
		},
	])(
		"provides restored frontend translations in $language",
		async ({ language, expected }) => {
			localStorage.setItem("aster-language", language);
			const module = await loadI18nModule();
			const i18n = module.default;

			await module.ensureI18nNamespaces(["admin", "login"], language);

			expect(i18n.t("admin:connection_test_failed_save_prompt")).toBe(
				expected.connectionPrompt,
			);
			expect(
				i18n.t("admin:policy_connector_field_required", {
					field: language === "zh" ? "远程节点" : "Remote node",
				}),
			).toBe(expected.fieldRequired);
			expect(i18n.t("login:password_reset_success_login")).toBe(
				expected.passwordReset,
			);
			expect(i18n.t("core:yes")).toBe(expected.yes);
		},
	);

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

	it.each([
		{
			language: "zh" as const,
			labels: {
				thumbnail: "使用存储后端生成图片缩略图",
				thumbnailExtensions: "存储原生图片缩略图后缀",
				media: "使用存储后端解析音视频媒体信息",
				mediaExtensions: "存储原生音视频媒体信息后缀",
			},
			descriptionTerms: {
				storage_native_thumbnail_enabled_desc: [
					"存储后端",
					"图片",
					"全局缩略图处理链",
					"休眠配置",
					"计费",
				],
				storage_native_thumbnail_extensions_desc: [
					"图片",
					"匹配这些后缀",
					"列表为空",
					"关闭时列表仍会保存",
				],
				storage_native_media_metadata_enabled_desc: [
					"存储后端",
					"音视频",
					"全局媒体信息处理链",
					"休眠配置",
					"计费",
				],
				storage_native_media_metadata_extensions_desc: [
					"音视频",
					"匹配这些后缀",
					"列表为空",
					"关闭时列表仍会保存",
				],
			},
		},
		{
			language: "en" as const,
			labels: {
				thumbnail: "Use the storage backend to generate image thumbnails",
				thumbnailExtensions: "Storage-native image thumbnail extensions",
				media: "Use the storage backend to parse audio/video media information",
				mediaExtensions:
					"Storage-native audio/video media-information extensions",
			},
			descriptionTerms: {
				storage_native_thumbnail_enabled_desc: [
					"storage backend",
					"images",
					"global thumbnail processor chain",
					"dormant",
					"charge",
				],
				storage_native_thumbnail_extensions_desc: [
					"images with matching extensions",
					"empty list",
					"remains saved while disabled",
				],
				storage_native_media_metadata_enabled_desc: [
					"storage backend",
					"audio or video",
					"global media-information processor chain",
					"dormant",
					"charge",
				],
				storage_native_media_metadata_extensions_desc: [
					"audio or video with matching extensions",
					"empty list",
					"remains saved while disabled",
				],
			},
		},
	])(
		"provides explicit storage-native policy labels and detailed help in $language",
		async ({ language, labels, descriptionTerms }) => {
			localStorage.setItem("aster-language", language);
			const module = await loadI18nModule();
			const i18n = module.default;
			await module.ensureI18nNamespaces(["admin"], language);

			expect(i18n.t("admin:storage_native_thumbnail_enabled")).toBe(
				labels.thumbnail,
			);
			expect(i18n.t("admin:storage_native_thumbnail_extensions")).toBe(
				labels.thumbnailExtensions,
			);
			expect(i18n.t("admin:storage_native_media_metadata_enabled")).toBe(
				labels.media,
			);
			expect(i18n.t("admin:storage_native_media_metadata_extensions")).toBe(
				labels.mediaExtensions,
			);

			for (const [key, terms] of Object.entries(descriptionTerms)) {
				const description = i18n.t(`admin:${key}`).toLowerCase();
				for (const term of terms) {
					expect(description, key).toContain(term.toLowerCase());
				}
			}
		},
	);

	it("keeps unsplit locale files loadable", async () => {
		localStorage.setItem("aster-language", "en");
		const module = await loadI18nModule();
		const i18n = module.default;

		await module.ensureI18nNamespaces(["webdav"], "en");

		expect(i18n.t("webdav:webdav_endpoint")).toBe("WebDAV Endpoint");
	});

	it.each(["en", "zh"] as const)(
		"includes translated error messages for auth API codes in %s",
		async (language) => {
			localStorage.setItem("aster-language", language);
			const module = await loadI18nModule();
			const i18n = module.default;

			for (const code of Object.values(ApiErrorCode)) {
				if (!code.startsWith("auth.")) continue;

				const key = `errors:${code.replaceAll(".", "_")}`;
				expect(i18n.exists(key), key).toBe(true);
			}
		},
	);

	it.each(["en", "zh"] as const)(
		"includes translated error messages for granular backend API codes in %s",
		async (language) => {
			localStorage.setItem("aster-language", language);
			const module = await loadI18nModule();
			const i18n = module.default;

			await module.ensureI18nNamespaces(["errors"], language);

			const granularCodes = [
				ApiErrorCode.OperationResourceLimitExceeded,
				ApiErrorCode.ConfigPublicSiteUrlRequired,
				ApiErrorCode.ConfigPublicSiteUrlInvalid,
				ApiErrorCode.ExternalAuthCallbackRedirectUriRequired,
				ApiErrorCode.PolicyStorageAccessKeyRequired,
				ApiErrorCode.PolicyStorageSecretKeyRequired,
				ApiErrorCode.PolicyStorageBucketRequired,
				ApiErrorCode.PolicyStorageEndpointInvalid,
				ApiErrorCode.PolicyRemoteNodeRequired,
				ApiErrorCode.PolicyRemoteNodeUnexpected,
				ApiErrorCode.PolicyRemoteStorageTargetRequired,
				ApiErrorCode.RemoteStorageTargetNotFound,
				ApiErrorCode.PolicyRemoteNodeDisabled,
				ApiErrorCode.PolicyRemoteNodeBaseUrlRequired,
				ApiErrorCode.PolicyRemoteNodeTransferStrategyUnsupported,
				ApiErrorCode.PolicyOneDriveOptionsUnsupported,
				ApiErrorCode.PolicySftpOptionsUnsupported,
				ApiErrorCode.PolicyOneDriveAccountModeRequired,
				ApiErrorCode.PolicyOneDrivePersonalChinaCloudUnsupported,
				ApiErrorCode.PolicyOneDriveSharepointSiteRequired,
				ApiErrorCode.PolicyOneDriveGroupRequired,
				ApiErrorCode.PolicyNativeThumbnailUnsupported,
				ApiErrorCode.PolicyNativeMediaMetadataUnsupported,
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
			] satisfies readonly string[];

			for (const code of granularCodes) {
				const key = `errors:${code.replaceAll(".", "_")}`;
				expect(i18n.exists(key), key).toBe(true);
			}
		},
	);
});
