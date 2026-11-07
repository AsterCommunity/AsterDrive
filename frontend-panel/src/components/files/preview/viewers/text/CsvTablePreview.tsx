import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from "@/components/ui/table";
import { useTextContent } from "@/hooks/useTextContent";
import type { ResourcePath } from "@/lib/resourceRequest";
import type { TablePreviewDelimiterValue } from "@/lib/tablePreview";
import { PreviewError } from "../../shared/PreviewError";
import { PreviewLoadingState } from "../../shared/PreviewLoadingState";
import {
	PreviewSurface,
	PreviewSurfaceContent,
	PreviewSurfaceMessage,
	PreviewSurfaceToolbar,
} from "../../shared/PreviewSurface";
import { parseCsvPreview } from "./structuredPreview";

interface CsvTablePreviewProps {
	resource: ResourcePath;
	delimiter: TablePreviewDelimiterValue;
}

export function CsvTablePreview({ resource, delimiter }: CsvTablePreviewProps) {
	const { t } = useTranslation(["core", "files"]);
	const { content, loading, error, reload } = useTextContent(resource);

	const parsed = useMemo(() => {
		if (!content) return null;
		return parseCsvPreview(content, delimiter);
	}, [content, delimiter]);

	if (loading) {
		return <PreviewLoadingState text={t("files:loading_preview")} />;
	}

	if (error || content === null) {
		return <PreviewError onRetry={() => void reload()} />;
	}

	if (!parsed || parsed.invalid || parsed.rows.length === 0) {
		return (
			<PreviewSurface>
				<PreviewSurfaceMessage tone="danger">
					{t("files:table_parse_failed")}
				</PreviewSurfaceMessage>
			</PreviewSurface>
		);
	}

	const rows = parsed.rows;
	// Preview positions are stable until the entire parsed document changes.
	// Compact keys avoid copying large attacker-controlled cell text per cell.
	const header = (rows[0] ?? []).map((text, column) => ({
		key: String(column),
		text,
	}));
	const body = rows.slice(1).map((cells, row) => ({
		key: String(row),
		cells,
	}));

	return (
		<PreviewSurface>
			<PreviewSurfaceToolbar
				icon="Table"
				label={t("files:preview_mode_table")}
				meta={
					parsed.limited
						? t("files:structured_preview_limited")
						: t("files:preview_mode_formatted")
				}
			/>
			<PreviewSurfaceContent>
				<ScrollArea className="h-full bg-background/80 dark:bg-background/25">
					<Table>
						<TableHeader>
							<TableRow>
								{header.map((cell, index) => (
									<TableHead
										key={cell.key}
										className="sticky top-0 z-10 bg-background whitespace-pre-wrap break-words"
									>
										{cell.text || `${t("column")} ${index + 1}`}
									</TableHead>
								))}
							</TableRow>
						</TableHeader>
						<TableBody>
							{body.map((row) => {
								return (
									<TableRow key={row.key}>
										{header.map((cell, cellIndex) => (
											<TableCell
												key={cell.key}
												className="max-w-80 whitespace-pre-wrap break-words align-top"
											>
												{row.cells[cellIndex] ?? ""}
											</TableCell>
										))}
									</TableRow>
								);
							})}
						</TableBody>
					</Table>
				</ScrollArea>
			</PreviewSurfaceContent>
		</PreviewSurface>
	);
}
