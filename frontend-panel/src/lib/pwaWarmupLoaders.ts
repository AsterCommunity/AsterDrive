export interface WarmupLoaderEntry {
	key: string;
	label: string;
	load: () => Promise<unknown>;
}

const authenticatedShellI18nWarmupLoader = {
	key: "i18n:authenticated-shell",
	label: "AuthenticatedShellI18n",
	load: () =>
		import("@/i18n").then((module) =>
			module.ensureAuthenticatedShellI18nNamespaces(),
		),
} satisfies WarmupLoaderEntry;

const loginRouteWarmupLoader = {
	key: "route:login",
	label: "LoginPage",
	load: () => import("@/pages/LoginPage"),
} satisfies WarmupLoaderEntry;

const fileBrowserRouteWarmupLoader = {
	key: "route:file-browser",
	label: "FileBrowserPage",
	load: () => import("@/pages/FileBrowserPage"),
} satisfies WarmupLoaderEntry;

export const loginSuccessPathWarmupLoaders = [
	authenticatedShellI18nWarmupLoader,
	{
		key: "route:file-browser-entry",
		label: "FileBrowserPage",
		load: () => import("@/pages/FileBrowserPage"),
	},
] satisfies WarmupLoaderEntry[];

export const userRouteWarmupLoaders = [
	loginRouteWarmupLoader,
	{
		key: "route:error",
		label: "ErrorPage",
		load: () => import("@/pages/ErrorPage"),
	},
	fileBrowserRouteWarmupLoader,
	{
		key: "route:category-browser",
		label: "CategoryBrowserPage",
		load: () => import("@/pages/CategoryBrowserPage"),
	},
	{
		key: "route:search-browser",
		label: "SearchBrowserPage",
		load: () => import("@/pages/SearchBrowserPage"),
	},
	{
		key: "route:my-shares",
		label: "MySharesPage",
		load: () => import("@/pages/MySharesPage"),
	},
	{
		key: "route:tasks",
		label: "TasksPage",
		load: () => import("@/pages/TasksPage"),
	},
	{
		key: "route:trash",
		label: "TrashPage",
		load: () => import("@/pages/TrashPage"),
	},
	{
		key: "route:settings",
		label: "SettingsPage",
		load: () => import("@/pages/SettingsPage"),
	},
	{
		key: "route:webdav-accounts",
		label: "WebdavAccountsPage",
		load: () => import("@/pages/WebdavAccountsPage"),
	},
	{
		key: "route:team-manage",
		label: "TeamManagePage",
		load: () => import("@/pages/TeamManagePage"),
	},
	{
		key: "route:force-password-change",
		label: "ForcePasswordChangePage",
		load: () => import("@/pages/ForcePasswordChangePage"),
	},
	{
		key: "route:reset-password",
		label: "ResetPasswordPage",
		load: () => import("@/pages/ResetPasswordPage"),
	},
	{
		key: "route:invite-register",
		label: "InviteRegisterPage",
		load: () => import("@/pages/InviteRegisterPage"),
	},
	{
		key: "route:share-view",
		label: "ShareViewPage",
		load: () => import("@/pages/ShareViewPage"),
	},
] satisfies WarmupLoaderEntry[];

export const adminRouteWarmupLoaders = [
	{
		key: "route:admin-overview",
		label: "AdminOverviewPage",
		load: () => import("@/pages/admin/AdminOverviewPage"),
	},
	{
		key: "route:admin-users",
		label: "AdminUsersPage",
		load: () => import("@/pages/admin/AdminUsersPage"),
	},
	{
		key: "route:admin-user-invitations",
		label: "AdminUserInvitationsPage",
		load: () => import("@/pages/admin/AdminUserInvitationsPage"),
	},
	{
		key: "route:admin-teams",
		label: "AdminTeamsPage",
		load: () => import("@/pages/admin/AdminTeamsPage"),
	},
	{
		key: "route:admin-team-detail",
		label: "AdminTeamDetailPage",
		load: () => import("@/pages/admin/AdminTeamDetailPage"),
	},
	{
		key: "route:admin-policies",
		label: "AdminPoliciesPage",
		load: () => import("@/pages/admin/AdminPoliciesPage"),
	},
	{
		key: "route:admin-remote-nodes",
		label: "AdminRemoteNodesPage",
		load: () => import("@/pages/admin/AdminRemoteNodesPage"),
	},
	{
		key: "route:admin-external-auth",
		label: "AdminExternalAuthPage",
		load: () => import("@/pages/admin/AdminExternalAuthPage"),
	},
	{
		key: "route:admin-policy-groups",
		label: "AdminPolicyGroupsPage",
		load: () => import("@/pages/admin/AdminPolicyGroupsPage"),
	},
	{
		key: "route:admin-tasks",
		label: "AdminTasksPage",
		load: () => import("@/pages/admin/AdminTasksPage"),
	},
	{
		key: "route:admin-settings",
		label: "AdminSettingsPage",
		load: () => import("@/pages/admin/AdminSettingsPage"),
	},
	{
		key: "route:admin-shares",
		label: "AdminSharesPage",
		load: () => import("@/pages/admin/AdminSharesPage"),
	},
	{
		key: "route:admin-files",
		label: "AdminFilesPage",
		load: () => import("@/pages/admin/AdminFilesPage"),
	},
	{
		key: "route:admin-locks",
		label: "AdminLocksPage",
		load: () => import("@/pages/admin/AdminLocksPage"),
	},
	{
		key: "route:admin-audit",
		label: "AdminAuditPage",
		load: () => import("@/pages/admin/AdminAuditPage"),
	},
	{
		key: "route:admin-about",
		label: "AdminAboutPage",
		load: () => import("@/pages/admin/AdminAboutPage"),
	},
] satisfies WarmupLoaderEntry[];

export const userFeatureWarmupLoaders = [
	{
		key: "feature:file-preview",
		label: "FilePreview",
		load: () => import("@/components/files/FilePreview"),
	},
	{
		key: "feature:language-icons",
		label: "LanguageIcons",
		load: () =>
			import("@/components/ui/language-icon").then((module) =>
				module.loadLanguageIcons(),
			),
	},
	{
		key: "feature:upload-area",
		label: "UploadArea",
		load: () => import("@/components/files/UploadArea"),
	},
	{
		key: "feature:share-dialog",
		label: "ShareDialog",
		load: () => import("@/components/files/ShareDialog"),
	},
	{
		key: "feature:file-info-dialog",
		label: "FileInfoDialog",
		load: () => import("@/components/files/FileInfoDialog"),
	},
	{
		key: "feature:rename-dialog",
		label: "RenameDialog",
		load: () => import("@/components/files/RenameDialog"),
	},
	{
		key: "feature:version-history-dialog",
		label: "VersionHistoryDialog",
		load: () => import("@/components/files/VersionHistoryDialog"),
	},
	{
		key: "feature:batch-target-folder-dialog",
		label: "BatchTargetFolderDialog",
		load: () => import("@/components/files/BatchTargetFolderDialog"),
	},
	{
		key: "feature:create-file-dialog",
		label: "CreateFileDialog",
		load: () => import("@/components/files/CreateFileDialog"),
	},
	{
		key: "feature:create-folder-dialog",
		label: "CreateFolderDialog",
		load: () => import("@/components/files/CreateFolderDialog"),
	},
] satisfies WarmupLoaderEntry[];

export const filePreviewWarmupLoaders = [
	{
		key: "preview:text-code",
		label: "TextCodePreview",
		load: () => import("@/components/files/preview/TextCodePreview"),
	},
	{
		key: "preview:json",
		label: "JsonPreview",
		load: () => import("@/components/files/preview/JsonPreview"),
	},
	{
		key: "preview:xml",
		label: "XmlPreview",
		load: () => import("@/components/files/preview/XmlPreview"),
	},
	{
		key: "preview:csv",
		label: "CsvTablePreview",
		load: () => import("@/components/files/preview/CsvTablePreview"),
	},
	{
		key: "preview:markdown",
		label: "MarkdownPreview",
		load: () => import("@/components/files/preview/MarkdownPreview"),
	},
	{
		key: "preview:pdf",
		label: "PdfPreview",
		load: () => import("@/components/files/preview/PdfPreview"),
	},
] satisfies WarmupLoaderEntry[];
