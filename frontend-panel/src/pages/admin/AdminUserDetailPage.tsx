import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Navigate, useNavigate, useParams } from "react-router-dom";
import { toast } from "sonner";
import { UserDetailContent } from "@/components/admin/user-detail/UserDetailContent";
import { userDetailDraftKey } from "@/components/admin/user-detail/userDetailState";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { usePageTitle } from "@/hooks/usePageTitle";
import {
	loadAdminPolicyGroupLookup,
	readAdminPolicyGroupLookup,
} from "@/lib/adminPolicyGroupLookup";
import { adminUserService } from "@/services/adminService";
import type {
	StoragePolicyGroup,
	UpdateUserRequest,
	UserInfo,
} from "@/types/api";

export default function AdminUserDetailPage() {
	const { t } = useTranslation(["admin", "core"]);
	const navigate = useNavigate();
	const { userId } = useParams<{ userId?: string }>();
	const parsedUserId = Number(userId);
	const isValidRoute = Number.isSafeInteger(parsedUserId) && parsedUserId > 0;

	const [user, setUser] = useState<UserInfo | null>(null);
	const [userLoading, setUserLoading] = useState(true);
	const [userNotFound, setUserNotFound] = useState(false);

	const initialPolicyGroups = readAdminPolicyGroupLookup();
	const [policyGroups, setPolicyGroups] = useState<StoragePolicyGroup[]>(
		initialPolicyGroups ?? [],
	);
	const [policyGroupsLoading, setPolicyGroupsLoading] = useState(
		initialPolicyGroups == null,
	);

	usePageTitle(
		user ? `${user.username} · ${t("user_details")}` : t("user_details"),
	);

	const loadPolicyGroups = useCallback(
		async (options?: { force?: boolean }) => {
			try {
				const cachedPolicyGroups = readAdminPolicyGroupLookup();
				if (!options?.force && cachedPolicyGroups != null) {
					setPolicyGroups(cachedPolicyGroups);
					setPolicyGroupsLoading(false);
				} else {
					setPolicyGroupsLoading(true);
				}
				setPolicyGroups(await loadAdminPolicyGroupLookup(options));
			} catch (error) {
				handleApiError(error);
			} finally {
				setPolicyGroupsLoading(false);
			}
		},
		[],
	);

	useEffect(() => {
		void loadPolicyGroups();
	}, [loadPolicyGroups]);

	useEffect(() => {
		if (!isValidRoute) return;

		let cancelled = false;
		setUserLoading(true);
		adminUserService
			.get(parsedUserId)
			.then((loadedUser) => {
				if (cancelled) return;
				setUser(loadedUser);
			})
			.catch(() => {
				if (!cancelled) {
					setUser(null);
					setUserNotFound(true);
				}
			})
			.finally(() => {
				if (!cancelled) {
					setUserLoading(false);
				}
			});

		return () => {
			cancelled = true;
		};
	}, [isValidRoute, parsedUserId]);

	const backToList = useCallback(() => {
		navigate("/admin/users", { viewTransition: false });
	}, [navigate]);

	const handleUpdate = useCallback(
		async (id: number, data: UpdateUserRequest) => {
			try {
				await adminUserService.update(id, data);
				setUser(await adminUserService.get(id));
				toast.success(t("user_updated"));
			} catch (error) {
				handleApiError(error);
			}
		},
		[t],
	);

	if (!isValidRoute) {
		return <Navigate to="/admin/users" replace />;
	}

	if (userNotFound) {
		return (
			<AdminLayout>
				<AdminPageShell>
					<div className="flex flex-col items-center gap-4 py-16 text-center">
						<p className="text-sm text-muted-foreground">
							{t("user_not_found")}
						</p>
						<Button variant="outline" size="sm" onClick={backToList}>
							<Icon name="ArrowLeft" className="mr-1 size-4" />
							{t("user_back_to_list")}
						</Button>
					</div>
				</AdminPageShell>
			</AdminLayout>
		);
	}

	return (
		<AdminLayout>
			<AdminPageShell className="overflow-hidden px-0 md:px-0">
				{userLoading || user == null ? (
					<div className="flex min-h-0 flex-1 items-center justify-center">
						<Icon
							name="Spinner"
							className="size-6 animate-spin text-muted-foreground"
						/>
					</div>
				) : (
					<UserDetailContent
						key={userDetailDraftKey(user)}
						onPageBack={backToList}
						onRefreshPolicyGroups={() => loadPolicyGroups({ force: true })}
						onUpdate={handleUpdate}
						policyGroups={policyGroups}
						policyGroupsLoading={policyGroupsLoading}
						user={user}
					/>
				)}
			</AdminPageShell>
		</AdminLayout>
	);
}
