import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Icon } from "@/components/ui/icon";
import { Switch } from "@/components/ui/switch";
import {
	getOfflineDownloadEngineConfigIssues,
	type OfflineDownloadEngineEditorConfig,
	type OfflineDownloadEngineKind,
	parseOfflineDownloadEngineConfig,
	serializeOfflineDownloadEngineConfig,
} from "./offlineDownloadEngineRegistryShared";

interface OfflineDownloadEngineRegistryEditorProps {
	onChange: (value: string) => void;
	onTestAria2Rpc?: (value: string) => Promise<void>;
	value: string;
}

function parseDraftValue(value: string): OfflineDownloadEngineEditorConfig {
	try {
		return parseOfflineDownloadEngineConfig(value);
	} catch (error) {
		console.error("Failed to parse offline download engine registry draft", {
			error,
			value,
		});
		return parseOfflineDownloadEngineConfig("");
	}
}

function validationIssueKey(
	issue: ReturnType<typeof getOfflineDownloadEngineConfigIssues>[number],
) {
	const values = Object.entries(issue.values ?? {})
		.sort(([left], [right]) => left.localeCompare(right))
		.map(([key, value]) => `${key}:${value}`)
		.join(",");
	return `${issue.key}:${values}`;
}

function getEngineLabelKey(kind: OfflineDownloadEngineKind) {
	return `offline_download_engine_editor_${kind}_label`;
}

function getEngineDescriptionKey(kind: OfflineDownloadEngineKind) {
	return `offline_download_engine_editor_${kind}_desc`;
}

export function OfflineDownloadEngineRegistryEditor({
	onChange,
	onTestAria2Rpc,
	value,
}: OfflineDownloadEngineRegistryEditorProps) {
	const { t } = useTranslation("admin");
	const draft = useMemo(() => parseDraftValue(value), [value]);
	const [testingAria2, setTestingAria2] = useState(false);

	const validationIssues = getOfflineDownloadEngineConfigIssues(draft);
	const enabledCount = draft.engines.filter((engine) => engine.enabled).length;

	function updateDraft(
		updater: (
			current: OfflineDownloadEngineEditorConfig,
		) => OfflineDownloadEngineEditorConfig,
	) {
		const nextDraft = updater(draft);
		onChange(serializeOfflineDownloadEngineConfig(nextDraft));
	}

	function updateEngine(
		kind: OfflineDownloadEngineKind,
		updater: (engine: {
			kind: OfflineDownloadEngineKind;
			enabled: boolean;
		}) => {
			kind: OfflineDownloadEngineKind;
			enabled: boolean;
		},
	) {
		updateDraft((current) => ({
			...current,
			engines: current.engines.map((engine) =>
				engine.kind === kind ? updater(engine) : engine,
			),
		}));
	}

	function moveEngine(kind: OfflineDownloadEngineKind, direction: -1 | 1) {
		updateDraft((current) => {
			const index = current.engines.findIndex((engine) => engine.kind === kind);
			const target = index + direction;
			if (index < 0 || target < 0 || target >= current.engines.length) {
				return current;
			}
			const engines = [...current.engines];
			const [engine] = engines.splice(index, 1);
			engines.splice(target, 0, engine);
			return { ...current, engines };
		});
	}

	const handleTestAria2Rpc = useCallback(async () => {
		if (!onTestAria2Rpc) return;
		setTestingAria2(true);
		try {
			await onTestAria2Rpc(serializeOfflineDownloadEngineConfig(draft));
		} finally {
			setTestingAria2(false);
		}
	}, [draft, onTestAria2Rpc]);

	return (
		<div className="space-y-4">
			<div className="space-y-1">
				<p className="text-sm font-medium">
					{t("offline_download_engine_editor_title")}
				</p>
			</div>

			{enabledCount === 0 ? (
				<div className="rounded-lg border border-dashed bg-muted/20 px-3 py-2 text-sm text-muted-foreground">
					{t("offline_download_engine_editor_disabled_hint")}
				</div>
			) : null}

			{validationIssues.length > 0 ? (
				<div className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm">
					<p className="font-medium text-destructive">
						{t("offline_download_engine_editor_validation_title")}
					</p>
					<ul className="mt-2 space-y-1 text-destructive">
						{validationIssues.map((issue) => (
							<li key={validationIssueKey(issue)}>
								{t(issue.key, issue.values)}
							</li>
						))}
					</ul>
				</div>
			) : null}

			<div className="space-y-3">
				{draft.engines.map((engine, index) => {
					const canMoveUp = index > 0;
					const canMoveDown = index + 1 < draft.engines.length;
					const isAria2 = engine.kind === "aria2";

					return (
						<Card key={engine.kind} size="sm">
							<CardHeader>
								<div className="flex flex-wrap items-start justify-between gap-3">
									<div className="space-y-1">
										<CardTitle>{t(getEngineLabelKey(engine.kind))}</CardTitle>
										<CardDescription>
											{t(getEngineDescriptionKey(engine.kind))}
										</CardDescription>
									</div>
									<div className="flex flex-wrap items-center gap-2">
										<Badge variant={engine.enabled ? "secondary" : "outline"}>
											{engine.enabled
												? t("offline_download_engine_editor_enabled")
												: t("offline_download_engine_editor_disabled")}
										</Badge>
										{engine.enabled ? (
											<Badge variant="outline">
												{t("offline_download_engine_editor_priority", {
													index: index + 1,
												})}
											</Badge>
										) : null}
									</div>
								</div>
							</CardHeader>
							<CardContent className="space-y-4">
								<div className="flex items-center gap-3 rounded-lg border bg-muted/30 px-3 py-2">
									<Switch
										id={`offline-download-engine-${engine.kind}`}
										checked={engine.enabled}
										onCheckedChange={(checked) =>
											updateEngine(engine.kind, (current) => ({
												...current,
												enabled: checked,
											}))
										}
									/>
									<div>
										<p className="text-sm font-medium">
											{engine.enabled
												? t("offline_download_engine_editor_enabled")
												: t("offline_download_engine_editor_disabled")}
										</p>
									</div>
								</div>

								<div className="flex flex-wrap items-center gap-2">
									<Button
										type="button"
										variant="outline"
										size="sm"
										disabled={!canMoveUp}
										onClick={() => moveEngine(engine.kind, -1)}
									>
										<Icon name="ArrowUp" className="size-3.5" />
										{t("offline_download_engine_editor_move_up")}
									</Button>
									<Button
										type="button"
										variant="outline"
										size="sm"
										disabled={!canMoveDown}
										onClick={() => moveEngine(engine.kind, 1)}
									>
										<Icon name="ArrowDown" className="size-3.5" />
										{t("offline_download_engine_editor_move_down")}
									</Button>
									{isAria2 && onTestAria2Rpc ? (
										<Button
											type="button"
											variant="outline"
											size="sm"
											disabled={testingAria2}
											onClick={() => {
												handleTestAria2Rpc().catch((error) => {
													console.error("Failed to test aria2 RPC", error);
												});
											}}
										>
											<Icon name="WifiHigh" className="size-3.5" />
											{testingAria2
												? t("offline_download_engine_editor_testing_aria2")
												: t("offline_download_engine_editor_test_aria2")}
										</Button>
									) : null}
								</div>
							</CardContent>
						</Card>
					);
				})}
			</div>
		</div>
	);
}
