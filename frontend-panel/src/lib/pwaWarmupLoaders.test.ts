import { describe, expect, it, vi } from "vitest";
import {
	adminRouteWarmupLoaders,
	filePreviewWarmupLoaders,
	loginSuccessPathWarmupLoaders,
	userFeatureWarmupLoaders,
	userRouteWarmupLoaders,
} from "@/lib/pwaWarmupLoaders";

vi.mock("@/i18n", () => ({
	ensureAuthenticatedShellI18nNamespaces: vi.fn(() =>
		Promise.resolve("shell-i18n-loaded"),
	),
}));
vi.mock("@/pages/LoginPage", () => ({ default: "LoginPage" }));
vi.mock("@/pages/ErrorPage", () => ({ default: "ErrorPage" }));
vi.mock("@/pages/FileBrowserPage", () => ({ default: "FileBrowserPage" }));
vi.mock("@/pages/CategoryBrowserPage", () => ({
	default: "CategoryBrowserPage",
}));
vi.mock("@/pages/SearchBrowserPage", () => ({ default: "SearchBrowserPage" }));
vi.mock("@/pages/MySharesPage", () => ({ default: "MySharesPage" }));
vi.mock("@/pages/TasksPage", () => ({ default: "TasksPage" }));
vi.mock("@/pages/TrashPage", () => ({ default: "TrashPage" }));
vi.mock("@/pages/SettingsPage", () => ({ default: "SettingsPage" }));
vi.mock("@/pages/WebdavAccountsPage", () => ({
	default: "WebdavAccountsPage",
}));
vi.mock("@/pages/TeamManagePage", () => ({ default: "TeamManagePage" }));
vi.mock("@/pages/ForcePasswordChangePage", () => ({
	default: "ForcePasswordChangePage",
}));
vi.mock("@/pages/ResetPasswordPage", () => ({ default: "ResetPasswordPage" }));
vi.mock("@/pages/InviteRegisterPage", () => ({
	default: "InviteRegisterPage",
}));
vi.mock("@/pages/ShareViewPage", () => ({ default: "ShareViewPage" }));
vi.mock("@/pages/admin/AdminOverviewPage", () => ({
	default: "AdminOverviewPage",
}));
vi.mock("@/pages/admin/AdminUsersPage", () => ({ default: "AdminUsersPage" }));
vi.mock("@/pages/admin/AdminUserInvitationsPage", () => ({
	default: "AdminUserInvitationsPage",
}));
vi.mock("@/pages/admin/AdminTeamsPage", () => ({ default: "AdminTeamsPage" }));
vi.mock("@/pages/admin/AdminTeamDetailPage", () => ({
	default: "AdminTeamDetailPage",
}));
vi.mock("@/pages/admin/AdminPoliciesPage", () => ({
	default: "AdminPoliciesPage",
}));
vi.mock("@/pages/admin/AdminRemoteNodesPage", () => ({
	default: "AdminRemoteNodesPage",
}));
vi.mock("@/pages/admin/AdminExternalAuthPage", () => ({
	default: "AdminExternalAuthPage",
}));
vi.mock("@/pages/admin/AdminPolicyGroupsPage", () => ({
	default: "AdminPolicyGroupsPage",
}));
vi.mock("@/pages/admin/AdminTasksPage", () => ({
	default: "AdminTasksPage",
}));
vi.mock("@/pages/admin/AdminSettingsPage", () => ({
	default: "AdminSettingsPage",
}));
vi.mock("@/pages/admin/AdminSharesPage", () => ({
	default: "AdminSharesPage",
}));
vi.mock("@/pages/admin/AdminFilesPage", () => ({ default: "AdminFilesPage" }));
vi.mock("@/pages/admin/AdminLocksPage", () => ({ default: "AdminLocksPage" }));
vi.mock("@/pages/admin/AdminAuditPage", () => ({ default: "AdminAuditPage" }));
vi.mock("@/pages/admin/AdminAboutPage", () => ({ default: "AdminAboutPage" }));
vi.mock("@/components/files/FilePreview", () => ({
	FilePreview: "FilePreview",
}));
vi.mock("@/components/ui/language-icon", () => ({
	loadLanguageIcons: vi.fn(() => Promise.resolve("icons-loaded")),
}));
vi.mock("@/components/files/UploadArea", () => ({ UploadArea: "UploadArea" }));
vi.mock("@/components/files/ShareDialog", () => ({
	ShareDialog: "ShareDialog",
}));
vi.mock("@/components/files/FileInfoDialog", () => ({
	FileInfoDialog: "FileInfoDialog",
}));
vi.mock("@/components/files/RenameDialog", () => ({
	RenameDialog: "RenameDialog",
}));
vi.mock("@/components/files/VersionHistoryDialog", () => ({
	VersionHistoryDialog: "VersionHistoryDialog",
}));
vi.mock("@/components/files/BatchTargetFolderDialog", () => ({
	BatchTargetFolderDialog: "BatchTargetFolderDialog",
}));
vi.mock("@/components/files/CreateFileDialog", () => ({
	CreateFileDialog: "CreateFileDialog",
}));
vi.mock("@/components/files/CreateFolderDialog", () => ({
	CreateFolderDialog: "CreateFolderDialog",
}));
vi.mock("@/components/files/preview/TextCodePreview", () => ({
	TextCodePreview: "TextCodePreview",
}));
vi.mock("@/components/files/preview/JsonPreview", () => ({
	JsonPreview: "JsonPreview",
}));
vi.mock("@/components/files/preview/XmlPreview", () => ({
	XmlPreview: "XmlPreview",
}));
vi.mock("@/components/files/preview/CsvTablePreview", () => ({
	CsvTablePreview: "CsvTablePreview",
}));
vi.mock("@/components/files/preview/MarkdownPreview", () => ({
	MarkdownPreview: "MarkdownPreview",
}));
vi.mock("@/components/files/preview/PdfPreview", () => ({
	PdfPreview: "PdfPreview",
}));

describe("pwaWarmupLoaders", () => {
	const allQueues = [
		userRouteWarmupLoaders,
		adminRouteWarmupLoaders,
		loginSuccessPathWarmupLoaders,
		userFeatureWarmupLoaders,
		filePreviewWarmupLoaders,
	];

	it("defines stable keys for each warmup queue", () => {
		expect(userRouteWarmupLoaders.map((loader) => loader.key)).toEqual([
			"route:login",
			"route:error",
			"route:file-browser",
			"route:category-browser",
			"route:search-browser",
			"route:my-shares",
			"route:tasks",
			"route:trash",
			"route:settings",
			"route:webdav-accounts",
			"route:team-manage",
			"route:force-password-change",
			"route:reset-password",
			"route:invite-register",
			"route:share-view",
		]);
		expect(adminRouteWarmupLoaders.map((loader) => loader.key)).toEqual([
			"route:admin-overview",
			"route:admin-users",
			"route:admin-user-invitations",
			"route:admin-teams",
			"route:admin-team-detail",
			"route:admin-policies",
			"route:admin-remote-nodes",
			"route:admin-external-auth",
			"route:admin-policy-groups",
			"route:admin-tasks",
			"route:admin-settings",
			"route:admin-shares",
			"route:admin-files",
			"route:admin-locks",
			"route:admin-audit",
			"route:admin-about",
		]);
		expect(loginSuccessPathWarmupLoaders.map((loader) => loader.key)).toEqual([
			"i18n:authenticated-shell",
			"route:file-browser-entry",
		]);
		expect(userFeatureWarmupLoaders.map((loader) => loader.key)).toEqual([
			"feature:file-preview",
			"feature:language-icons",
			"feature:upload-area",
			"feature:share-dialog",
			"feature:file-info-dialog",
			"feature:rename-dialog",
			"feature:version-history-dialog",
			"feature:batch-target-folder-dialog",
			"feature:create-file-dialog",
			"feature:create-folder-dialog",
		]);
		expect(filePreviewWarmupLoaders.map((loader) => loader.key)).toEqual([
			"preview:text-code",
			"preview:json",
			"preview:xml",
			"preview:csv",
			"preview:markdown",
			"preview:pdf",
		]);
	});

	it("keeps every loader entry unique and executable", async () => {
		const entries = allQueues.flat();
		const keys = entries.map((loader) => loader.key);

		expect(new Set(keys).size).toBe(entries.length);
		for (const loader of entries) {
			expect(loader.label).toMatch(/\S/);
			expect(loader.load).toEqual(expect.any(Function));
		}

		await expect(
			Promise.all(entries.map((loader) => loader.load())),
		).resolves.toHaveLength(entries.length);
	});

	it("loads language icon data through the feature loader", async () => {
		const result = await userFeatureWarmupLoaders
			.find((loader) => loader.key === "feature:language-icons")
			?.load();

		expect(result).toBe("icons-loaded");
	});
});
