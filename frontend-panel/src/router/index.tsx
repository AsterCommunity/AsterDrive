import { lazy, Suspense } from "react";
import { createBrowserRouter, Navigate } from "react-router-dom";
import ErrorPage from "@/pages/ErrorPage";
import { AdminRoute } from "./AdminRoute";
import { Loading } from "./Loading";
import { LoginGuard } from "./LoginGuard";
import { PersonalWorkspaceRoute } from "./PersonalWorkspaceRoute";
import { ProtectedRoute } from "./ProtectedRoute";
import { TeamWorkspaceRoute } from "./TeamWorkspaceRoute";

const LoginPage = lazy(() => import("@/pages/LoginPage"));
const ResetPasswordPage = lazy(() => import("@/pages/ResetPasswordPage"));
const InviteRegisterPage = lazy(() => import("@/pages/InviteRegisterPage"));
const FileBrowserPage = lazy(() => import("@/pages/FileBrowserPage"));
const AdminOverviewPage = lazy(() => import("@/pages/admin/AdminOverviewPage"));
const AdminUsersPage = lazy(() => import("@/pages/admin/AdminUsersPage"));
const AdminTeamsPage = lazy(() => import("@/pages/admin/AdminTeamsPage"));
const AdminTeamDetailPage = lazy(
	() => import("@/pages/admin/AdminTeamDetailPage"),
);
const AdminPoliciesPage = lazy(() => import("@/pages/admin/AdminPoliciesPage"));
const AdminRemoteNodesPage = lazy(
	() => import("@/pages/admin/AdminRemoteNodesPage"),
);
const AdminExternalAuthPage = lazy(
	() => import("@/pages/admin/AdminExternalAuthPage"),
);
const AdminPolicyGroupsPage = lazy(
	() => import("@/pages/admin/AdminPolicyGroupsPage"),
);
const AdminTasksPage = lazy(() => import("@/pages/admin/AdminTasksPage"));
const AdminSettingsPage = lazy(() => import("@/pages/admin/AdminSettingsPage"));
const AdminSharesPage = lazy(() => import("@/pages/admin/AdminSharesPage"));
const AdminFilesPage = lazy(() => import("@/pages/admin/AdminFilesPage"));
const AdminLocksPage = lazy(() => import("@/pages/admin/AdminLocksPage"));
const AdminAboutPage = lazy(() => import("@/pages/admin/AdminAboutPage"));
const ShareViewPage = lazy(() => import("@/pages/ShareViewPage"));
const WebdavAccountsPage = lazy(() => import("@/pages/WebdavAccountsPage"));
const TrashPage = lazy(() => import("@/pages/TrashPage"));
const SettingsPage = lazy(() => import("@/pages/SettingsPage"));
const TeamManagePage = lazy(() => import("@/pages/TeamManagePage"));
const MySharesPage = lazy(() => import("@/pages/MySharesPage"));
const TasksPage = lazy(() => import("@/pages/TasksPage"));
const AdminAuditPage = lazy(() => import("@/pages/admin/AdminAuditPage"));

export const router = createBrowserRouter([
	{
		element: <LoginGuard />,
		errorElement: <ErrorPage />,
		children: [{ path: "/login", element: <LoginPage /> }],
	},
	{
		path: "/reset-password",
		errorElement: <ErrorPage />,
		element: (
			<Suspense fallback={<Loading />}>
				<ResetPasswordPage />
			</Suspense>
		),
	},
	{
		path: "/invite/:token",
		errorElement: <ErrorPage />,
		element: (
			<Suspense fallback={<Loading />}>
				<InviteRegisterPage />
			</Suspense>
		),
	},
	{
		element: <ProtectedRoute />,
		errorElement: <ErrorPage />,
		children: [
			{
				element: <PersonalWorkspaceRoute />,
				children: [
					{ path: "/", element: <FileBrowserPage /> },
					{ path: "/folder/:folderId", element: <FileBrowserPage /> },
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
		errorElement: <ErrorPage />,
		element: (
			<Suspense fallback={<Loading />}>
				<ShareViewPage />
			</Suspense>
		),
	},
	{
		element: <AdminRoute />,
		errorElement: <ErrorPage />,
		children: [
			{ path: "/admin", element: <Navigate to="/admin/overview" replace /> },
			{ path: "/admin/overview", element: <AdminOverviewPage /> },
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
