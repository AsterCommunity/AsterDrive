import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { AppLayout } from "@/components/layout/AppLayout";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { useWebdavAccountDialogState } from "@/components/webdav/useWebdavAccountDialogState";
import { WebdavAccountTable } from "@/components/webdav/WebdavAccountTable";
import { WebdavCopyField } from "@/components/webdav/WebdavCopyField";
import { WebdavCreateAccountDialog } from "@/components/webdav/WebdavCreateAccountDialog";
import { WebdavCredentialsDialog } from "@/components/webdav/WebdavCredentialsDialog";
import { handleApiError } from "@/hooks/useApiError";
import { useApiList } from "@/hooks/useApiList";
import { usePageTitle } from "@/hooks/usePageTitle";
import { usePendingId } from "@/hooks/usePendingId";
import { useRetainedDialogValue } from "@/hooks/useRetainedDialogValue";
import { writeTextToClipboard } from "@/lib/clipboard";
import { FOLDER_LIMIT, PAGE_SECTION_PADDING_CLASS } from "@/lib/constants";
import { absoluteAppUrl } from "@/lib/publicSiteUrl";
import { normalizeWebdavPrefix, webdavEndpointPath } from "@/lib/webdav";
import { fileService } from "@/services/fileService";
import { webdavAccountService } from "@/services/webdavAccountService";
import { useAuthStore } from "@/stores/authStore";
import type { FolderListItem } from "@/types/api";

export default function WebdavAccountsPage() {
	const { t } = useTranslation(["core", "admin", "auth", "webdav", "errors"]);
	const currentUserId = useAuthStore((state) => state.user?.id ?? null);
	usePageTitle(t("core:webdav"));
	const {
		items: accounts,
		loading,
		reload,
	} = useApiList(() => webdavAccountService.list({ limit: 200, offset: 0 }));
	const [folders, setFolders] = useState<FolderListItem[]>([]);
	const [webdavPrefix, setWebdavPrefix] = useState("/webdav");
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
			const data = await fileService.listRoot({
				file_limit: 0,
				folder_limit: FOLDER_LIMIT,
			});
			setFolders(data.folders);
		} catch (err) {
			handleApiError(err);
		}
	}, []);

	const fetchWebdavSettings = useCallback(async () => {
		try {
			const data = await webdavAccountService.settings();
			setWebdavPrefix(normalizeWebdavPrefix(data.prefix));
		} catch (err) {
			handleApiError(err);
		}
	}, []);

	useEffect(() => {
		void fetchFolders();
	}, [fetchFolders]);

	useEffect(() => {
		void fetchWebdavSettings();
	}, [fetchWebdavSettings]);

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
				toast.success(t("copied_to_clipboard"));
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
			const result = await webdavAccountService.create({
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
				await webdavAccountService.delete(id);
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
				await webdavAccountService.toggle(id);
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
		<AppLayout>
			<WebdavCreateAccountDialog
				open={dialogState.createDialogOpen}
				onOpenChange={dialogState.setCreateDialogOpen}
				createTitle={t("webdav:create_webdav_account")}
				description={t("webdav:webdav_test_hint")}
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
				loadingLabel={t("loading")}
				createLabel={t("create")}
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

			<div className="flex min-h-0 flex-1 flex-col overflow-auto">
				<div
					className={`mx-auto flex w-full max-w-7xl flex-col gap-4 py-4 md:py-6 ${PAGE_SECTION_PADDING_CLASS}`}
				>
					{/* Page Header */}
					<div className="flex items-start justify-between gap-4">
						<div>
							<h1 className="text-xl font-semibold">{t("webdav")}</h1>
							<p className="mt-1 text-sm text-muted-foreground">
								{t("webdav:webdav_page_desc")}
							</p>
						</div>
						<Button
							className="shrink-0"
							onClick={() => dialogState.setCreateDialogOpen(true)}
						>
							<Icon name="Plus" className="size-4" />
							{t("webdav:create_webdav_account")}
						</Button>
					</div>

					{/* Endpoint Info：D9 去框化，地址区块直接坐页面背景，
					    描边只留给 CopyField 输入框本身 */}
					<div className="px-1 py-2">
						<div className="flex items-center gap-2 mb-1">
							<Icon name="Globe" className="size-4 text-muted-foreground" />
							<p className="text-sm font-medium">
								{t("webdav:webdav_endpoint")}
							</p>
						</div>
						<p className="mb-3 text-xs text-muted-foreground">
							{t("webdav:webdav_use_credentials_hint")}
						</p>
						<WebdavCopyField
							value={endpointUrl}
							onCopy={() => void copyToClipboard(endpointUrl)}
							copyLabel={t("webdav:webdav_copy_endpoint")}
						/>
					</div>

					{/* Accounts Table */}
					<WebdavAccountTable
						loading={loading}
						accounts={sortedAccounts}
						currentUserId={currentUserId}
						frameless
						deletingAccountId={deletingAccountId}
						togglingAccountId={togglingAccountId}
						onDelete={(accountId) => void handleDelete(accountId)}
						onToggle={(accountId) => void handleToggle(accountId)}
						labels={{
							accessScope: t("webdav:access_scope"),
							actions: t("actions"),
							active: t("active"),
							allFiles: t("all_files"),
							cancel: t("cancel"),
							createdAt: t("created_at"),
							delete: t("delete"),
							deleting: t("admin:webdav_account_deleting"),
							disabled: t("disabled_status"),
							emptyDescription: t("webdav:no_webdav_accounts_desc"),
							emptyTitle: t("webdav:no_webdav_accounts"),
							status: t("status"),
							toggleUpdating: t("admin:webdav_account_updating"),
							username: t("core:username"),
						}}
					/>
				</div>
			</div>
		</AppLayout>
	);
}
