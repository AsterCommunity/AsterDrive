import type { TFunction } from "i18next";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { formatDateTime } from "@/lib/format";
import type { RemoteNodeEnrollmentStatus, RemoteNodeInfo } from "@/types/api";

export function TestConnectionButton({
	disabled = false,
	onTest,
}: {
	disabled?: boolean;
	onTest: () => Promise<boolean>;
}) {
	const { t } = useTranslation("admin");
	const [testing, setTesting] = useState(false);
	const [result, setResult] = useState<boolean | null>(null);

	const handleTest = async () => {
		setTesting(true);
		setResult(null);
		const passed = await onTest();
		setResult(passed);
		setTesting(false);
	};

	return (
		<Button
			type="button"
			variant="outline"
			className={ADMIN_CONTROL_HEIGHT_CLASS}
			disabled={disabled || testing}
			onClick={handleTest}
		>
			{testing ? (
				<Icon name="Spinner" className="mr-1 h-4 w-4 animate-spin" />
			) : result === true ? (
				<Icon name="Check" className="mr-1 h-4 w-4 text-green-600" />
			) : (
				<Icon name="WifiHigh" className="mr-1 h-4 w-4" />
			)}
			{t("test_connection")}
		</Button>
	);
}

export function getRemoteNodeStatusTone(node: RemoteNodeInfo) {
	if (!node.is_enabled) {
		return "border-slate-500/40 bg-slate-500/10 text-slate-600 dark:text-slate-300";
	}

	if (!node.last_checked_at) {
		return "border-blue-500/60 bg-blue-500/10 text-blue-600 dark:text-blue-300";
	}

	if (node.last_error) {
		return "border-amber-500/60 bg-amber-500/10 text-amber-600 dark:text-amber-300";
	}

	return "border-emerald-500/60 bg-emerald-500/10 text-emerald-600 dark:text-emerald-300";
}

export function getRemoteNodeStatusLabel(t: TFunction, node: RemoteNodeInfo) {
	if (!node.is_enabled) {
		return t("remote_node_status_disabled");
	}

	if (!node.last_checked_at) {
		return t("remote_node_status_pending");
	}

	if (node.last_error) {
		return t("remote_node_status_degraded");
	}

	return t("remote_node_status_enabled");
}

export function getRemoteNodeEnrollmentStatusTone(
	status: RemoteNodeEnrollmentStatus,
) {
	switch (status) {
		case "completed":
			return "border-emerald-500/60 bg-emerald-500/10 text-emerald-600 dark:text-emerald-300";
		case "pending":
			return "border-blue-500/60 bg-blue-500/10 text-blue-600 dark:text-blue-300";
		case "redeemed":
			return "border-cyan-500/60 bg-cyan-500/10 text-cyan-600 dark:text-cyan-300";
		case "expired":
			return "border-amber-500/60 bg-amber-500/10 text-amber-600 dark:text-amber-300";
		case "not_started":
			return "border-slate-500/40 bg-slate-500/10 text-slate-600 dark:text-slate-300";
	}

	const _exhaustive: never = status;
	return _exhaustive;
}

export function getRemoteNodeEnrollmentStatusLabel(
	t: TFunction,
	status: RemoteNodeEnrollmentStatus,
) {
	switch (status) {
		case "completed":
			return t("remote_node_enrollment_status_completed");
		case "pending":
			return t("remote_node_enrollment_status_pending");
		case "redeemed":
			return t("remote_node_enrollment_status_redeemed");
		case "expired":
			return t("remote_node_enrollment_status_expired");
		case "not_started":
			return t("remote_node_enrollment_status_not_started");
	}

	const _exhaustive: never = status;
	return _exhaustive;
}

export function hasCompletedRemoteNodeEnrollment(node: RemoteNodeInfo) {
	return node.enrollment_status === "completed";
}

export function formatLastChecked(
	t: TFunction,
	lastCheckedAt: string | null | undefined,
) {
	return lastCheckedAt
		? formatDateTime(lastCheckedAt)
		: t("remote_node_never_checked");
}
