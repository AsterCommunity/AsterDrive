import { useReducer } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
	FixedDialogFooter,
	ManagerDialogScrollableList,
	ManagerDialogShell,
} from "@/components/common/ManagerDialogShell";
import {
	computeShareExpiry,
	normalizeMaxDownloads,
} from "@/components/files/shareDialogShared";
import {
	initialShareDialogState,
	shareDialogReducer,
} from "@/components/files/shareDialogState";
import { Button } from "@/components/ui/button";
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
import { handleApiError } from "@/hooks/useApiError";
import { usePendingAction } from "@/hooks/usePendingAction";
import { writeTextToClipboard } from "@/lib/clipboard";
import { fileService } from "@/services/fileService";
import { shareService } from "@/services/shareService";

type ShareLinkMode = "page" | "direct";

interface ShareDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onOpenChangeComplete?: (open: boolean) => void;
	onShareCreated?: () => void | Promise<void>;
	fileId?: number;
	folderId?: number;
	name: string;
	initialMode?: ShareLinkMode;
}

export function ShareDialog({
	open,
	onOpenChange,
	onOpenChangeComplete,
	onShareCreated,
	fileId,
	folderId,
	name,
	initialMode,
}: ShareDialogProps) {
	const { t } = useTranslation(["core", "share", "errors"]);
	const directEligible = fileId != null;
	const mode: ShareLinkMode =
		directEligible && initialMode === "direct" ? "direct" : "page";
	const [state, dispatch] = useReducer(
		shareDialogReducer,
		initialShareDialogState,
	);
	const { pending: creating, runWithPending } = usePendingAction();
	const { copiedLink, createdLinks, expiry, loading, maxDownloads, password } =
		state;
	const expiryOptions = [
		{ label: t("share:share_expiry_never"), value: "never" },
		{ label: t("share:share_expiry_1h"), value: "1h" },
		{ label: t("share:share_expiry_1d"), value: "1d" },
		{ label: t("share:share_expiry_7d"), value: "7d" },
		{ label: t("share:share_expiry_30d"), value: "30d" },
	] satisfies ReadonlyArray<{ label: string; value: string }>;

	const handleCreate = async (e: React.FormEvent) => {
		e.preventDefault();
		await runWithPending(async () => {
			dispatch({ type: "createStarted" });
			try {
				let primaryUrl: string;
				let forceDownloadUrl: string | null = null;

				if (mode === "direct") {
					if (fileId == null) {
						throw new Error("fileId is required for direct links");
					}
					const directLink = await fileService.getDirectLinkToken(fileId);
					primaryUrl = fileService.directUrl(directLink.token, name);
					forceDownloadUrl = fileService.forceDownloadUrl(
						directLink.token,
						name,
					);
				} else {
					const expiresAt = computeShareExpiry(expiry);
					const target =
						fileId != null
							? { type: "file" as const, id: fileId }
							: folderId != null
								? { type: "folder" as const, id: folderId }
								: null;
					if (target == null) {
						throw new Error("share target is required");
					}
					const share = await shareService.create({
						target,
						password: password || undefined,
						expires_at: expiresAt ?? undefined,
						max_downloads: normalizeMaxDownloads(maxDownloads),
					});
					primaryUrl = shareService.pageUrl(share.token);
					void Promise.resolve(onShareCreated?.()).catch(() => undefined);
				}

				dispatch({
					type: "createSucceeded",
					links: { primaryUrl, forceDownloadUrl },
				});
				toast.success(t("share:share_created"));
			} catch (error) {
				handleApiError(error);
			} finally {
				dispatch({ type: "createFinished" });
			}
		});
	};

	const handleCopy = async (
		value: string,
		link: "forceDownload" | "primary",
	) => {
		try {
			await writeTextToClipboard(value);
			toast.success(t("copied_to_clipboard"));
			dispatch({ type: "copySucceeded", link });
			setTimeout(() => dispatch({ type: "copyReset", link }), 2000);
		} catch {
			toast.error(t("errors:unexpected_error"));
		}
	};

	const handleClose = (open: boolean) => {
		onOpenChange(open);
	};

	const handleOpenChangeComplete = (open: boolean) => {
		if (!open) {
			dispatch({ type: "reset" });
		}
		onOpenChangeComplete?.(open);
	};

	return (
		<ManagerDialogShell
			open={open}
			onOpenChange={handleClose}
			onOpenChangeComplete={handleOpenChangeComplete}
			title={
				<span className="flex max-w-full min-w-0 items-start gap-2 leading-snug">
					<Icon name="Link" className="mt-0.5 size-4 shrink-0" />
					<span className="min-w-0 flex-1 overflow-hidden break-words">
						{t("share:share_dialog_title", { name })}
					</span>
				</span>
			}
			footer={
				<FixedDialogFooter>
					{createdLinks ? (
						<Button
							type="button"
							variant="outline"
							className="w-full"
							onClick={() => handleClose(false)}
						>
							{t("share:share_done")}
						</Button>
					) : (
						<Button
							form="share-create-form"
							type="submit"
							className="w-full"
							disabled={loading || creating}
						>
							{loading || creating ? (
								<Icon name="Spinner" className="size-4 animate-spin" />
							) : null}
							{loading || creating
								? t("share:share_creating")
								: t("share:share_create_button")}
						</Button>
					)}
				</FixedDialogFooter>
			}
		>
			<ManagerDialogScrollableList>
				{createdLinks ? (
					<div className="space-y-4">
						<div className="flex items-center gap-2">
							<Input
								value={createdLinks.primaryUrl}
								readOnly
								data-testid="share-primary-url"
								className="text-sm"
							/>
							<Button
								variant="outline"
								size="icon"
								onClick={() =>
									void handleCopy(createdLinks.primaryUrl, "primary")
								}
							>
								{copiedLink === "primary" ? (
									<Icon name="Check" className="size-4 text-green-500" />
								) : (
									<Icon name="Copy" className="size-4" />
								)}
							</Button>
						</div>
						{createdLinks.forceDownloadUrl && (
							<div className="space-y-2">
								<Label>{t("share:share_force_download_link")}</Label>
								<div className="flex items-center gap-2">
									<Input
										value={createdLinks.forceDownloadUrl}
										readOnly
										data-testid="share-force-download-url"
										className="text-sm"
									/>
									<Button
										variant="outline"
										size="icon"
										onClick={() =>
											void handleCopy(
												createdLinks.forceDownloadUrl ?? "",
												"forceDownload",
											)
										}
									>
										{copiedLink === "forceDownload" ? (
											<Icon name="Check" className="size-4 text-green-500" />
										) : (
											<Icon name="Copy" className="size-4" />
										)}
									</Button>
								</div>
							</div>
						)}
						{mode === "page" && password && (
							<p className="text-xs text-muted-foreground">
								{t("share:share_password_hint")}
							</p>
						)}
					</div>
				) : (
					<form
						id="share-create-form"
						onSubmit={handleCreate}
						className="space-y-4"
					>
						{mode === "page" ? (
							<>
								<div className="space-y-2">
									<Label htmlFor="share-password">
										{t("share:share_password_optional")}
									</Label>
									<Input
										id="share-password"
										type="password"
										autoComplete="new-password"
										placeholder={t("share:share_password_placeholder")}
										value={password}
										onChange={(e) =>
											dispatch({
												type: "setPassword",
												value: e.target.value,
											})
										}
									/>
								</div>

								<div className="space-y-2">
									<Label>{t("share:share_expiration")}</Label>
									<Select
										items={expiryOptions}
										value={expiry}
										onValueChange={(v) =>
											dispatch({
												type: "setExpiry",
												value: v ?? "never",
											})
										}
									>
										<SelectTrigger>
											<SelectValue />
										</SelectTrigger>
										<SelectContent>
											{expiryOptions.map((option) => (
												<SelectItem key={option.value} value={option.value}>
													{option.label}
												</SelectItem>
											))}
										</SelectContent>
									</Select>
								</div>
							</>
						) : (
							<p className="text-xs text-muted-foreground">
								{t("share:share_direct_mode_hint")}
							</p>
						)}

						{mode === "page" && (
							<div className="space-y-2">
								<Label htmlFor="max-downloads">
									{t("share:share_download_limit")}
								</Label>
								<Input
									id="max-downloads"
									type="number"
									placeholder={t("share:share_download_limit_placeholder")}
									value={maxDownloads}
									onChange={(e) =>
										dispatch({
											type: "setMaxDownloads",
											value: e.target.value,
										})
									}
								/>
							</div>
						)}
					</form>
				)}
			</ManagerDialogScrollableList>
		</ManagerDialogShell>
	);
}
