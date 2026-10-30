import { type ReactNode, useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { handleApiError } from "@/hooks/useApiError";
import { adminContentAccessService } from "@/services/adminContentAccessService";
import type {
	AdminAccessReason,
	AdminContentAccessInfo,
	AdminScopeKind,
} from "@/types/api";

export function AdminContentAccessGate({
	scopeKind,
	targetId,
	children,
	shell,
}: {
	scopeKind: AdminScopeKind;
	targetId: number;
	children: (accessId: string, end: () => void) => ReactNode;
	shell?: (children: ReactNode) => ReactNode;
}) {
	const { t } = useTranslation("admin");
	const reasonId = useId();
	const remarkId = useId();
	const [enabled, setEnabled] = useState<boolean | null>(null);
	const [access, setAccess] = useState<AdminContentAccessInfo | null>(null);
	const [reason, setReason] = useState<AdminAccessReason | null>(null);
	const [remark, setRemark] = useState("");
	const [busy, setBusy] = useState(false);
	const [failed, setFailed] = useState(false);
	useEffect(() => {
		let active = true;
		setAccess(null);
		setEnabled(null);
		setRemark("");
		setReason(null);
		adminContentAccessService
			.status()
			.then((status) => {
				if (active) setEnabled(status.enabled);
			})
			.catch((error) => {
				if (active) {
					setFailed(true);
					handleApiError(error);
				}
			});
		return () => {
			active = false;
		};
	}, []);
	useEffect(() => {
		if (!access) return;
		const timeout = setTimeout(
			() => setAccess(null),
			Math.max(0, Date.parse(access.expires_at) - Date.now()),
		);
		return () => clearTimeout(timeout);
	}, [access]);
	const reasons = (
		[
			"user_support",
			"data_recovery",
			"security_investigation",
			"other",
		] as const
	).map((value) => ({ value, label: t(`content_reason_${value}`) }));
	const begin = async () => {
		if (!reason || busy) return;
		setBusy(true);
		try {
			const created = await adminContentAccessService.create({
				scope_kind: scopeKind,
				target_id: targetId,
				reason,
				remark: remark.trim() || null,
			});
			setRemark("");
			setAccess(created);
		} catch (error) {
			handleApiError(error);
		} finally {
			setBusy(false);
		}
	};
	if (access) return <>{children(access.id, () => setAccess(null))}</>;
	const wrap = shell ?? ((node) => node);
	if (failed) return wrap(<p role="alert">{t("content_access_failed")}</p>);
	if (enabled === null) return wrap(<p>{t("core:loading")}</p>);
	if (!enabled) return wrap(<p>{t("content_access_disabled")}</p>);
	return wrap(
		<div className="space-y-4 rounded-lg border p-5">
			<p className="text-sm text-muted-foreground">
				{t("content_access_explanation")}
			</p>
			<label htmlFor={reasonId} className="grid gap-2 text-sm">
				{t("content_access_reason")}
				<Select
					items={reasons}
					value={reason}
					onValueChange={(value) => setReason(value)}
				>
					<SelectTrigger id={reasonId} aria-label={t("content_access_reason")}>
						<SelectValue placeholder={t("content_access_reason")} />
					</SelectTrigger>
					<SelectContent>
						{reasons.map((item) => (
							<SelectItem key={item.value} value={item.value}>
								{item.label}
							</SelectItem>
						))}
					</SelectContent>
				</Select>
			</label>
			<label htmlFor={remarkId} className="grid gap-2 text-sm">
				{t("content_access_remark")}
				<Input
					id={remarkId}
					maxLength={500}
					value={remark}
					onChange={(event) => setRemark(event.target.value)}
				/>
			</label>
			<Button disabled={!reason || busy} onClick={() => void begin()}>
				{t("content_access_begin")}
			</Button>
		</div>,
	);
}
