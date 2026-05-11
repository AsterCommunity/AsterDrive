import { lazy, Suspense } from "react";
import { useTranslation } from "react-i18next";
import { normalizeTablePreviewDelimiter } from "@/lib/tablePreview";
import type {
	FileInfo,
	FileListItem,
	PreviewLinkInfo,
	ShareStreamSessionInfo,
	WopiLaunchSession,
} from "@/types/api";
import { BlobMediaPreview } from "./BlobMediaPreview";
import type { detectFilePreviewProfile } from "./file-capabilities";
import { PreviewLoadingState } from "./PreviewLoadingState";
import { PreviewUnavailable } from "./PreviewUnavailable";
import type { OpenWithOption } from "./types";
import { UrlTemplatePreview } from "./UrlTemplatePreview";
import { VideoPreview } from "./VideoPreview";
import { WopiPreview } from "./WopiPreview";

const PdfPreview = lazy(async () => {
	const module = await import("./PdfPreview");
	return { default: module.PdfPreview };
});

const MarkdownPreview = lazy(async () => {
	const module = await import("./MarkdownPreview");
	return { default: module.MarkdownPreview };
});

const CsvTablePreview = lazy(async () => {
	const module = await import("./CsvTablePreview");
	return { default: module.CsvTablePreview };
});

const JsonPreview = lazy(async () => {
	const module = await import("./JsonPreview");
	return { default: module.JsonPreview };
});

const XmlPreview = lazy(async () => {
	const module = await import("./XmlPreview");
	return { default: module.XmlPreview };
});

const TextCodePreview = lazy(async () => {
	const module = await import("./TextCodePreview");
	return { default: module.TextCodePreview };
});

type PreviewProfile = ReturnType<typeof detectFilePreviewProfile>;

interface FilePreviewBodyProps {
	file: FileInfo | FileListItem;
	activeOption: OpenWithOption | null;
	profile: PreviewProfile | null;
	previewAppsLoaded: boolean;
	downloadPath: string;
	getOptionLabel: (option: OpenWithOption) => string;
	previewLinkFactory?: () => Promise<PreviewLinkInfo>;
	videoStreamLinkFactory?: () => Promise<ShareStreamSessionInfo>;
	createWopiSession?: (() => Promise<WopiLaunchSession>) | null;
	onFileUpdated?: () => void;
	onDirtyChange: (dirty: boolean) => void;
	editable: boolean;
	formattedCategory: "json" | "xml";
	isExpanded: boolean;
}

export function FilePreviewBody({
	file,
	activeOption,
	profile,
	previewAppsLoaded,
	downloadPath,
	getOptionLabel,
	previewLinkFactory,
	videoStreamLinkFactory,
	createWopiSession,
	onFileUpdated,
	onDirtyChange,
	editable,
	formattedCategory,
	isExpanded,
}: FilePreviewBodyProps) {
	const { t } = useTranslation(["files"]);
	const previewLoadingState = (
		<PreviewLoadingState
			text={t("files:loading_preview")}
			className="h-full min-h-[16rem]"
		/>
	);

	if (!previewAppsLoaded) {
		return previewLoadingState;
	}
	if (!profile || !activeOption) {
		return <PreviewUnavailable />;
	}

	if (activeOption.mode === "pdf") {
		return (
			<Suspense fallback={previewLoadingState}>
				<PdfPreview path={downloadPath} fileName={file.name} />
			</Suspense>
		);
	}

	if (activeOption.mode === "image" || activeOption.mode === "audio") {
		return (
			<BlobMediaPreview
				file={file}
				fillContainer={isExpanded}
				mode={activeOption.mode}
				path={downloadPath}
			/>
		);
	}

	if (activeOption.mode === "video") {
		return (
			<VideoPreview
				file={file}
				path={downloadPath}
				videoStreamLinkFactory={videoStreamLinkFactory}
			/>
		);
	}

	if (activeOption.mode === "url_template") {
		return (
			<UrlTemplatePreview
				file={file}
				downloadPath={downloadPath}
				label={getOptionLabel(activeOption)}
				rawConfig={activeOption.config ?? null}
				createPreviewLink={previewLinkFactory}
			/>
		);
	}

	if (activeOption.mode === "wopi") {
		if (!createWopiSession) {
			return <PreviewUnavailable />;
		}
		return (
			<WopiPreview
				label={getOptionLabel(activeOption)}
				rawConfig={activeOption.config ?? null}
				createSession={createWopiSession}
			/>
		);
	}

	if (activeOption.mode === "markdown") {
		return (
			<Suspense fallback={previewLoadingState}>
				<MarkdownPreview path={downloadPath} />
			</Suspense>
		);
	}

	if (activeOption.mode === "table") {
		const delimiter = normalizeTablePreviewDelimiter(
			activeOption.config?.delimiter,
		);

		return (
			<Suspense fallback={previewLoadingState}>
				<CsvTablePreview path={downloadPath} delimiter={delimiter} />
			</Suspense>
		);
	}

	if (activeOption.mode === "formatted") {
		if (formattedCategory === "xml") {
			return (
				<Suspense fallback={previewLoadingState}>
					<XmlPreview path={downloadPath} mode="formatted" />
				</Suspense>
			);
		}

		return (
			<Suspense fallback={previewLoadingState}>
				<JsonPreview path={downloadPath} />
			</Suspense>
		);
	}

	if (activeOption.mode === "code") {
		return (
			<Suspense fallback={previewLoadingState}>
				<TextCodePreview
					file={file}
					modeLabel={getOptionLabel(activeOption)}
					path={downloadPath}
					onFileUpdated={onFileUpdated}
					onDirtyChange={onDirtyChange}
					editable={editable}
				/>
			</Suspense>
		);
	}

	return <PreviewUnavailable />;
}
