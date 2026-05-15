import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { DelimitedListInput } from "@/components/admin/DelimitedListInput";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import {
	formatMediaProcessingDelimitedInput,
	getMediaProcessingConfigIssues,
	type MediaProcessingEditorConfig,
	type MediaProcessingEditorProcessor,
	parseMediaProcessingConfig,
	parseMediaProcessingDelimitedInput,
	serializeMediaProcessingConfig,
} from "./mediaProcessingConfigEditorShared";

interface MediaProcessingConfigEditorProps {
	onChange: (value: string) => void;
	onTestFfmpegCliCommand?: (value: string) => Promise<void>;
	onTestVipsCliCommand?: (value: string) => Promise<void>;
	value: string;
}

function parseDraftValue(value: string): MediaProcessingEditorConfig {
	try {
		return parseMediaProcessingConfig(value);
	} catch {
		return parseMediaProcessingConfig(`{
			"version": 1,
			"processors": []
		}`);
	}
}

function getProcessorLabelKey(kind: MediaProcessingEditorProcessor["kind"]) {
	switch (kind) {
		case "vips_cli":
			return "thumbnail_processor_vips_cli";
		case "ffmpeg_cli":
			return "thumbnail_processor_ffmpeg_cli";
		case "images":
			return "thumbnail_processor_images";
	}
}

export function MediaProcessingConfigEditor({
	onChange,
	onTestFfmpegCliCommand,
	onTestVipsCliCommand,
	value,
}: MediaProcessingConfigEditorProps) {
	const { t } = useTranslation("admin");
	const [draft, setDraft] = useState<MediaProcessingEditorConfig>(() =>
		parseDraftValue(value),
	);
	const [testingProcessorKind, setTestingProcessorKind] = useState<
		MediaProcessingEditorProcessor["kind"] | null
	>(null);

	useEffect(() => {
		setDraft(parseDraftValue(value));
	}, [value]);

	const validationIssues = getMediaProcessingConfigIssues(draft);

	function updateProcessors(
		updater: (
			processors: MediaProcessingEditorConfig["processors"],
		) => MediaProcessingEditorConfig["processors"],
	) {
		const nextDraft = {
			...draft,
			processors: updater(draft.processors),
		};
		setDraft(nextDraft);
		onChange(serializeMediaProcessingConfig(nextDraft));
	}

	function updateProcessor(
		kind: MediaProcessingEditorProcessor["kind"],
		updater: (
			processor: MediaProcessingEditorProcessor,
		) => MediaProcessingEditorProcessor,
	) {
		updateProcessors((processors) =>
			processors.map((processor) =>
				processor.kind === kind ? updater(processor) : processor,
			),
		);
	}

	const handleTestProcessorCommand = useCallback(
		async (kind: MediaProcessingEditorProcessor["kind"]) => {
			const handler =
				kind === "vips_cli"
					? onTestVipsCliCommand
					: kind === "ffmpeg_cli"
						? onTestFfmpegCliCommand
						: undefined;
			if (!handler) {
				return;
			}

			setTestingProcessorKind(kind);
			try {
				await handler(serializeMediaProcessingConfig(draft));
			} finally {
				setTestingProcessorKind(null);
			}
		},
		[draft, onTestFfmpegCliCommand, onTestVipsCliCommand],
	);

	return (
		<div className="space-y-4">
			<div className="space-y-1">
				<p className="text-sm font-medium">
					{t("media_processing_editor_title")}
				</p>
				<p className="text-sm text-muted-foreground">
					{t("media_processing_editor_desc")}
				</p>
			</div>

			{validationIssues.length > 0 ? (
				<div className="rounded-xl border border-destructive/30 bg-destructive/5 p-3 text-sm">
					<p className="font-medium text-destructive">
						{t("media_processing_editor_validation_title")}
					</p>
					<ul className="mt-2 space-y-1 text-destructive">
						{validationIssues.map((issue) => (
							<li key={JSON.stringify([issue.key, issue.values ?? null])}>
								{t(issue.key, issue.values)}
							</li>
						))}
					</ul>
				</div>
			) : null}

			<div className="space-y-3">
				{draft.processors.map((processor) => {
					const isBuiltinFallback = processor.kind === "images";
					const supportsCommand =
						processor.kind === "vips_cli" || processor.kind === "ffmpeg_cli";
					const canTestCommand =
						(processor.kind === "vips_cli" && onTestVipsCliCommand) ||
						(processor.kind === "ffmpeg_cli" && onTestFfmpegCliCommand);

					return (
						<Card key={processor.kind} size="sm">
							<CardHeader>
								<div className="flex flex-wrap items-start justify-between gap-3">
									<div className="space-y-1">
										<CardTitle>
											{t(getProcessorLabelKey(processor.kind))}
										</CardTitle>
										<CardDescription>
											{isBuiltinFallback
												? t("media_processing_editor_processor_builtin_desc")
												: t(
														"media_processing_editor_processor_extensions_desc",
													)}
										</CardDescription>
									</div>
									<div className="flex flex-wrap items-center gap-2">
										<Badge
											variant={processor.enabled ? "secondary" : "outline"}
										>
											{processor.enabled
												? t("media_processing_editor_processor_enabled")
												: t("media_processing_editor_processor_disabled")}
										</Badge>
										{isBuiltinFallback ? (
											<Badge variant="outline">
												{t("media_processing_editor_processor_fallback")}
											</Badge>
										) : null}
									</div>
								</div>
							</CardHeader>
							<CardContent className="space-y-4">
								<div className="flex items-center gap-3 rounded-lg border bg-muted/30 px-3 py-2">
									<Switch
										id={`media-processing-${processor.kind}-enabled`}
										checked={processor.enabled}
										onCheckedChange={(checked) =>
											updateProcessor(processor.kind, (current) => ({
												...current,
												enabled: checked,
											}))
										}
									/>
									<div>
										<p className="text-sm font-medium">
											{processor.enabled
												? t("media_processing_editor_processor_enabled")
												: t("media_processing_editor_processor_disabled")}
										</p>
										<p className="text-xs text-muted-foreground">
											{t("media_processing_editor_processor_enabled_desc")}
										</p>
									</div>
								</div>

								{isBuiltinFallback ? (
									<div className="rounded-lg border border-dashed bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
										{t("media_processing_editor_processor_builtin_hint")}
									</div>
								) : null}

								{!isBuiltinFallback ? (
									<div className="space-y-1.5">
										<p className="text-xs font-medium text-muted-foreground">
											{t("media_processing_editor_rule_extensions_label")}
										</p>
										<DelimitedListInput
											values={processor.extensions}
											formatValue={formatMediaProcessingDelimitedInput}
											parseValue={parseMediaProcessingDelimitedInput}
											onValueChange={(extensions) =>
												updateProcessor(processor.kind, (current) => ({
													...current,
													extensions,
												}))
											}
											placeholder={t(
												"media_processing_editor_rule_extensions_placeholder",
											)}
										/>
										<p className="text-xs text-muted-foreground">
											{t("media_processing_editor_rule_extensions_desc")}
										</p>
									</div>
								) : null}

								{supportsCommand ? (
									<div className="space-y-1.5">
										<p className="text-xs font-medium text-muted-foreground">
											{t("media_processing_editor_processor_command_label")}
										</p>
										<Input
											value={processor.config.command}
											onChange={(event) =>
												updateProcessor(processor.kind, (current) => ({
													...current,
													config: {
														...current.config,
														command: event.target.value,
													},
												}))
											}
											placeholder={t(
												"media_processing_editor_processor_command_placeholder",
											)}
										/>
										<p className="text-xs text-muted-foreground">
											{t("media_processing_editor_processor_command_desc")}
										</p>
										{canTestCommand ? (
											<div className="pt-1">
												<Button
													variant="outline"
													size="sm"
													disabled={testingProcessorKind !== null}
													onClick={() => {
														void handleTestProcessorCommand(processor.kind);
													}}
												>
													{testingProcessorKind === processor.kind
														? t(
																"media_processing_editor_processor_testing_command",
															)
														: t(
																"media_processing_editor_processor_test_command",
															)}
												</Button>
											</div>
										) : null}
									</div>
								) : null}
							</CardContent>
						</Card>
					);
				})}
			</div>
		</div>
	);
}
