import { Suspense } from "react";
import { Navigate, Outlet } from "react-router-dom";
import { useAuthStore } from "@/stores/authStore";
import { Loading } from "./Loading";

export function ProtectedRoute() {
	const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
	const isChecking = useAuthStore((s) => s.isChecking);
	const mustChangePassword = useAuthStore(
		(s) => s.user?.must_change_password ?? false,
	);
	if (!isAuthenticated && isChecking) return <Loading />;
	if (!isAuthenticated) return <Navigate to="/login" replace />;
	if (mustChangePassword)
		return <Navigate to="/force-password-change" replace />;
	return (
		<div
			className="animate-in fade-in duration-300"
			aria-busy={isChecking || undefined}
		>
			<Suspense fallback={<Loading />}>
				<Outlet />
			</Suspense>
		</div>
	);
}
