import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useDialogActivity } from "@/components/ui/dialog-lifecycle";
import { Icon } from "@/components/ui/icon";
import { PreviewAppsUrlTemplateVariablesDialog } from "./PreviewAppsUrlTemplateVariablesDialog";
import { PreviewAppsAddDialog } from "./preview-apps-config-editor/PreviewAppsAddDialog";
import { PreviewAppsEditDialog } from "./preview-apps-config-editor/PreviewAppsEditDialog";
import { PreviewAppsHeader } from "./preview-apps-config-editor/PreviewAppsHeader";
import { PreviewAppsTable } from "./preview-apps-config-editor/PreviewAppsTable";
import { PreviewAppsWopiDiscoveryDialog } from "./preview-apps-config-editor/PreviewAppsWopiDiscoveryDialog";
import {
	getAppHeading,
	moveActiveAppIndex,
} from "./preview-apps-config-editor/previewAppsConfigEditorPresentation";
import {
	createPreviewAppDraft,
	getPreviewAppsConfigIssues,
	isProtectedBuiltinPreviewAppKey,
	movePreviewEditorItem,
	PREVIEW_APPS_CONFIG_VERSION,
	type PreviewAppsEditorApp,
	type PreviewAppsEditorConfig,
	parsePreviewAppsConfig,
	serializePreviewAppsConfig,
} from "./previewAppsConfigEditorShared";

interface PreviewAppsConfigEditorProps {
	onBuildWopiDiscoveryPreviewConfig?: (input: {
		discoveryUrl: string;
		value: string;
	}) => Promise<string>;
	onChange: (value: string) => void;
	value: string;
}

export function PreviewAppsConfigEditor({
	onBuildWopiDiscoveryPreviewConfig,
	onChange,
	value,
}: PreviewAppsConfigEditorProps) {
	const { i18n, t } = useTranslation(["admin", "files"]);
	const [addAppDialogOpen, setAddAppDialogOpen] = useState(false);
	const [buildingWopiDiscoveryConfig, setBuildingWopiDiscoveryConfig] =
		useState(false);
	const [editingAppIndex, setEditingAppIndex] = useState<number | null>(null);
	const [
		activeUrlTemplateVariableAppIndex,
		setActiveUrlTemplateVariableAppIndex,
	] = useState<number | null>(null);
	const [wopiDiscoveryDialogOpen, setWopiDiscoveryDialogOpen] = useState(false);
	const [wopiDiscoveryUrl, setWopiDiscoveryUrl] = useState("");

	const parsed = useMemo(() => {
		try {
			const draft = parsePreviewAppsConfig(value);
			return {
				draft,
				issues: getPreviewAppsConfigIssues(draft),
			};
		} catch {
			return {
				draft: null,
				issues: [{ key: "preview_apps_error_parse" }],
			};
		}
	}, [value]);
	const replaceDraft = useCallback(
		(nextDraft: PreviewAppsEditorConfig) => {
			onChange(serializePreviewAppsConfig(nextDraft));
		},
		[onChange],
	);

	const updateDraft = useCallback(
		(
			updater: (current: PreviewAppsEditorConfig) => PreviewAppsEditorConfig,
		) => {
			if (!parsed.draft) {
				return;
			}

			replaceDraft(updater(parsed.draft));
		},
		[parsed.draft, replaceDraft],
	);

	const recoverDraft = useCallback(() => {
		replaceDraft({
			apps: [createPreviewAppDraft([])],
			version: PREVIEW_APPS_CONFIG_VERSION,
		});
		setEditingAppIndex(0);
	}, [replaceDraft]);

	const addEmbedApp = useCallback(() => {
		if (!parsed.draft) {
			return;
		}

		setAddAppDialogOpen(false);
		setEditingAppIndex(parsed.draft.apps.length);
		updateDraft((current) => ({
			...current,
			apps: [
				...current.apps,
				createPreviewAppDraft(current.apps.map((app) => app.key)),
			],
		}));
	}, [parsed.draft, updateDraft]);

	const isDiscoveryCurrent = useDialogActivity(
		wopiDiscoveryUrl,
		wopiDiscoveryDialogOpen,
	);
	const buildWopiDiscoveryConfig = useCallback(async () => {
		if (!onBuildWopiDiscoveryPreviewConfig) {
			return;
		}

		const discoveryUrl = wopiDiscoveryUrl.trim();
		if (!discoveryUrl) {
			return;
		}

		setBuildingWopiDiscoveryConfig(true);
		try {
			const nextValue = await onBuildWopiDiscoveryPreviewConfig({
				discoveryUrl,
				value,
			});
			if (!isDiscoveryCurrent()) return;
			onChange(nextValue);
			setWopiDiscoveryDialogOpen(false);
			setWopiDiscoveryUrl("");
		} catch {
			// Errors are handled by the caller so the dialog can stay open for retry.
		} finally {
			if (isDiscoveryCurrent()) setBuildingWopiDiscoveryConfig(false);
		}
	}, [
		onBuildWopiDiscoveryPreviewConfig,
		onChange,
		value,
		wopiDiscoveryUrl,
		isDiscoveryCurrent,
	]);

	const updateApp = useCallback(
		(
			index: number,
			updater: (app: PreviewAppsEditorApp) => PreviewAppsEditorApp,
		) => {
			updateDraft((current) => ({
				...current,
				apps: current.apps.map((app, appIndex) =>
					appIndex === index ? updater(app) : app,
				),
			}));
		},
		[updateDraft],
	);

	useEffect(() => {
		if (!parsed.draft) {
			setAddAppDialogOpen(false);
			setEditingAppIndex(null);
			setActiveUrlTemplateVariableAppIndex(null);
			setWopiDiscoveryDialogOpen(false);
			setWopiDiscoveryUrl("");
			return;
		}

		setEditingAppIndex((current) => {
			if (current === null) {
				return null;
			}
			return current < parsed.draft.apps.length ? current : null;
		});
		setActiveUrlTemplateVariableAppIndex((current) => {
			if (current === null) {
				return null;
			}
			return current < parsed.draft.apps.length ? current : null;
		});
	}, [parsed.draft]);

	if (!parsed.draft) {
		return (
			<div className="space-y-3 rounded-xl border border-destructive/30 bg-destructive/5 p-4">
				<div className="flex items-start gap-3">
					<Icon name="Warning" className="mt-0.5 size-4 text-destructive" />
					<div className="space-y-1">
						<p className="text-sm font-medium text-destructive">
							{t("preview_apps_validation_error")}
						</p>
						<p className="text-sm text-muted-foreground">
							{t("preview_apps_error_parse")}
						</p>
					</div>
				</div>
				<Button variant="outline" size="sm" onClick={recoverDraft}>
					<Icon name="ArrowCounterClockwise" className="size-4" />
					{t("preview_apps_recover")}
				</Button>
			</div>
		);
	}

	const draft = parsed.draft;
	const issueKeys = parsed.issues.map(
		(issue, issueIndex) => `${issue.key}::${issueIndex}`,
	);
	const appRowKeys = draft.apps.map((_app, index) => `app-row-${index}`);
	const activeUrlTemplateVariableApp =
		activeUrlTemplateVariableAppIndex === null
			? null
			: (draft.apps[activeUrlTemplateVariableAppIndex] ?? null);
	const activeEditingApp =
		editingAppIndex === null ? null : (draft.apps[editingAppIndex] ?? null);
	const activeEditingAppName = activeEditingApp
		? getAppHeading(activeEditingApp, editingAppIndex ?? 0, i18n?.language, t)
		: "";
	const activeEditingAppProtectedBuiltin = activeEditingApp
		? isProtectedBuiltinPreviewAppKey(activeEditingApp.key)
		: false;
	const activeUrlTemplateVariableAppName = activeUrlTemplateVariableApp
		? getAppHeading(
				activeUrlTemplateVariableApp,
				activeUrlTemplateVariableAppIndex ?? 0,
				i18n?.language,
				t,
			)
		: "";
	const handleMoveApp = (index: number, direction: -1 | 1) => {
		setEditingAppIndex((current) =>
			moveActiveAppIndex(current, index, direction, draft.apps.length),
		);
		updateDraft((current) => ({
			...current,
			apps: movePreviewEditorItem(current.apps, index, direction),
		}));
	};
	const handleDeleteApp = (index: number) => {
		const app = draft.apps[index];
		if (app && isProtectedBuiltinPreviewAppKey(app.key)) {
			return;
		}

		setEditingAppIndex((current) => {
			if (current === null) {
				return null;
			}
			if (current === index) {
				return null;
			}
			return current > index ? current - 1 : current;
		});
		updateDraft((current) => ({
			...current,
			apps: current.apps.filter((_app, appIndex) => appIndex !== index),
		}));
	};

	return (
		<>
			<section className="space-y-4">
				<PreviewAppsHeader
					draft={draft}
					issueKeys={issueKeys}
					issues={parsed.issues}
					onAddApp={() => setAddAppDialogOpen(true)}
				/>
				<PreviewAppsTable
					appRowKeys={appRowKeys}
					draft={draft}
					editingAppIndex={editingAppIndex}
					language={i18n?.language}
					onDeleteApp={handleDeleteApp}
					onEditApp={setEditingAppIndex}
					onMoveApp={handleMoveApp}
					onUpdateApp={updateApp}
				/>
			</section>

			<PreviewAppsAddDialog
				canBuildWopiDiscoveryConfig={Boolean(onBuildWopiDiscoveryPreviewConfig)}
				open={addAppDialogOpen}
				onAddEmbedApp={addEmbedApp}
				onOpenChange={setAddAppDialogOpen}
				onOpenWopiDiscovery={() => {
					setAddAppDialogOpen(false);
					setWopiDiscoveryDialogOpen(true);
				}}
			/>

			<PreviewAppsWopiDiscoveryDialog
				building={buildingWopiDiscoveryConfig}
				open={wopiDiscoveryDialogOpen}
				url={wopiDiscoveryUrl}
				onBuild={() => void buildWopiDiscoveryConfig()}
				onOpenChange={(open) => {
					setWopiDiscoveryDialogOpen(open);
					if (!open) {
						setWopiDiscoveryUrl("");
						setBuildingWopiDiscoveryConfig(false);
					}
				}}
				onUrlChange={setWopiDiscoveryUrl}
			/>

			<PreviewAppsEditDialog
				app={activeEditingApp}
				appName={activeEditingAppName}
				index={editingAppIndex ?? 0}
				open={activeEditingApp !== null}
				protectedBuiltin={activeEditingAppProtectedBuiltin}
				updateApp={updateApp}
				updateDraft={updateDraft}
				onOpenChange={(open) => {
					if (!open) {
						setEditingAppIndex(null);
						setActiveUrlTemplateVariableAppIndex(null);
					}
				}}
				onOpenUrlTemplateVariables={() =>
					setActiveUrlTemplateVariableAppIndex(editingAppIndex ?? 0)
				}
			>
				<PreviewAppsUrlTemplateVariablesDialog
					appName={activeUrlTemplateVariableAppName}
					open={activeUrlTemplateVariableAppIndex !== null}
					t={t}
					onOpenChange={(open) => {
						if (!open) {
							setActiveUrlTemplateVariableAppIndex(null);
						}
					}}
				/>
			</PreviewAppsEditDialog>
		</>
	);
}
