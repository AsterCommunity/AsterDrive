import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { useWebdavAccountDialogState } from "@/components/webdav/useWebdavAccountDialogState";
import { WebdavAccountTable } from "@/components/webdav/WebdavAccountTable";
import { WebdavCopyField } from "@/components/webdav/WebdavCopyField";
import { WebdavCreateAccountDialog } from "@/components/webdav/WebdavCreateAccountDialog";
import { WebdavCredentialsDialog } from "@/components/webdav/WebdavCredentialsDialog";
import { handleApiError } from "@/hooks/useApiError";
import { useApiList } from "@/hooks/useApiList";
import { usePendingId } from "@/hooks/usePendingId";
import { useRetainedDialogValue } from "@/hooks/useRetainedDialogValue";
import { writeTextToClipboard } from "@/lib/clipboard";
import { FOLDER_LIMIT } from "@/lib/constants";
import { absoluteAppUrl } from "@/lib/publicSiteUrl";
import { webdavEndpointPath } from "@/lib/webdav";
import { createFileService } from "@/services/fileService";
import { webdavAccountService } from "@/services/webdavAccountService";
import type { FolderListItem } from "@/types/api";

interface TeamManageWebdavSectionProps {
	canManageTeam: boolean;
	currentUserId: number | null;
	teamId: number;
	webdavPrefix: string;
}

export function TeamManageWebdavSection({
	canManageTeam,
	currentUserId,
	teamId,
	webdavPrefix,
}: TeamManageWebdavSectionProps) {
	const { t } = useTranslation([
		"core",
		"admin",
		"settings",
		"webdav",
		"errors",
	]);
	const teamFileService = useMemo(
		() => createFileService({ kind: "team", teamId }),
		[teamId],
	);
	const {
		items: accounts,
		loading,
		reload,
	} = useApiList(
		() => webdavAccountService.listForTeam(teamId, { limit: 200, offset: 0 }),
		[teamId],
	);
	const [folders, setFolders] = useState<FolderListItem[]>([]);
	const dialogState = useWebdavAccountDialogState();
	const {
		pendingId: deletingAccountId,
		runWithPending: runWithDeletingAccount,
	} = usePendingId<number>();
	const {
		pendingId: togglingAccountId,
		runWithPending: runWithTogglingAccount,
	} = usePendingId<number>();
	const {
		retainedValue: recentCredentials,
		handleOpenChangeComplete: handleCredentialsDialogOpenChangeComplete,
	} = useRetainedDialogValue(
		dialogState.showPassword,
		dialogState.credentialsDialogOpen,
	);

	const fetchFolders = useCallback(async () => {
		try {
			const data = await teamFileService.listRoot({
				file_limit: 0,
				folder_limit: FOLDER_LIMIT,
			});
			setFolders(data.folders);
		} catch (err) {
			handleApiError(err);
		}
	}, [teamFileService]);

	useEffect(() => {
		void fetchFolders();
	}, [fetchFolders]);

	const endpointUrl = absoluteAppUrl(webdavEndpointPath(webdavPrefix));
	const rootFolderOptions = [
		{
			label: t("webdav:all_files_full_access"),
			value: "__all__",
		},
		...folders.map((folder) => ({
			label: `/${folder.name}`,
			value: String(folder.id),
		})),
	];
	const sortedAccounts = useMemo(
		() =>
			accounts.toSorted(
				(a, b) =>
					new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
			),
		[accounts],
	);

	const copyToClipboard = useCallback(
		async (value: string) => {
			try {
				await writeTextToClipboard(value);
				toast.success(t("core:copied_to_clipboard"));
			} catch {
				toast.error(t("errors:unexpected_error"));
			}
		},
		[t],
	);

	const handleCreate = async () => {
		if (!dialogState.newUsername.trim()) {
			toast.error(t("webdav:username_required"));
			return;
		}

		dialogState.setCreating(true);
		try {
			const result = await webdavAccountService.createForTeam(teamId, {
				username: dialogState.newUsername.trim(),
				password: dialogState.newPassword.trim() || undefined,
				root_folder_id: dialogState.selectedFolderId ?? null,
			});
			dialogState.showCreatedCredentials({
				username: result.username,
				password: result.password,
			});
			toast.success(t("admin:webdav_account_created"));
			void reload();
		} catch (err) {
			handleApiError(err);
		} finally {
			dialogState.setCreating(false);
		}
	};

	const handleDelete = async (id: number) => {
		await runWithDeletingAccount(id, async () => {
			try {
				await webdavAccountService.deleteForTeam(teamId, id);
				toast.success(t("admin:webdav_account_deleted"));
				void reload();
			} catch (err) {
				handleApiError(err);
			}
		});
	};

	const handleToggle = async (id: number) => {
		await runWithTogglingAccount(id, async () => {
			try {
				await webdavAccountService.toggleForTeam(teamId, id);
				void reload();
			} catch (err) {
				handleApiError(err);
			}
		});
	};

	const handleTest = async () => {
		if (!recentCredentials) return;
		dialogState.setTesting(true);
		dialogState.setTestResult(null);
		try {
			await webdavAccountService.test({
				username: recentCredentials.username,
				password: recentCredentials.password,
			});
			dialogState.setTestResult(true);
			toast.success(t("admin:connection_success"));
		} catch {
			dialogState.setTestResult(false);
			toast.error(t("admin:connection_test_failed"));
		} finally {
			dialogState.setTesting(false);
		}
	};

	return (
		<section>
			<div className="mb-5 flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
				<div>
					<h4 className="text-base font-semibold text-foreground">
						{t("settings:settings_team_webdav_title")}
					</h4>
					<p className="mt-1 text-sm text-muted-foreground">
						{canManageTeam
							? t("settings:settings_team_webdav_desc_manager")
							: t("settings:settings_team_webdav_desc_member")}
					</p>
				</div>
				<Button
					type="button"
					onClick={() => dialogState.setCreateDialogOpen(true)}
				>
					<Icon name="Plus" className="size-4" />
					{t("webdav:create_webdav_account")}
				</Button>
			</div>

			<div className="mb-4 rounded-xl bg-muted/30 p-4">
				<div className="mb-1 flex items-center gap-2">
					<Icon name="Globe" className="size-4 text-muted-foreground" />
					<p className="text-sm font-medium">{t("webdav:webdav_endpoint")}</p>
				</div>
				<p className="mb-3 text-xs text-muted-foreground">
					{t("settings:settings_team_webdav_endpoint_hint")}
				</p>
				<WebdavCopyField
					value={endpointUrl}
					onCopy={() => void copyToClipboard(endpointUrl)}
					copyLabel={t("webdav:webdav_copy_endpoint")}
				/>
			</div>

			<WebdavAccountTable
				frameless
				loading={loading}
				accounts={sortedAccounts}
				canManageTeam={canManageTeam}
				currentUserId={currentUserId}
				deletingAccountId={deletingAccountId}
				togglingAccountId={togglingAccountId}
				onDelete={(accountId) => void handleDelete(accountId)}
				onToggle={(accountId) => void handleToggle(accountId)}
				labels={{
					accessScope: t("webdav:access_scope"),
					actions: t("core:actions"),
					active: t("core:active"),
					allFiles: t("core:all_files"),
					cancel: t("core:cancel"),
					createdAt: t("core:created_at"),
					delete: t("core:delete"),
					deleting: t("admin:webdav_account_deleting"),
					disabled: t("core:disabled_status"),
					emptyDescription: t("settings:settings_team_webdav_empty_desc"),
					emptyTitle: t("webdav:no_webdav_accounts"),
					owner: t("settings:settings_team_webdav_owner"),
					status: t("core:status"),
					toggleUpdating: t("admin:webdav_account_updating"),
					username: t("core:username"),
				}}
			/>

			<WebdavCreateAccountDialog
				open={dialogState.createDialogOpen}
				onOpenChange={dialogState.setCreateDialogOpen}
				createTitle={t("webdav:create_webdav_account")}
				description={t("settings:settings_team_webdav_create_desc")}
				usernameLabel={t("core:username")}
				usernamePlaceholder={t("webdav:webdav_username_placeholder")}
				passwordLabel={t("core:password")}
				autoGenerateLabel={t("webdav:auto_generate_password")}
				rootFolderLabel={t("webdav:access_scope")}
				rootFolderOptions={rootFolderOptions}
				rootFolderId={dialogState.selectedFolderId}
				noFoldersLabel={t("webdav:webdav_no_root_folders")}
				newUsername={dialogState.newUsername}
				newPassword={dialogState.newPassword}
				creating={dialogState.creating}
				loadingLabel={t("core:loading")}
				createLabel={t("core:create")}
				onUsernameChange={dialogState.setNewUsername}
				onPasswordChange={dialogState.setNewPassword}
				onRootFolderChange={dialogState.setSelectedFolderId}
				onCreate={() => void handleCreate()}
			/>

			<WebdavCredentialsDialog
				open={dialogState.credentialsDialogOpen}
				credentials={recentCredentials}
				onOpenChange={(open) => {
					dialogState.setCredentialsDialogOpen(open);
					if (!open) {
						dialogState.clearCredentials();
					}
				}}
				onOpenChangeComplete={(open) => {
					handleCredentialsDialogOpenChangeComplete(open);
					if (!open) {
						dialogState.setTestResult(null);
					}
				}}
				onCopy={(value) => void copyToClipboard(value)}
				onTest={() => void handleTest()}
				title={t("webdav:webdav_recent_credentials")}
				description={t("webdav:webdav_recent_credentials_desc")}
				usernameLabel={t("core:username")}
				passwordLabel={t("core:password")}
				testResult={dialogState.testResult}
				testing={dialogState.testing}
				connectionSuccessLabel={t("admin:connection_success")}
				connectionFailedLabel={t("admin:connection_test_failed")}
				testConnectionLabel={t("admin:test_connection")}
			/>
		</section>
	);
}
