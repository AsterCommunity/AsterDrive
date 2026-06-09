import { Suspense } from "react";
import { Navigate, Outlet } from "react-router-dom";
import { useAuthStore } from "@/stores/authStore";
import { Loading } from "./Loading";

export function LoginGuard() {
	const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
	const isChecking = useAuthStore((s) => s.isChecking);
	const mustChangePassword = useAuthStore(
		(s) => s.user?.must_change_password ?? false,
	);
	if (isAuthenticated && mustChangePassword) {
		return <Navigate to="/force-password-change" replace />;
	}
	if (isAuthenticated) return <Navigate to="/" replace />;
	if (isChecking) return <Loading />;
	return (
		<Suspense fallback={<Loading />}>
			<Outlet />
		</Suspense>
	);
}
