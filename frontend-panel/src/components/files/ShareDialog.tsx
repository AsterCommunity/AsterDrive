import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
	computeShareExpiry,
	normalizeMaxDownloads,
} from "@/components/files/shareDialogShared";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
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
import { handleApiError } from "@/hooks/useApiError";
import { writeTextToClipboard } from "@/lib/clipboard";
import { fileService } from "@/services/fileService";
import { shareService } from "@/services/shareService";

type ShareLinkMode = "page" | "direct";

interface ShareDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	fileId?: number;
	folderId?: number;
	name: string;
	initialMode?: ShareLinkMode;
}

export function ShareDialog({
	open,
	onOpenChange,
	fileId,
	folderId,
	name,
	initialMode,
}: ShareDialogProps) {
	const { t } = useTranslation(["core", "share", "errors"]);
	const directEligible = fileId != null;
	const mode: ShareLinkMode =
		directEligible && initialMode === "direct" ? "direct" : "page";
	const [password, setPassword] = useState("");
	const [expiry, setExpiry] = useState("never");
	const [maxDownloads, setMaxDownloads] = useState("");
	const [createdLinks, setCreatedLinks] = useState<{
		primaryUrl: string;
		forceDownloadUrl: string | null;
	} | null>(null);
	const [copied, setCopied] = useState(false);
	const [loading, setLoading] = useState(false);
	const expiryOptions = [
		{ label: t("share:share_expiry_never"), value: "never" },
		{ label: t("share:share_expiry_1h"), value: "1h" },
		{ label: t("share:share_expiry_1d"), value: "1d" },
		{ label: t("share:share_expiry_7d"), value: "7d" },
		{ label: t("share:share_expiry_30d"), value: "30d" },
	] satisfies ReadonlyArray<{ label: string; value: string }>;

	const handleCreate = async (e: React.FormEvent) => {
		e.preventDefault();
		setLoading(true);
		try {
			let primaryUrl: string;
			let forceDownloadUrl: string | null = null;

			if (mode === "direct") {
				if (fileId == null) {
					throw new Error("fileId is required for direct links");
				}
				const directLink = await fileService.getDirectLinkToken(fileId);
				primaryUrl = fileService.directUrl(directLink.token, name);
				forceDownloadUrl = fileService.forceDownloadUrl(directLink.token, name);
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
			}

			setCreatedLinks({ primaryUrl, forceDownloadUrl });
			toast.success(t("share:share_created"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setLoading(false);
		}
	};

	const handleCopy = async (value: string) => {
		try {
			await writeTextToClipboard(value);
			toast.success(t("copied_to_clipboard"));
			setCopied(true);
			setTimeout(() => setCopied(false), 2000);
		} catch {
			toast.error(t("errors:unexpected_error"));
		}
	};

	const handleClose = (open: boolean) => {
		if (!open) {
			setPassword("");
			setExpiry("never");
			setMaxDownloads("");
			setCreatedLinks(null);
			setCopied(false);
		}
		onOpenChange(open);
	};

	return (
		<Dialog open={open} onOpenChange={handleClose}>
			<DialogContent keepMounted className="max-w-md">
				<DialogHeader className="min-w-0 pr-8">
					<DialogTitle className="flex max-w-full min-w-0 items-start gap-2 leading-snug">
						<Icon name="Link" className="mt-0.5 h-4 w-4 shrink-0" />
						<span className="min-w-0 flex-1 overflow-hidden break-words">
							{t("share:share_dialog_title", { name })}
						</span>
					</DialogTitle>
				</DialogHeader>

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
								onClick={() => void handleCopy(createdLinks.primaryUrl)}
							>
								{copied ? (
									<Icon name="Check" className="h-4 w-4 text-green-500" />
								) : (
									<Icon name="Copy" className="h-4 w-4" />
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
											void handleCopy(createdLinks.forceDownloadUrl ?? "")
										}
									>
										{copied ? (
											<Icon name="Check" className="h-4 w-4 text-green-500" />
										) : (
											<Icon name="Copy" className="h-4 w-4" />
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
						<Button
							variant="outline"
							className="w-full"
							onClick={() => handleClose(false)}
						>
							{t("share:share_done")}
						</Button>
					</div>
				) : (
					<form onSubmit={handleCreate} className="space-y-4">
						{mode === "page" ? (
							<>
								<div className="space-y-2">
									<Label htmlFor="share-password">
										{t("share:share_password_optional")}
									</Label>
									<Input
										id="share-password"
										type="password"
										placeholder={t("share:share_password_placeholder")}
										value={password}
										onChange={(e) => setPassword(e.target.value)}
									/>
								</div>

								<div className="space-y-2">
									<Label>{t("share:share_expiration")}</Label>
									<Select
										items={expiryOptions}
										value={expiry}
										onValueChange={(v) => setExpiry(v ?? "never")}
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
									onChange={(e) => setMaxDownloads(e.target.value)}
								/>
							</div>
						)}

						<Button type="submit" className="w-full" disabled={loading}>
							{loading
								? t("share:share_creating")
								: t("share:share_create_button")}
						</Button>
					</form>
				)}
			</DialogContent>
		</Dialog>
	);
}
