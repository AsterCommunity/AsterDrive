import { type ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { RemoteNodeRemoteStorageTargetSection } from "@/components/admin/admin-remote-nodes-page/RemoteNodeRemoteStorageTargetSection";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type {
	RemoteCreateStorageTargetRequest,
	RemoteNodeInfo,
	RemoteStorageTargetInfo,
	RemoteUpdateStorageTargetRequest,
	StorageConnectorDescriptor,
} from "@/types/api";
import type { RemoteNodeFormData } from "../remoteNodePageShared";
import {
	RemoteNodeDiagnosticsCard,
	RemoteNodeDocsCard,
	RemoteNodeSectionIntro,
	RemoteNodeSummaryCard,
} from "./RemoteNodePageCards";
import type {
	RemoteNodeFieldChangeHandler,
	RemoteNodeSummaryItem,
} from "./RemoteNodePageTypes";
import {
	type TransportModeOption,
	TransportModeSelector,
} from "./TransportModeSelector";

interface RemoteNodeEditFormProps {
	baseUrlValidationMessage: string | null;
	editingNode: RemoteNodeInfo | null;
	enabledToneClass: string;
	form: RemoteNodeFormData;
	remoteStorageTargetConnectorDescriptors: StorageConnectorDescriptor[];
	remoteStorageTargetConnectorDescriptorsError: string | null;
	remoteStorageTargetConnectorDescriptorsLoading: boolean;
	remoteStorageTargets: RemoteStorageTargetInfo[];
	remoteStorageTargetsEnabled: boolean;
	remoteStorageTargetsError: string | null;
	remoteStorageTargetsLoading: boolean;
	modeToneClass: string;
	deploymentPanel?: ReactNode;
	pageTab?: "overview" | "storage-targets";
	onPageTabChange?: (value: string) => void;
	onCreateRemoteStorageTarget?: (
		payload: RemoteCreateStorageTargetRequest,
	) => Promise<void>;
	onDeleteRemoteStorageTarget?: (
		profile: RemoteStorageTargetInfo,
	) => Promise<void>;
	onFieldChange: RemoteNodeFieldChangeHandler;
	onUpdateRemoteStorageTarget?: (
		target_key: string,
		payload: RemoteUpdateStorageTargetRequest,
	) => Promise<void>;
	summaryItems: RemoteNodeSummaryItem[];
	transportOptions: TransportModeOption[];
}

export function RemoteNodeEditForm({
	baseUrlValidationMessage,
	editingNode,
	enabledToneClass,
	form,
	remoteStorageTargetConnectorDescriptors,
	remoteStorageTargetConnectorDescriptorsError,
	remoteStorageTargetConnectorDescriptorsLoading,
	remoteStorageTargets,
	remoteStorageTargetsEnabled,
	remoteStorageTargetsError,
	remoteStorageTargetsLoading,
	modeToneClass,
	deploymentPanel,
	pageTab = "overview",
	onPageTabChange,
	onCreateRemoteStorageTarget,
	onDeleteRemoteStorageTarget,
	onFieldChange,
	onUpdateRemoteStorageTarget,
	summaryItems,
	transportOptions,
}: RemoteNodeEditFormProps) {
	const { t } = useTranslation("admin");
	const [tabDirection, setTabDirection] = useState<"forward" | "backward">(
		"forward",
	);
	const [previousTab, setPreviousTab] = useState(pageTab);
	useEffect(() => {
		if (previousTab === pageTab) return;
		setTabDirection(pageTab === "storage-targets" ? "forward" : "backward");
		setPreviousTab(pageTab);
	}, [pageTab, previousTab]);
	const panelAnimationClass =
		tabDirection === "forward"
			? "animate-in fade-in duration-300 slide-in-from-right-4 motion-reduce:animate-none"
			: "animate-in fade-in duration-300 slide-in-from-left-4 motion-reduce:animate-none";

	const overviewSection = (
		<>
			{deploymentPanel ? (
				<div className="animate-in fade-in slide-in-from-top-1 duration-200 motion-reduce:animate-none">
					{deploymentPanel}
				</div>
			) : null}
			<section className="animate-in fade-in slide-in-from-top-1 rounded-xl bg-muted/30 p-5 duration-200 fill-mode-backwards motion-reduce:animate-none">
				<RemoteNodeSectionIntro
					title={t("remote_node_overview_title")}
					description={t("remote_node_overview_desc")}
				/>
				<div className="grid gap-4 md:grid-cols-2">
					<div className="space-y-2">
						<Label htmlFor="remote-node-name">{t("core:name")}</Label>
						<Input
							id="remote-node-name"
							value={form.name}
							onChange={(event) => onFieldChange("name", event.target.value)}
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							required
						/>
						<p className="text-xs text-muted-foreground">
							{t("remote_node_name_hint")}
						</p>
					</div>
					<div className="space-y-3 md:col-span-2">
						<Label id="remote-node-edit-transport-mode-label">
							{t("remote_node_transport_mode")}
						</Label>
						<TransportModeSelector
							ariaLabelledBy="remote-node-edit-transport-mode-label"
							options={transportOptions}
							value={form.transport_mode}
							onChange={(value) => onFieldChange("transport_mode", value)}
						/>
					</div>
					<div className="space-y-2 md:col-span-2">
						<Label htmlFor="remote-node-base-url">{t("base_url")}</Label>
						<Input
							id="remote-node-base-url"
							value={form.base_url}
							onChange={(event) =>
								onFieldChange("base_url", event.target.value)
							}
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							aria-invalid={baseUrlValidationMessage ? true : undefined}
							placeholder="https://remote.example.com"
						/>
						<p className="text-xs text-muted-foreground">
							{t("remote_node_base_url_hint")}
						</p>
						{baseUrlValidationMessage ? (
							<p className="text-xs text-destructive">
								{baseUrlValidationMessage}
							</p>
						) : null}
					</div>
				</div>
			</section>

			<section className="animate-in fade-in slide-in-from-top-1 rounded-xl bg-muted/30 p-5 duration-200 fill-mode-backwards delay-150 motion-reduce:animate-none">
				<RemoteNodeSectionIntro
					title={t("remote_node_status_settings_title")}
					description={t("remote_node_status_settings_desc")}
				/>
				<div className="space-y-4">
					<div className="space-y-2">
						<div className="flex items-center gap-2">
							<Switch
								id="remote-node-enabled"
								checked={form.is_enabled}
								onCheckedChange={(value) => onFieldChange("is_enabled", value)}
							/>
							<Label htmlFor="remote-node-enabled">
								{t("remote_node_enabled")}
							</Label>
						</div>
						<p className="text-xs text-muted-foreground">
							{t("remote_node_enabled_desc")}
						</p>
					</div>
				</div>
			</section>
		</>
	);

	const storageTargetsSection =
		remoteStorageTargetsEnabled &&
		onCreateRemoteStorageTarget &&
		onUpdateRemoteStorageTarget &&
		onDeleteRemoteStorageTarget ? (
			<RemoteNodeRemoteStorageTargetSection
				surface="plain"
				targets={remoteStorageTargets}
				connectorDescriptors={remoteStorageTargetConnectorDescriptors}
				listViewportClassName="max-h-[min(62vh,36rem)] overflow-y-auto pr-1"
				loading={
					remoteStorageTargetsLoading ||
					remoteStorageTargetConnectorDescriptorsLoading
				}
				errorMessage={
					remoteStorageTargetsError ??
					remoteStorageTargetConnectorDescriptorsError
				}
				onCreateTarget={onCreateRemoteStorageTarget}
				onUpdateTarget={onUpdateRemoteStorageTarget}
				onDeleteTarget={onDeleteRemoteStorageTarget}
			/>
		) : (
			<section className="rounded-xl bg-muted/30 p-5">
				<RemoteNodeSectionIntro
					title={t("remote_node_routing_title")}
					description={t("remote_node_routing_desc")}
				/>
				<p className="text-sm text-muted-foreground">
					{t("remote_node_ingress_profiles_empty_desc")}
				</p>
			</section>
		);

	const detailContent = onPageTabChange ? (
		<Tabs
			value={pageTab}
			onValueChange={onPageTabChange}
			className="flex min-h-0 flex-col"
		>
			<TabsList
				variant="line"
				className="h-auto w-full gap-5 border-b px-0 pb-2"
			>
				<TabsTrigger
					value="overview"
					className="h-10 min-w-0 flex-none rounded-none px-0"
				>
					{t("overview")}
				</TabsTrigger>
				<TabsTrigger
					value="storage-targets"
					className="h-10 min-w-0 flex-none rounded-none px-0"
				>
					{t("remote_node_storage_targets_tab")}
				</TabsTrigger>
			</TabsList>
			<div className="pt-4">
				<TabsContent
					value="overview"
					className={`mt-0 space-y-8 outline-none ${panelAnimationClass}`}
				>
					{overviewSection}
				</TabsContent>
				<TabsContent
					value="storage-targets"
					className={`mt-0 outline-none ${panelAnimationClass}`}
				>
					{storageTargetsSection}
				</TabsContent>
			</div>
		</Tabs>
	) : (
		overviewSection
	);

	return (
		<div className="grid gap-8 lg:grid-cols-[300px_minmax(0,1fr)]">
			<aside className="min-w-0 space-y-4 lg:sticky lg:top-6 lg:self-start">
				<RemoteNodeSummaryCard
					description={t("policy_editor_summary_desc")}
					editingNode={editingNode}
					enabledToneClass={enabledToneClass}
					form={form}
					modeToneClass={modeToneClass}
					summaryItems={summaryItems}
				/>

				{editingNode ? (
					<RemoteNodeDiagnosticsCard editingNode={editingNode} />
				) : (
					<RemoteNodeDocsCard />
				)}
			</aside>

			<div className="min-w-0 space-y-8">{detailContent}</div>
		</div>
	);
}
