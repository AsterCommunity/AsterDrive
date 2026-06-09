import { Suspense } from "react";
import { Navigate, Outlet } from "react-router-dom";
import { AdminSiteUrlMismatchPrompt } from "@/components/layout/AdminSiteUrlMismatchPrompt";
import { useAuthStore } from "@/stores/authStore";
import { Loading } from "./Loading";

export function AdminRoute() {
	const user = useAuthStore((s) => s.user);
	const isChecking = useAuthStore((s) => s.isChecking);
	const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
	if (!isAuthenticated && isChecking) return <Loading />;
	if (!isAuthenticated) return <Navigate to="/login" replace />;
	if (!user && isChecking) return <Loading />;
	if (user?.must_change_password) {
		return <Navigate to="/force-password-change" replace />;
	}
	if (user?.role !== "admin") return <Navigate to="/" replace />;
	return (
		<div aria-busy={isChecking || undefined}>
			<Suspense fallback={<Loading />}>
				<AdminSiteUrlMismatchPrompt />
				<Outlet />
			</Suspense>
		</div>
	);
}
