import { useEffect, useEffectEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { handleApiError } from "@/hooks/useApiError";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { formatDateAbsolute, formatDateAbsoluteWithOffset } from "@/lib/format";
import {
	createPasskeyCredential,
	isWebAuthnSupported,
	WebAuthnCancelledError,
	WebAuthnUnsupportedError,
} from "@/lib/webauthn";
import { authService } from "@/services/authService";
import type { PasskeyInfo } from "@/types/api";

interface EditablePasskeyName {
	id: number;
	value: string;
}

function lastUsedLabel(passkey: PasskeyInfo, fallback: string) {
	return passkey.last_used_at
		? formatDateAbsolute(passkey.last_used_at)
		: fallback;
}

export function SecurityPasskeysSection() {
	const { t } = useTranslation(["auth", "core", "settings"]);
	const [passkeys, setPasskeys] = useState<PasskeyInfo[]>([]);
	const [loading, setLoading] = useState(false);
	const [creating, setCreating] = useState(false);
	const [name, setName] = useState("");
	const [editing, setEditing] = useState<EditablePasskeyName | null>(null);
	const [busyIds, setBusyIds] = useState<Set<number>>(() => new Set());
	const [supported, setSupported] = useState(false);

	const loadPasskeys = useEffectEvent(async () => {
		try {
			setLoading(true);
			setPasskeys(await authService.listPasskeys());
		} catch (error) {
			handleApiError(error);
		} finally {
			setLoading(false);
		}
	});

	useEffect(() => {
		setSupported(isWebAuthnSupported());
		void loadPasskeys();
	}, []);

	const handleCreate = async () => {
		if (!supported) {
			toast.error(t("auth:passkey_unsupported"));
			return;
		}

		const finalName =
			name.trim() || t("settings:settings_passkeys_default_name");
		try {
			setCreating(true);
			const start = await authService.startPasskeyRegistration({
				name: finalName,
			});
			const credential = await createPasskeyCredential(start.public_key);
			const created = await authService.finishPasskeyRegistration(
				start.flow_id,
				credential,
				finalName,
			);
			setPasskeys((prev) => [created, ...prev]);
			setName("");
			toast.success(t("settings:settings_passkeys_added"));
		} catch (error) {
			if (error instanceof WebAuthnUnsupportedError) {
				toast.error(t("auth:passkey_unsupported"));
				return;
			}
			if (error instanceof WebAuthnCancelledError) {
				toast.error(t("auth:passkey_cancelled"));
				return;
			}
			handleApiError(error);
		} finally {
			setCreating(false);
		}
	};

	const handleRename = async () => {
		if (!editing) return;

		const finalName = editing.value.trim();
		if (!finalName) return;

		try {
			setBusyIds((previous) => new Set(previous).add(editing.id));
			const updated = await authService.renamePasskey(editing.id, {
				name: finalName,
			});
			setPasskeys((prev) =>
				prev.map((passkey) => (passkey.id === updated.id ? updated : passkey)),
			);
			setEditing(null);
			toast.success(t("settings:settings_passkeys_renamed"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setBusyIds((previous) => {
				const next = new Set(previous);
				next.delete(editing.id);
				return next;
			});
		}
	};

	const handleDelete = async (id: number) => {
		try {
			setBusyIds((previous) => new Set(previous).add(id));
			await authService.deletePasskey(id);
			setPasskeys((prev) => prev.filter((passkey) => passkey.id !== id));
			toast.success(t("settings:settings_passkeys_deleted"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setBusyIds((previous) => {
				const next = new Set(previous);
				next.delete(id);
				return next;
			});
		}
	};

	const { requestConfirm, dialogProps } =
		useConfirmDialog<number>(handleDelete);

	return (
		<div className="space-y-4 rounded-xl border bg-background p-4">
			<div className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
				<div className="space-y-1">
					<h3 className="text-sm font-semibold">
						{t("settings:settings_passkeys_section")}
					</h3>
					<p className="text-sm text-muted-foreground">
						{t("settings:settings_passkeys_section_desc")}
					</p>
				</div>
				<Button
					type="button"
					variant="outline"
					disabled={loading}
					onClick={() => void loadPasskeys()}
				>
					{loading ? (
						<Icon name="Spinner" className="mr-2 h-4 w-4 animate-spin" />
					) : (
						<Icon name="ArrowClockwise" className="mr-2 h-4 w-4" />
					)}
					{t("core:refresh")}
				</Button>
			</div>

			<div className="rounded-xl border bg-muted/20 p-4">
				<div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_auto] md:items-end">
					<div className="space-y-2">
						<Label htmlFor="new-passkey-name">
							{t("settings:settings_passkeys_new_name")}
						</Label>
						<Input
							id="new-passkey-name"
							value={name}
							disabled={creating}
							maxLength={128}
							placeholder={t("settings:settings_passkeys_name_placeholder")}
							onChange={(event) => setName(event.target.value)}
						/>
						<p className="text-xs text-muted-foreground">
							{supported
								? t("settings:settings_passkeys_add_hint")
								: t("auth:passkey_unsupported")}
						</p>
					</div>
					<Button
						type="button"
						disabled={creating || !supported}
						onClick={() => void handleCreate()}
					>
						{creating ? (
							<Icon name="Spinner" className="mr-2 h-4 w-4 animate-spin" />
						) : (
							<Icon name="Plus" className="mr-2 h-4 w-4" />
						)}
						{creating
							? t("settings:settings_passkeys_adding")
							: t("settings:settings_passkeys_add")}
					</Button>
				</div>
			</div>

			{loading ? (
				<div className="rounded-xl border border-dashed bg-muted/20 px-4 py-8 text-center text-sm text-muted-foreground">
					{t("core:loading")}
				</div>
			) : passkeys.length === 0 ? (
				<div className="rounded-xl border border-dashed bg-muted/20 px-4 py-8 text-center">
					<p className="text-sm font-medium">
						{t("settings:settings_passkeys_empty")}
					</p>
					<p className="mt-1 text-sm text-muted-foreground">
						{t("settings:settings_passkeys_empty_desc")}
					</p>
				</div>
			) : (
				<div className="space-y-3">
					{passkeys.map((passkey) => {
						const currentEdit = editing?.id === passkey.id ? editing : null;
						const busy = busyIds.has(passkey.id);
						return (
							<div
								key={passkey.id}
								className="rounded-xl border bg-muted/20 p-4"
							>
								<div className="flex flex-col gap-4 md:flex-row md:items-start md:justify-between">
									<div className="min-w-0 flex-1 space-y-3">
										<div className="flex flex-wrap items-center gap-2">
											<div className="rounded-lg border bg-background p-2">
												<Icon name="Shield" className="h-4 w-4" />
											</div>
											{currentEdit ? (
												<Input
													value={currentEdit.value}
													disabled={busy}
													maxLength={128}
													aria-label={t("settings:settings_passkeys_edit_name")}
													className="h-9 max-w-sm"
													onChange={(event) =>
														setEditing({
															id: passkey.id,
															value: event.target.value,
														})
													}
												/>
											) : (
												<p className="break-words text-sm font-semibold">
													{passkey.name}
												</p>
											)}
											{passkey.backed_up ? (
												<Badge variant="secondary">
													{t("settings:settings_passkeys_synced")}
												</Badge>
											) : null}
										</div>
										<div className="grid gap-2 text-xs text-muted-foreground md:grid-cols-2">
											<p>
												{t("settings:settings_passkeys_last_used")}:{" "}
												<span
													title={
														passkey.last_used_at
															? formatDateAbsoluteWithOffset(
																	passkey.last_used_at,
																)
															: undefined
													}
												>
													{lastUsedLabel(
														passkey,
														t("settings:settings_passkeys_never_used"),
													)}
												</span>
											</p>
											<p>
												{t("settings:settings_passkeys_created")}:{" "}
												<span
													title={formatDateAbsoluteWithOffset(
														passkey.created_at,
													)}
												>
													{formatDateAbsolute(passkey.created_at)}
												</span>
											</p>
										</div>
									</div>
									<div className="flex flex-wrap gap-2">
										{currentEdit ? (
											<>
												<Button
													type="button"
													size="sm"
													disabled={
														busy || currentEdit.value.trim().length === 0
													}
													onClick={() => void handleRename()}
												>
													{busy ? (
														<Icon
															name="Spinner"
															className="mr-2 h-4 w-4 animate-spin"
														/>
													) : (
														<Icon name="Check" className="mr-2 h-4 w-4" />
													)}
													{t("core:save")}
												</Button>
												<Button
													type="button"
													size="sm"
													variant="outline"
													disabled={busy}
													onClick={() => setEditing(null)}
												>
													{t("core:cancel")}
												</Button>
											</>
										) : (
											<Button
												type="button"
												size="sm"
												variant="outline"
												disabled={busy}
												onClick={() =>
													setEditing({
														id: passkey.id,
														value: passkey.name,
													})
												}
											>
												<Icon name="PencilSimple" className="mr-2 h-4 w-4" />
												{t("settings:settings_passkeys_rename")}
											</Button>
										)}
										<Button
											type="button"
											size="sm"
											variant="destructive"
											disabled={busy}
											onClick={() => requestConfirm(passkey.id)}
										>
											{busy ? (
												<Icon
													name="Spinner"
													className="mr-2 h-4 w-4 animate-spin"
												/>
											) : (
												<Icon name="Trash" className="mr-2 h-4 w-4" />
											)}
											{t("settings:settings_passkeys_delete")}
										</Button>
									</div>
								</div>
							</div>
						);
					})}
				</div>
			)}

			<ConfirmDialog
				{...dialogProps}
				title={t("settings:settings_passkeys_delete_title")}
				description={t("settings:settings_passkeys_delete_desc")}
				confirmLabel={t("settings:settings_passkeys_delete")}
				variant="destructive"
			/>
		</div>
	);
}
