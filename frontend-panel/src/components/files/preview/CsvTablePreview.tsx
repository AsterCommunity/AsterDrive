import Papa from "papaparse";
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
import type { TablePreviewDelimiterValue } from "@/lib/tablePreview";
import { PreviewError } from "./PreviewError";
import { PreviewLoadingState } from "./PreviewLoadingState";
import {
	PreviewSurface,
	PreviewSurfaceContent,
	PreviewSurfaceMessage,
	PreviewSurfaceToolbar,
} from "./PreviewSurface";

interface CsvTablePreviewProps {
	path: string;
	delimiter: TablePreviewDelimiterValue;
}

const MAX_ROWS = 500;

export function CsvTablePreview({ path, delimiter }: CsvTablePreviewProps) {
	const { t } = useTranslation(["core", "files"]);
	const { content, loading, error, reload } = useTextContent(path);

	const parsed = useMemo(() => {
		if (!content) return null;
		return Papa.parse<string[]>(content, {
			...(delimiter === "auto"
				? { delimitersToGuess: [",", "\t", ";", "|"] }
				: { delimiter }),
			skipEmptyLines: true,
		});
	}, [content, delimiter]);

	if (loading) {
		return <PreviewLoadingState text={t("files:loading_preview")} />;
	}

	if (error || content === null) {
		return <PreviewError onRetry={() => void reload()} />;
	}

	if (!parsed || parsed.errors.length > 0 || parsed.data.length === 0) {
		return (
			<PreviewSurface>
				<PreviewSurfaceMessage tone="danger">
					{t("files:table_parse_failed")}
				</PreviewSurfaceMessage>
			</PreviewSurface>
		);
	}

	const rows = parsed.data.slice(0, MAX_ROWS);
	const header = rows[0] ?? [];
	const body = rows.slice(1);
	const headerKey = header.join("|");

	return (
		<PreviewSurface>
			<PreviewSurfaceToolbar
				icon="Table"
				label={t("files:preview_mode_table")}
				meta={
					parsed.data.length > MAX_ROWS
						? t("files:table_truncated", { count: MAX_ROWS })
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
										key={`header-${headerKey}-${cell || `column-${index + 1}`}`}
										className="sticky top-0 z-10 bg-background whitespace-pre-wrap break-words"
									>
										{cell || `${t("column")} ${index + 1}`}
									</TableHead>
								))}
							</TableRow>
						</TableHeader>
						<TableBody>
							{body.map((row) => {
								const rowKey = row.join("|");
								return (
									<TableRow key={`row-${rowKey}`}>
										{header.map((_, cellIndex) => (
											<TableCell
												key={`cell-${rowKey}-${header[cellIndex] ?? `column-${cellIndex + 1}`}`}
												className="max-w-80 whitespace-pre-wrap break-words align-top"
											>
												{row[cellIndex] ?? ""}
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
