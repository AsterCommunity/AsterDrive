import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import type {
	RemoteDownloadStrategy,
	RemoteNodeInfo,
	RemoteStorageTargetInfo,
	RemoteUploadStrategy,
} from "@/types/api";
import type {
	SelectOption,
	SharedFieldProps,
	Translate,
} from "./StoragePolicyFieldTypes";
import { StrategySelectField } from "./StoragePolicyStrategyFields";

export function RemoteNodeField({
	error,
	form,
	onFieldChange,
	remoteNodes,
	remoteStorageTargets = [],
	remoteStorageTargetsError = null,
	remoteStorageTargetsLoading = false,
	showCreateValidation = false,
	t,
}: SharedFieldProps & {
	error: string | null;
	remoteNodes: RemoteNodeInfo[];
	remoteStorageTargets?: RemoteStorageTargetInfo[];
	remoteStorageTargetsError?: string | null;
	remoteStorageTargetsLoading?: boolean;
	showCreateValidation?: boolean;
}) {
	const remoteNodeOptions = remoteNodes.map((node) => ({
		label: node.name,
		value: String(node.id),
	}));
	const selectedRemoteNode =
		remoteNodes.find((node) => String(node.id) === form.remote_node_id) ?? null;
	const targetOptions = remoteStorageTargets.map((target) => ({
		label: target.is_default
			? `${target.name} (${t("core:default")})`
			: target.name,
		value: target.target_key,
	}));
	const selectedTarget =
		remoteStorageTargets.find(
			(target) => target.target_key === form.remote_storage_target_key,
		) ?? null;

	return (
		<div className="space-y-2">
			<Label htmlFor="remote_node_id">{t("remote_node")}</Label>
			<Select
				items={remoteNodeOptions}
				value={form.remote_node_id || "__none__"}
				onValueChange={(value) => {
					onFieldChange(
						"remote_node_id",
						value == null || value === "__none__" ? "" : value,
					);
					onFieldChange("remote_storage_target_key", "");
				}}
			>
				<SelectTrigger id="remote_node_id">
					<SelectValue />
				</SelectTrigger>
				<SelectContent>
					<SelectItem value="__none__">
						{t("select_remote_node_placeholder")}
					</SelectItem>
					{remoteNodeOptions.map((option) => (
						<SelectItem key={option.value} value={option.value}>
							{option.label}
						</SelectItem>
					))}
				</SelectContent>
			</Select>
			{showCreateValidation && error && !form.remote_node_id ? (
				<p className="text-xs text-destructive">{error}</p>
			) : null}
			{selectedRemoteNode ? (
				<p className="text-xs text-muted-foreground">
					{t("policy_wizard_remote_node_hint", {
						base_url:
							selectedRemoteNode.base_url ||
							t("policy_wizard_remote_base_url_empty"),
					})}
				</p>
			) : remoteNodes.length === 0 ? (
				<p className="text-xs text-muted-foreground">
					{t("policy_wizard_remote_nodes_empty")}
				</p>
			) : null}
			{selectedRemoteNode ? (
				<div className="space-y-2 pt-2">
					<Label htmlFor="remote_storage_target_key">
						{t("remote_storage_target")}
					</Label>
					<Select
						items={targetOptions}
						value={form.remote_storage_target_key || "__none__"}
						onValueChange={(value) =>
							onFieldChange(
								"remote_storage_target_key",
								value == null || value === "__none__" ? "" : value,
							)
						}
						disabled={remoteStorageTargetsLoading}
					>
						<SelectTrigger id="remote_storage_target_key">
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							<SelectItem value="__none__">
								{remoteStorageTargetsLoading
									? t("remote_storage_targets_loading")
									: t("select_remote_storage_target_placeholder")}
							</SelectItem>
							{targetOptions.map((option) => (
								<SelectItem key={option.value} value={option.value}>
									{option.label}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
					{error && form.remote_node_id ? (
						<p className="text-xs text-destructive">{error}</p>
					) : null}
					{remoteStorageTargetsError ? (
						<p className="text-xs text-destructive">
							{remoteStorageTargetsError}
						</p>
					) : selectedTarget ? (
						<p className="text-xs text-muted-foreground">
							{t("remote_storage_target_hint", {
								driver: selectedTarget.driver_type,
								base_path: selectedTarget.base_path || "/",
							})}
						</p>
					) : !remoteStorageTargetsLoading &&
						remoteStorageTargets.length === 0 ? (
						<p className="text-xs text-muted-foreground">
							{t("remote_storage_targets_empty")}
						</p>
					) : null}
				</div>
			) : null}
		</div>
	);
}

export function RemoteDownloadStrategyField({
	form,
	onFieldChange,
	t,
}: SharedFieldProps) {
	const options = [
		{
			label: t("download_strategy_relay_stream"),
			value: "relay_stream",
		},
		{
			label: t("download_strategy_presigned"),
			value: "presigned",
		},
	] satisfies ReadonlyArray<SelectOption<RemoteDownloadStrategy>>;

	return (
		<StrategySelectField
			id="remote_download_strategy"
			label={t("remote_download_strategy")}
			options={options}
			value={form.remote_download_strategy}
			onChange={(value) => onFieldChange("remote_download_strategy", value)}
			description={t(
				form.remote_download_strategy === "relay_stream"
					? "download_strategy_relay_stream_desc"
					: "download_strategy_presigned_desc",
			)}
		/>
	);
}

export function RemoteUploadStrategyField({
	form,
	onFieldChange,
	t,
}: SharedFieldProps) {
	const options = [
		{
			label: t("upload_strategy_relay_stream"),
			value: "relay_stream",
		},
		{
			label: t("upload_strategy_presigned"),
			value: "presigned",
		},
	] satisfies ReadonlyArray<SelectOption<RemoteUploadStrategy>>;

	return (
		<StrategySelectField
			id="remote_upload_strategy"
			label={t("remote_upload_strategy")}
			options={options}
			value={form.remote_upload_strategy}
			onChange={(value) => onFieldChange("remote_upload_strategy", value)}
			description={t(
				form.remote_upload_strategy === "relay_stream"
					? "upload_strategy_relay_stream_desc"
					: "upload_strategy_presigned_desc",
			)}
		/>
	);
}

export function RemoteRulesHelper({ t }: { t: Translate }) {
	return (
		<div className="rounded-2xl border border-dashed border-border/80 bg-muted/20 p-4 text-sm text-muted-foreground">
			{t("policy_wizard_remote_rules_helper")}
		</div>
	);
}
