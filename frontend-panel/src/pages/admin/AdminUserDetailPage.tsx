import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Navigate, useNavigate, useParams } from "react-router-dom";
import { toast } from "sonner";
import { UserDetailEditor } from "@/components/admin/UserDetailEditor";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { usePageTitle } from "@/hooks/usePageTitle";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { getUserDisplayName } from "@/lib/user";
import { adminUserService } from "@/services/adminService";
import type { UpdateUserRequest, UserInfo } from "@/types/api";

export default function AdminUserDetailPage() {
	const { t } = useTranslation("admin");
	const navigate = useNavigate();
	const { userId } = useParams<{ userId?: string }>();
	const parsedUserId = Number(userId);
	const isValidRoute = Number.isSafeInteger(parsedUserId) && parsedUserId > 0;
	const [user, setUser] = useState<UserInfo | null>(null);
	const [loading, setLoading] = useState(isValidRoute);
	const [notFound, setNotFound] = useState(false);

	usePageTitle(user ? getUserDisplayName(user) : t("user_details"));

	useEffect(() => {
		if (!isValidRoute) return;

		let cancelled = false;
		setLoading(true);
		setNotFound(false);
		adminUserService
			.get(parsedUserId)
			.then((loadedUser) => {
				if (!cancelled) setUser(loadedUser);
			})
			.catch(() => {
				if (!cancelled) setNotFound(true);
			})
			.finally(() => {
				if (!cancelled) setLoading(false);
			});

		return () => {
			cancelled = true;
		};
	}, [isValidRoute, parsedUserId]);

	const backToList = () => {
		navigate("/admin/users", { viewTransition: false });
	};

	const updateUser = async (id: number, data: UpdateUserRequest) => {
		const updatedUser = await adminUserService.update(id, data);
		setUser(updatedUser);
		toast.success(t("user_updated"));
		return updatedUser;
	};

	if (!isValidRoute) {
		return <Navigate to="/admin/users" replace />;
	}

	if (notFound) {
		return (
			<AdminLayout>
				<AdminPageShell>
					<div className="flex flex-col items-center gap-4 py-16 text-center">
						<p className="text-sm text-muted-foreground">
							{t("user_not_found")}
						</p>
						<Button
							variant="outline"
							size="sm"
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							onClick={backToList}
						>
							<Icon name="ArrowLeft" className="mr-1 size-4" />
							{t("back_to_users")}
						</Button>
					</div>
				</AdminPageShell>
			</AdminLayout>
		);
	}

	return (
		<AdminLayout>
			<AdminPageShell>
				{loading || !user ? (
					<div className="flex items-center justify-center gap-2 py-16 text-sm text-muted-foreground">
						<Icon name="Spinner" className="size-4 animate-spin" />
						{t("core:loading")}
					</div>
				) : (
					<UserDetailEditor
						onBack={backToList}
						onUpdate={updateUser}
						user={user}
					/>
				)}
			</AdminPageShell>
		</AdminLayout>
	);
}
