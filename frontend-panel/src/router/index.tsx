import { type ComponentType, lazy, Suspense } from "react";
import { createBrowserRouter, Navigate } from "react-router-dom";
import { ensureI18nNamespaces, type LocaleNamespace } from "@/i18n";
import { AdminRoute } from "./AdminRoute";
import { Loading } from "./Loading";
import { LoginGuard } from "./LoginGuard";
import { PersonalWorkspaceRoute } from "./PersonalWorkspaceRoute";
import { ProtectedRoute } from "./ProtectedRoute";
import { TeamWorkspaceRoute } from "./TeamWorkspaceRoute";

function lazyPage<TProps extends object>(
	load: () => Promise<{ default: ComponentType<TProps> }>,
	namespaces: readonly LocaleNamespace[] = [],
) {
	return lazy<ComponentType<TProps>>(async () => {
		const [module] = await Promise.all([
			load(),
			ensureI18nNamespaces(namespaces),
		]);
		return module;
	});
}

const LoginPage = lazyPage(() => import("@/pages/LoginPage"));
const ForcePasswordChangePage = lazyPage(
	() => import("@/pages/ForcePasswordChangePage"),
	["auth", "settings"],
);
const ResetPasswordPage = lazyPage(
	() => import("@/pages/ResetPasswordPage"),
	["auth"],
);
const InviteRegisterPage = lazyPage(
	() => import("@/pages/InviteRegisterPage"),
	["auth"],
);
const FileBrowserPage = lazyPage(
	() => import("@/pages/FileBrowserPage"),
	["files"],
);
const CategoryBrowserPage = lazyPage(
	() => import("@/pages/CategoryBrowserPage"),
	["files"],
);
const SearchBrowserPage = lazyPage(
	() => import("@/pages/SearchBrowserPage"),
	["files", "search"],
);
const AdminOverviewPage = lazyPage(
	() => import("@/pages/admin/AdminOverviewPage"),
	["admin"],
);
const AdminUsersPage = lazyPage(
	() => import("@/pages/admin/AdminUsersPage"),
	["admin"],
);
const AdminUserInvitationsPage = lazyPage(
	() => import("@/pages/admin/AdminUserInvitationsPage"),
	["admin"],
);
const AdminTeamsPage = lazyPage(
	() => import("@/pages/admin/AdminTeamsPage"),
	["admin"],
);
const AdminTeamDetailPage = lazyPage(
	() => import("@/pages/admin/AdminTeamDetailPage"),
	["admin", "settings"],
);
const AdminPoliciesPage = lazyPage(
	() => import("@/pages/admin/AdminPoliciesPage"),
	["admin"],
);
const AdminRemoteNodesPage = lazyPage(
	() => import("@/pages/admin/AdminRemoteNodesPage"),
	["admin"],
);
const AdminExternalAuthPage = lazyPage(
	() => import("@/pages/admin/AdminExternalAuthPage"),
	["admin"],
);
const AdminPolicyGroupsPage = lazyPage(
	() => import("@/pages/admin/AdminPolicyGroupsPage"),
	["admin"],
);
const AdminTasksPage = lazyPage(
	() => import("@/pages/admin/AdminTasksPage"),
	["admin", "tasks"],
);
const AdminSettingsPage = lazyPage(
	() => import("@/pages/admin/AdminSettingsPage"),
	["admin"],
);
const AdminSharesPage = lazyPage(
	() => import("@/pages/admin/AdminSharesPage"),
	["admin"],
);
const AdminFilesPage = lazyPage(
	() => import("@/pages/admin/AdminFilesPage"),
	["admin"],
);
const AdminLocksPage = lazyPage(
	() => import("@/pages/admin/AdminLocksPage"),
	["admin"],
);
const AdminAboutPage = lazyPage(
	() => import("@/pages/admin/AdminAboutPage"),
	["admin"],
);
const ShareViewPage = lazyPage(
	() => import("@/pages/ShareViewPage"),
	["share", "files"],
);
const WebdavAccountsPage = lazyPage(
	() => import("@/pages/WebdavAccountsPage"),
	["webdav"],
);
const TrashPage = lazyPage(() => import("@/pages/TrashPage"), ["files"]);
const SettingsPage = lazyPage(
	() => import("@/pages/SettingsPage"),
	["settings"],
);
const TeamManagePage = lazyPage(
	() => import("@/pages/TeamManagePage"),
	["settings"],
);
const MySharesPage = lazyPage(
	() => import("@/pages/MySharesPage"),
	["share", "files"],
);
const TasksPage = lazyPage(() => import("@/pages/TasksPage"), ["tasks"]);
const AdminAuditPage = lazyPage(
	() => import("@/pages/admin/AdminAuditPage"),
	["admin"],
);
const ErrorPage = lazyPage(() => import("@/pages/ErrorPage"));

const errorElement = (
	<Suspense fallback={<Loading />}>
		<ErrorPage />
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
		element: (
			<Suspense fallback={<Loading />}>
				<ShareViewPage />
			</Suspense>
		),
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
