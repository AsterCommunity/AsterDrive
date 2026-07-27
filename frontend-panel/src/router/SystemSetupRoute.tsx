import { type ReactNode, Suspense, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Navigate, Outlet, useLocation } from "react-router-dom";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { useAuthStore } from "@/stores/authStore";
import { useSystemSetupStore } from "@/stores/systemSetupStore";
import { Loading } from "./Loading";

const STORAGE_SETUP_POLL_MS = 2_000;

function useSetupStateRefresh(pollWhileStorageSetup = false) {
	const isAuthenticated = useAuthStore((state) => state.isAuthenticated);
	const mustChangePassword = useAuthStore(
		(state) => state.user?.must_change_password ?? false,
	);
	const refresh = useSystemSetupStore((state) => state.refresh);
	const setupState = useSystemSetupStore((state) => state.setupState);

	useEffect(() => {
		if (!isAuthenticated || mustChangePassword) return;

		void refresh().catch(() => undefined);
	}, [isAuthenticated, mustChangePassword, refresh]);

	useEffect(() => {
		if (
			!pollWhileStorageSetup ||
			!isAuthenticated ||
			mustChangePassword ||
			setupState !== "needs_storage"
		) {
			return;
		}

		const timer = window.setInterval(() => {
			void refresh().catch(() => undefined);
		}, STORAGE_SETUP_POLL_MS);
		return () => window.clearInterval(timer);
	}, [
		isAuthenticated,
		mustChangePassword,
		pollWhileStorageSetup,
		refresh,
		setupState,
	]);

	useEffect(() => {
		if (!isAuthenticated || mustChangePassword) return;

		const refreshWhenVisible = () => {
			if (document.visibilityState === "visible") {
				void refresh().catch(() => undefined);
			}
		};
		window.addEventListener("focus", refreshWhenVisible);
		document.addEventListener("visibilitychange", refreshWhenVisible);
		return () => {
			window.removeEventListener("focus", refreshWhenVisible);
			document.removeEventListener("visibilitychange", refreshWhenVisible);
		};
	}, [isAuthenticated, mustChangePassword, refresh]);
}

function SetupStateUnavailable() {
	const { t } = useTranslation("auth");
	const refresh = useSystemSetupStore((state) => state.refresh);
	const isChecking = useSystemSetupStore((state) => state.isChecking);

	return (
		<main className="flex min-h-screen items-center justify-center bg-background px-6">
			<div className="w-full max-w-md rounded-3xl border border-border/70 bg-card p-7 shadow-xl shadow-black/5">
				<div className="flex size-11 items-center justify-center rounded-2xl bg-amber-500/10 text-amber-700 dark:text-amber-300">
					<Icon name="CircleAlert" className="size-5" />
				</div>
				<h1 className="mt-5 font-heading text-xl font-semibold">
					{t("storage_setup_state_load_failed_title")}
				</h1>
				<p className="mt-2 text-sm leading-6 text-muted-foreground">
					{t("storage_setup_state_load_failed_desc")}
				</p>
				<Button
					type="button"
					className="mt-6 w-full"
					disabled={isChecking}
					onClick={() => void refresh().catch(() => undefined)}
				>
					{isChecking ? (
						<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
					) : null}
					{t("storage_setup_retry_state")}
				</Button>
			</div>
		</main>
	);
}

function SetupStateBoundary({ children }: { children: ReactNode }) {
	const setupState = useSystemSetupStore((state) => state.setupState);
	const error = useSystemSetupStore((state) => state.error);

	if (setupState === null && error) return <SetupStateUnavailable />;
	if (setupState === null) return <Loading />;
	return children;
}

export function ReadySystemSetupRoute() {
	useSetupStateRefresh();
	const setupState = useSystemSetupStore((state) => state.setupState);
	const userRole = useAuthStore((state) => state.user?.role);
	const location = useLocation();

	return (
		<SetupStateBoundary>
			{setupState === "needs_admin" ? (
				<Navigate to="/login" replace />
			) : setupState === "needs_storage" ? (
				<Navigate
					to={
						userRole === "admin"
							? { pathname: "/setup/storage", search: location.search }
							: "/setup/pending"
					}
					replace
				/>
			) : (
				<Suspense fallback={<Loading />}>
					<Outlet />
				</Suspense>
			)}
		</SetupStateBoundary>
	);
}

export function StorageSystemSetupRoute() {
	useSetupStateRefresh(true);
	const isAuthenticated = useAuthStore((state) => state.isAuthenticated);
	const isChecking = useAuthStore((state) => state.isChecking);
	const mustChangePassword = useAuthStore(
		(state) => state.user?.must_change_password ?? false,
	);
	const userRole = useAuthStore((state) => state.user?.role);
	const setupState = useSystemSetupStore((state) => state.setupState);

	if (!isAuthenticated && isChecking) return <Loading />;
	if (!isAuthenticated) return <Navigate to="/login" replace />;
	if (mustChangePassword) {
		return <Navigate to="/force-password-change" replace />;
	}

	return (
		<SetupStateBoundary>
			{setupState === "needs_admin" ? (
				<Navigate to="/login" replace />
			) : setupState === "ready" ? (
				<Navigate to="/" replace />
			) : userRole !== "admin" ? (
				<Navigate to="/setup/pending" replace />
			) : (
				<Suspense fallback={<Loading />}>
					<Outlet />
				</Suspense>
			)}
		</SetupStateBoundary>
	);
}

export function PendingSystemSetupRoute() {
	useSetupStateRefresh(true);
	const isAuthenticated = useAuthStore((state) => state.isAuthenticated);
	const isChecking = useAuthStore((state) => state.isChecking);
	const mustChangePassword = useAuthStore(
		(state) => state.user?.must_change_password ?? false,
	);
	const userRole = useAuthStore((state) => state.user?.role);
	const setupState = useSystemSetupStore((state) => state.setupState);

	if (!isAuthenticated && isChecking) return <Loading />;
	if (!isAuthenticated) return <Navigate to="/login" replace />;
	if (mustChangePassword) {
		return <Navigate to="/force-password-change" replace />;
	}

	return (
		<SetupStateBoundary>
			{setupState === "ready" ? (
				<Navigate to="/" replace />
			) : setupState === "needs_admin" ? (
				<Navigate to="/login" replace />
			) : userRole === "admin" ? (
				<Navigate to="/setup/storage" replace />
			) : (
				<Suspense fallback={<Loading />}>
					<Outlet />
				</Suspense>
			)}
		</SetupStateBoundary>
	);
}
