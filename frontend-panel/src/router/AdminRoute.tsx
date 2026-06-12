import { lazy, Suspense, useEffect, useState } from "react";
import { Navigate, Outlet } from "react-router-dom";
import { ensureI18nNamespaces } from "@/i18n";
import { logger } from "@/lib/logger";
import { useAuthStore } from "@/stores/authStore";
import { Loading } from "./Loading";

const AdminSiteUrlMismatchPrompt = lazy(() =>
	import("@/components/layout/AdminSiteUrlMismatchPrompt").then((module) => ({
		default: module.AdminSiteUrlMismatchPrompt,
	})),
);

export function AdminRoute() {
	const user = useAuthStore((s) => s.user);
	const isChecking = useAuthStore((s) => s.isChecking);
	const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
	const [adminLocaleReady, setAdminLocaleReady] = useState(false);

	useEffect(() => {
		if (!isAuthenticated || user?.role !== "admin") {
			setAdminLocaleReady(false);
			return;
		}

		let cancelled = false;
		void (async () => {
			try {
				await ensureI18nNamespaces(["admin", "core"]);
			} catch (error) {
				if (!cancelled) {
					logger.warn("failed to load admin locale namespaces", error);
				}
			}
			if (!cancelled) {
				setAdminLocaleReady(true);
			}
		})();

		return () => {
			cancelled = true;
		};
	}, [isAuthenticated, user?.role]);

	if (!isAuthenticated && isChecking) return <Loading />;
	if (!isAuthenticated) return <Navigate to="/login" replace />;
	if (!user && isChecking) return <Loading />;
	if (user?.must_change_password) {
		return <Navigate to="/force-password-change" replace />;
	}
	if (user?.role !== "admin") return <Navigate to="/" replace />;
	if (!adminLocaleReady) return <Loading />;
	return (
		<div aria-busy={isChecking || undefined}>
			<Suspense fallback={<Loading />}>
				<AdminSiteUrlMismatchPrompt />
				<Outlet />
			</Suspense>
		</div>
	);
}
