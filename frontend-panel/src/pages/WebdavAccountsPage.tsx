import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { AdminTableList } from "@/components/common/AdminTableList";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { AppLayout } from "@/components/layout/AppLayout";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import {
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from "@/components/ui/table";
import { handleApiError } from "@/hooks/useApiError";
import { useApiList } from "@/hooks/useApiList";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { usePageTitle } from "@/hooks/usePageTitle";
import { useRetainedDialogValue } from "@/hooks/useRetainedDialogValue";
import { writeTextToClipboard } from "@/lib/clipboard";
import { FOLDER_LIMIT, PAGE_SECTION_PADDING_CLASS } from "@/lib/constants";
import { formatDateShort } from "@/lib/format";
import { absoluteAppUrl } from "@/lib/publicSiteUrl";
import { fileService } from "@/services/fileService";
import { webdavAccountService } from "@/services/webdavAccountService";
import type { FolderListItem } from "@/types/api";

function CopyField({
	value,
	onCopy,
	copyLabel,
}: {
	value: string;
	onCopy: () => void;
	copyLabel?: string;
}) {
	return (
		<div className="flex flex-col gap-2 sm:flex-row">
			<Input readOnly value={value} className="font-mono" />
			<Button
				variant="outline"
				size={copyLabel ? "default" : "icon-sm"}
				className="sm:shrink-0"
				onClick={onCopy}
			>
				<Icon name="Copy" className="h-3.5 w-3.5" />
				{copyLabel ? copyLabel : null}
			</Button>
		</div>
	);
}

function normalizeWebdavPrefix(prefix: string): string {
	const trimmed = prefix.trim();
	if (!trimmed) return "/webdav";
	const withLeadingSlash = trimmed.startsWith("/") ? trimmed : `/${trimmed}`;
	if (withLeadingSlash === "/") return "/";
	return withLeadingSlash.endsWith("/")
		? withLeadingSlash.slice(0, -1)
		: withLeadingSlash;
}

export default function WebdavAccountsPage() {
	const { t } = useTranslation(["core", "admin", "auth", "webdav", "errors"]);
	usePageTitle(t("core:webdav"));
	const {
		items: accounts,
		loading,
		reload,
	} = useApiList(() => webdavAccountService.list({ limit: 200, offset: 0 }));
	const [creating, setCreating] = useState(false);
	const [newUsername, setNewUsername] = useState("");
	const [newPassword, setNewPassword] = useState("");
	const [selectedFolderId, setSelectedFolderId] = useState<number | undefined>(
		undefined,
	);
	const [folders, setFolders] = useState<FolderListItem[]>([]);
	const [showPassword, setShowPassword] = useState<{
		username: string;
		password: string;
	} | null>(null);
	const [testing, setTesting] = useState(false);
	const [testResult, setTestResult] = useState<boolean | null>(null);
	const [createDialogOpen, setCreateDialogOpen] = useState(false);
	const [credentialsDialogOpen, setCredentialsDialogOpen] = useState(false);
	const [webdavPrefix, setWebdavPrefix] = useState("/webdav");
	const {
		retainedValue: recentCredentials,
		handleOpenChangeComplete: handleCredentialsDialogOpenChangeComplete,
	} = useRetainedDialogValue(showPassword, credentialsDialogOpen);

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

	const endpointPath = webdavPrefix === "/" ? "/" : `${webdavPrefix}/`;
	const endpointUrl = absoluteAppUrl(endpointPath);
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
			[...accounts].sort(
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
		if (!newUsername.trim()) {
			toast.error(t("webdav:username_required"));
			return;
		}

		setCreating(true);
		try {
			const result = await webdavAccountService.create(
				newUsername.trim(),
				newPassword.trim() || undefined,
				selectedFolderId,
			);
			setShowPassword({
				username: result.username,
				password: result.password,
			});
			setTestResult(null);
			setNewUsername("");
			setNewPassword("");
			setSelectedFolderId(undefined);
			setCreateDialogOpen(false);
			setCredentialsDialogOpen(true);
			toast.success(t("admin:webdav_account_created"));
			void reload();
		} catch (err) {
			handleApiError(err);
		} finally {
			setCreating(false);
		}
	};

	const handleDelete = async (id: number) => {
		try {
			await webdavAccountService.delete(id);
			toast.success(t("admin:webdav_account_deleted"));
			void reload();
		} catch (err) {
			handleApiError(err);
		}
	};

	const { requestConfirm, dialogProps } = useConfirmDialog(handleDelete);

	const handleToggle = async (id: number) => {
		try {
			await webdavAccountService.toggle(id);
			void reload();
		} catch (err) {
			handleApiError(err);
		}
	};

	const handleTest = async () => {
		if (!recentCredentials) return;
		setTesting(true);
		setTestResult(null);
		try {
			await webdavAccountService.test(
				recentCredentials.username,
				recentCredentials.password,
			);
			setTestResult(true);
			toast.success(t("admin:connection_success"));
		} catch {
			setTestResult(false);
			toast.error(t("admin:connection_test_failed"));
		} finally {
			setTesting(false);
		}
	};

	return (
		<AppLayout>
			{/* Create Account Dialog */}
			<Dialog open={createDialogOpen} onOpenChange={setCreateDialogOpen}>
				<DialogContent className="max-w-md">
					<DialogHeader>
						<DialogTitle>{t("webdav:create_webdav_account")}</DialogTitle>
						<DialogDescription>
							{t("webdav:webdav_test_hint")}
						</DialogDescription>
					</DialogHeader>
					<div className="space-y-4 py-2">
						{/* username field */}
						<div className="space-y-1.5">
							<Label htmlFor="username">{t("core:username")}</Label>
							<Input
								id="username"
								value={newUsername}
								onChange={(e) => setNewUsername(e.target.value)}
								placeholder={t("webdav:webdav_username_placeholder")}
							/>
						</div>
						{/* password field */}
						<div className="space-y-1.5">
							<Label htmlFor="password">{t("core:password")}</Label>
							<Input
								id="password"
								type="password"
								value={newPassword}
								onChange={(e) => setNewPassword(e.target.value)}
								placeholder={t("webdav:auto_generate_password")}
							/>
							<p className="text-xs text-muted-foreground">
								{t("webdav:auto_generate_password")}
							</p>
						</div>
						{/* access scope select */}
						<div className="space-y-1.5">
							<Label htmlFor="rootFolder">{t("webdav:access_scope")}</Label>
							<Select
								items={rootFolderOptions}
								value={
									selectedFolderId != null
										? String(selectedFolderId)
										: "__all__"
								}
								onValueChange={(value) =>
									setSelectedFolderId(
										value === "__all__" ? undefined : Number(value),
									)
								}
							>
								<SelectTrigger id="rootFolder">
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									{rootFolderOptions.map((option) => (
										<SelectItem key={option.value} value={option.value}>
											{option.label}
										</SelectItem>
									))}
								</SelectContent>
							</Select>
							{folders.length === 0 && (
								<p className="text-xs text-muted-foreground">
									{t("webdav:webdav_no_root_folders")}
								</p>
							)}
						</div>
					</div>
					<DialogFooter>
						<Button
							onClick={() => void handleCreate()}
							disabled={creating || !newUsername.trim()}
						>
							<Icon name="Plus" className="h-4 w-4" />
							{creating ? t("loading") : t("create")}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>

			{/* Credentials Dialog - shows after successful creation */}
			{recentCredentials && (
				<Dialog
					open={credentialsDialogOpen}
					onOpenChange={(open) => {
						setCredentialsDialogOpen(open);
						if (!open) {
							setShowPassword(null);
						}
					}}
					onOpenChangeComplete={(open) => {
						handleCredentialsDialogOpenChangeComplete(open);
						if (!open) {
							setTestResult(null);
						}
					}}
				>
					<DialogContent className="max-w-md">
						<DialogHeader>
							<DialogTitle>{t("webdav:webdav_recent_credentials")}</DialogTitle>
							<DialogDescription>
								{t("webdav:webdav_recent_credentials_desc")}
							</DialogDescription>
						</DialogHeader>
						<div className="space-y-4 py-2">
							<div className="space-y-1.5">
								<Label>{t("core:username")}</Label>
								<CopyField
									value={recentCredentials.username}
									onCopy={() =>
										void copyToClipboard(recentCredentials.username)
									}
								/>
							</div>
							<div className="space-y-1.5">
								<Label>{t("core:password")}</Label>
								<CopyField
									value={recentCredentials.password}
									onCopy={() =>
										void copyToClipboard(recentCredentials.password)
									}
								/>
							</div>
							{testResult !== null && (
								<Badge
									variant={testResult ? "secondary" : "destructive"}
									className={
										testResult
											? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
											: undefined
									}
								>
									{testResult
										? t("admin:connection_success")
										: t("admin:connection_test_failed")}
								</Badge>
							)}
						</div>
						<DialogFooter>
							<Button
								variant="outline"
								onClick={() => void handleTest()}
								disabled={testing}
							>
								{testing ? (
									<Icon name="Spinner" className="h-4 w-4 animate-spin" />
								) : (
									<Icon name="WifiHigh" className="h-4 w-4" />
								)}
								{t("admin:test_connection")}
							</Button>
						</DialogFooter>
					</DialogContent>
				</Dialog>
			)}

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
							onClick={() => setCreateDialogOpen(true)}
						>
							<Icon name="Plus" className="h-4 w-4" />
							{t("webdav:create_webdav_account")}
						</Button>
					</div>

					{/* Endpoint Info Card */}
					<div className="rounded-xl border bg-muted/20 p-4">
						<div className="flex items-center gap-2 mb-1">
							<Icon name="Globe" className="h-4 w-4 text-muted-foreground" />
							<p className="text-sm font-medium">
								{t("webdav:webdav_endpoint")}
							</p>
						</div>
						<p className="mb-3 text-xs text-muted-foreground">
							{t("webdav:webdav_use_credentials_hint")}
						</p>
						<CopyField
							value={endpointUrl}
							onCopy={() => void copyToClipboard(endpointUrl)}
							copyLabel={t("webdav:webdav_copy_endpoint")}
						/>
					</div>

					{/* Accounts Table */}
					<AdminTableList
						loading={loading}
						items={sortedAccounts}
						columns={5}
						rows={5}
						emptyIcon={<Icon name="Globe" className="h-10 w-10" />}
						emptyTitle={t("webdav:no_webdav_accounts")}
						emptyDescription={t("webdav:no_webdav_accounts_desc")}
						headerRow={
							<TableHeader>
								<TableRow>
									<TableHead>{t("core:username")}</TableHead>
									<TableHead>{t("webdav:access_scope")}</TableHead>
									<TableHead>{t("status")}</TableHead>
									<TableHead>{t("created_at")}</TableHead>
									<TableHead className="w-[96px] text-right">
										{t("actions")}
									</TableHead>
								</TableRow>
							</TableHeader>
						}
						renderRow={(account) => (
							<TableRow key={account.id}>
								<TableCell>
									<div className="min-w-[140px]">
										<span className="truncate font-mono text-sm font-medium text-foreground">
											{account.username}
										</span>
									</div>
								</TableCell>
								<TableCell>
									<div className="flex min-w-[180px] items-center gap-2 text-sm text-foreground">
										<Icon
											name={account.root_folder_path ? "FolderOpen" : "Globe"}
											className="h-3.5 w-3.5 shrink-0 text-muted-foreground"
										/>
										<span className="truncate">
											{account.root_folder_path ?? t("all_files")}
										</span>
									</div>
								</TableCell>
								<TableCell>
									<Badge
										variant={account.is_active ? "secondary" : "outline"}
										className={
											account.is_active
												? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
												: undefined
										}
									>
										{account.is_active ? t("active") : t("disabled_status")}
									</Badge>
								</TableCell>
								<TableCell className="text-sm text-muted-foreground">
									{formatDateShort(account.created_at)}
								</TableCell>
								<TableCell>
									<div className="flex justify-end gap-2">
										<Button
											variant="outline"
											size="icon-sm"
											onClick={() => void handleToggle(account.id)}
											title={
												account.is_active ? t("disabled_status") : t("active")
											}
										>
											<Icon name="Power" className="h-3.5 w-3.5" />
										</Button>
										<Button
											variant="destructive"
											size="icon-sm"
											onClick={() => requestConfirm(account.id)}
											title={t("delete")}
										>
											<Icon name="Trash" className="h-3.5 w-3.5" />
										</Button>
									</div>
								</TableCell>
							</TableRow>
						)}
					/>
				</div>
			</div>

			<ConfirmDialog
				{...dialogProps}
				title={t("are_you_sure")}
				description={t("cannot_undo")}
				confirmLabel={t("delete")}
				variant="destructive"
			/>
		</AppLayout>
	);
}
