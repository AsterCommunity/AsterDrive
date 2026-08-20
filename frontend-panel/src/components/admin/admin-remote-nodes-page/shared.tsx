import type { TFunction } from "i18next";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { formatDateTime } from "@/lib/format";
import type { RemoteNodeEnrollmentStatus, RemoteNodeInfo } from "@/types/api";
import type { RemoteNodeTransportMode } from "../remoteNodeDialogShared";

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
				<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
			) : result === true ? (
				<Icon name="Check" className="mr-1 size-4 text-green-600" />
			) : result === false ? (
				<Icon name="WifiX" className="mr-1 size-4 text-destructive" />
			) : (
				<Icon name="WifiHigh" className="mr-1 size-4" />
			)}
			{t("test_connection")}
		</Button>
	);
}

export function getRemoteNodeStatusTone(node: RemoteNodeInfo) {
	if (!node.is_enabled) {
		return "border-slate-500/40 bg-slate-500/10 text-slate-600 dark:text-slate-300";
	}

	if (!node.last_probe_at) {
		return "border-blue-500/60 bg-blue-500/10 text-blue-600 dark:text-blue-300";
	}

	if (node.last_probe_error) {
		return "border-amber-500/60 bg-amber-500/10 text-amber-600 dark:text-amber-300";
	}

	return "border-emerald-500/60 bg-emerald-500/10 text-emerald-600 dark:text-emerald-300";
}

export function getRemoteNodeStatusLabel(t: TFunction, node: RemoteNodeInfo) {
	if (!node.is_enabled) {
		return t("remote_node_status_disabled");
	}

	if (!node.last_probe_at) {
		return t("remote_node_status_pending");
	}

	if (node.last_probe_error) {
		return t("remote_node_status_degraded");
	}

	return t("remote_node_status_enabled");
}

export function getRemoteNodeTransportTone(mode: RemoteNodeTransportMode) {
	switch (mode) {
		case "direct":
			return "border-blue-500/60 bg-blue-500/10 text-blue-600 dark:text-blue-300";
		case "reverse_tunnel":
			return "border-cyan-500/60 bg-cyan-500/10 text-cyan-600 dark:text-cyan-300";
		case "auto":
			return "border-violet-500/60 bg-violet-500/10 text-violet-600 dark:text-violet-300";
	}

	const _exhaustive: never = mode;
	return _exhaustive;
}

export function getRemoteNodeTransportLabel(
	t: TFunction,
	mode: RemoteNodeTransportMode,
) {
	switch (mode) {
		case "direct":
			return t("remote_node_transport_direct");
		case "reverse_tunnel":
			return t("remote_node_transport_reverse_tunnel");
		case "auto":
			return t("remote_node_transport_auto");
	}

	const _exhaustive: never = mode;
	return _exhaustive;
}

export function getRemoteNodeTransportBadge(
	t: TFunction,
	mode: RemoteNodeTransportMode,
) {
	return mode === "reverse_tunnel"
		? t("remote_node_transport_test_badge")
		: null;
}

export function getRemoteNodeTunnelTone(node: RemoteNodeInfo) {
	if (node.transport_mode === "direct") {
		return "border-slate-500/40 bg-slate-500/10 text-slate-600 dark:text-slate-300";
	}

	return node.tunnel?.status === "online"
		? "border-emerald-500/60 bg-emerald-500/10 text-emerald-600 dark:text-emerald-300"
		: "border-amber-500/60 bg-amber-500/10 text-amber-600 dark:text-amber-300";
}

export function getRemoteNodeTunnelLabel(t: TFunction, node: RemoteNodeInfo) {
	if (node.transport_mode === "direct") {
		return t("remote_node_tunnel_not_used");
	}

	return node.tunnel?.status === "online"
		? t("remote_node_tunnel_online")
		: t("remote_node_tunnel_offline");
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
