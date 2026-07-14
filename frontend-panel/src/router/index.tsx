import { type ComponentType, lazy, Suspense } from "react";
import { createBrowserRouter, Navigate } from "react-router-dom";
import { ensureI18nNamespaces, type LocaleNamespace } from "@/i18n";
import { logger } from "@/lib/logger";
import { AdminRoute } from "./AdminRoute";
import { Loading } from "./Loading";
import { LoginGuard } from "./LoginGuard";
import { PersonalWorkspaceRoute } from "./PersonalWorkspaceRoute";
import { ProtectedRoute } from "./ProtectedRoute";
import { TeamWorkspaceRoute } from "./TeamWorkspaceRoute";

function lazyPage<TProps extends object>(
	load: () => Promise<{ default: ComponentType<TProps> }>,
) {
	return lazy<ComponentType<TProps>>(load);
}

function localizedLazyPage<TProps extends object>(
	namespaces: readonly LocaleNamespace[],
	load: () => Promise<{ default: ComponentType<TProps> }>,
) {
	return lazyPage(async () => {
		try {
			await ensureI18nNamespaces(namespaces);
		} catch (error) {
			logger.warn("Failed to preload localized page namespaces", {
				error,
				namespaces,
			});
		}
		return load();
	});
}

const LoginPage = lazyPage(() => import("@/pages/LoginPage"));
const ForcePasswordChangePage = localizedLazyPage(
	["auth", "core", "settings", "validation"],
	() => import("@/pages/ForcePasswordChangePage"),
);
const ResetPasswordPage = localizedLazyPage(
	["auth", "core"],
	() => import("@/pages/ResetPasswordPage"),
);
const InviteRegisterPage = localizedLazyPage(
	["auth", "core"],
	() => import("@/pages/InviteRegisterPage"),
);
const FileBrowserPage = lazyPage(() => import("@/pages/FileBrowserPage"));
const CategoryBrowserPage = lazyPage(
	() => import("@/pages/CategoryBrowserPage"),
);
const SearchBrowserPage = lazyPage(() => import("@/pages/SearchBrowserPage"));
const AdminOverviewPage = lazyPage(
	() => import("@/pages/admin/AdminOverviewPage"),
);
const AdminUsersPage = lazyPage(() => import("@/pages/admin/AdminUsersPage"));
const AdminUserInvitationsPage = lazyPage(
	() => import("@/pages/admin/AdminUserInvitationsPage"),
);
const AdminTeamsPage = lazyPage(() => import("@/pages/admin/AdminTeamsPage"));
const AdminTeamDetailPage = localizedLazyPage(
	["admin", "core", "settings"],
	() => import("@/pages/admin/AdminTeamDetailPage"),
);
const AdminPoliciesPage = lazyPage(
	() => import("@/pages/admin/AdminPoliciesPage"),
);
const AdminRemoteNodesPage = lazyPage(
	() => import("@/pages/admin/AdminRemoteNodesPage"),
);
const AdminExternalAuthPage = lazyPage(
	() => import("@/pages/admin/AdminExternalAuthPage"),
);
const AdminPolicyGroupsPage = lazyPage(
	() => import("@/pages/admin/AdminPolicyGroupsPage"),
);
const AdminTasksPage = lazyPage(() => import("@/pages/admin/AdminTasksPage"));
const AdminSettingsPage = lazyPage(
	() => import("@/pages/admin/AdminSettingsPage"),
);
const AdminSharesPage = lazyPage(() => import("@/pages/admin/AdminSharesPage"));
const AdminFilesPage = lazyPage(() => import("@/pages/admin/AdminFilesPage"));
const AdminLocksPage = lazyPage(() => import("@/pages/admin/AdminLocksPage"));
const AdminAboutPage = lazyPage(() => import("@/pages/admin/AdminAboutPage"));
const ShareViewPage = localizedLazyPage(
	["core", "share", "files", "tasks", "errors"],
	() => import("@/pages/ShareViewPage"),
);
const WebdavAccountsPage = localizedLazyPage(
	["core", "admin", "auth", "webdav", "errors"],
	() => import("@/pages/WebdavAccountsPage"),
);
const TrashPage = localizedLazyPage(
	["core", "files", "admin", "tasks"],
	() => import("@/pages/TrashPage"),
);
const SettingsPage = localizedLazyPage(
	["core", "files", "settings", "auth", "admin"],
	() => import("@/pages/SettingsPage"),
);
const TeamManagePage = localizedLazyPage(
	["core", "settings", "admin", "webdav", "errors"],
	() => import("@/pages/TeamManagePage"),
);
const MySharesPage = lazyPage(() => import("@/pages/MySharesPage"));
const TasksPage = lazyPage(() => import("@/pages/TasksPage"));
const AdminAuditPage = lazyPage(() => import("@/pages/admin/AdminAuditPage"));
const ErrorPage = localizedLazyPage(
	["errors"],
	() => import("@/pages/ErrorPage"),
);

const errorElement = (
	<Suspense fallback={<Loading />}>
		<ErrorPage />
	</Suspense>
);

const shareViewElement = (
	<Suspense fallback={<Loading />}>
		<ShareViewPage />
	</Suspense>
);

export const router = createBrowserRouter([
	{
		element: <LoginGuard />,
		errorElement,
		children: [{ path: "/login", element: <LoginPage /> }],
	},
	{
		path: "/force-password-change",
		errorElement,
		element: (
			<Suspense fallback={<Loading />}>
				<ForcePasswordChangePage />
			</Suspense>
		),
	},
	{
		path: "/reset-password",
		errorElement,
		element: (
			<Suspense fallback={<Loading />}>
				<ResetPasswordPage />
			</Suspense>
		),
	},
	{
		path: "/invite/:token",
		errorElement,
		element: (
			<Suspense fallback={<Loading />}>
				<InviteRegisterPage />
			</Suspense>
		),
	},
	{
		element: <ProtectedRoute />,
		errorElement,
		children: [
			{
				element: <PersonalWorkspaceRoute />,
				children: [
					{ path: "/", element: <FileBrowserPage /> },
					{ path: "/folder/:folderId", element: <FileBrowserPage /> },
					{ path: "/category/:category", element: <CategoryBrowserPage /> },
					{ path: "/search", element: <SearchBrowserPage /> },
					{ path: "/shares", element: <MySharesPage /> },
					{ path: "/tasks", element: <TasksPage /> },
					{ path: "/trash", element: <TrashPage /> },
				],
			},
			{
				path: "/teams/:teamId",
				element: <TeamWorkspaceRoute />,
				children: [
					{ index: true, element: <FileBrowserPage /> },
					{ path: "folder/:folderId", element: <FileBrowserPage /> },
					{ path: "category/:category", element: <CategoryBrowserPage /> },
					{ path: "search", element: <SearchBrowserPage /> },
					{ path: "shares", element: <MySharesPage /> },
					{ path: "tasks", element: <TasksPage /> },
					{ path: "trash", element: <TrashPage /> },
				],
			},
			{ path: "/settings/webdav", element: <WebdavAccountsPage /> },
			{
				path: "/settings",
				element: <Navigate to="/settings/profile" replace />,
			},
			{
				path: "/settings/:section",
				element: <SettingsPage />,
			},
			{
				path: "/settings/teams/:teamId",
				element: <TeamManagePage />,
			},
			{
				path: "/settings/teams/:teamId/:section",
				element: <TeamManagePage />,
			},
		],
	},
	{
		// Public share page — no auth required
		path: "/s/:token",
		errorElement,
		element: shareViewElement,
	},
	{
		path: "/s/:token/folder/:folderId",
		errorElement,
		element: shareViewElement,
	},
	{
		element: <AdminRoute />,
		errorElement,
		children: [
			{ path: "/admin", element: <Navigate to="/admin/overview" replace /> },
			{ path: "/admin/overview", element: <AdminOverviewPage /> },
			{
				path: "/admin/users/invitations",
				element: <AdminUserInvitationsPage />,
			},
			{ path: "/admin/users", element: <AdminUsersPage /> },
			{ path: "/admin/teams", element: <AdminTeamsPage /> },
			{ path: "/admin/teams/:teamId", element: <AdminTeamDetailPage /> },
			{
				path: "/admin/teams/:teamId/:section",
				element: <AdminTeamDetailPage />,
			},
			{ path: "/admin/policies", element: <AdminPoliciesPage /> },
			{ path: "/admin/remote-nodes", element: <AdminRemoteNodesPage /> },
			{ path: "/admin/external-auth", element: <AdminExternalAuthPage /> },
			{ path: "/admin/policy-groups", element: <AdminPolicyGroupsPage /> },
			{ path: "/admin/shares", element: <AdminSharesPage /> },
			{ path: "/admin/files", element: <AdminFilesPage kind="files" /> },
			{
				path: "/admin/file-blobs",
				element: <AdminFilesPage kind="blobs" />,
			},
			{ path: "/admin/tasks", element: <AdminTasksPage /> },
			{ path: "/admin/locks", element: <AdminLocksPage /> },
			{
				path: "/admin/settings",
				element: <Navigate to="/admin/settings/site" replace />,
			},
			{
				path: "/admin/settings/site",
				element: <AdminSettingsPage section="site" />,
			},
			{
				path: "/admin/settings/auth",
				element: <AdminSettingsPage section="auth" />,
			},
			{
				path: "/admin/settings/mail",
				element: <AdminSettingsPage section="mail" />,
			},
			{
				path: "/admin/settings/user",
				element: <AdminSettingsPage section="user" />,
			},
			{
				path: "/admin/settings/network",
				element: <AdminSettingsPage section="network" />,
			},
			{
				path: "/admin/settings/runtime",
				element: <AdminSettingsPage section="runtime" />,
			},
			{
				path: "/admin/settings/storage",
				element: <AdminSettingsPage section="storage" />,
			},
			{
				path: "/admin/settings/file-processing",
				element: <AdminSettingsPage section="file_processing" />,
			},
			{
				path: "/admin/settings/file_processing",
				element: <Navigate to="/admin/settings/file-processing" replace />,
			},
			{
				path: "/admin/settings/webdav",
				element: <AdminSettingsPage section="webdav" />,
			},
			{
				path: "/admin/settings/audit",
				element: <AdminSettingsPage section="audit" />,
			},
			{
				path: "/admin/settings/general",
				element: <Navigate to="/admin/settings/site" replace />,
			},
			{
				path: "/admin/settings/operations",
				element: <Navigate to="/admin/settings/runtime" replace />,
			},
			{
				path: "/admin/settings/custom",
				element: <AdminSettingsPage section="custom" />,
			},
			{
				path: "/admin/settings/other",
				element: <AdminSettingsPage section="other" />,
			},
			{
				path: "/admin/settings/:section",
				element: <Navigate to="/admin/settings/site" replace />,
			},
			{ path: "/admin/audit", element: <AdminAuditPage /> },
			{ path: "/admin/about", element: <AdminAboutPage /> },
		],
	},
	{ path: "*", element: <Navigate to="/" replace /> },
]);
