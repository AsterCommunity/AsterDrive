import { lazy, Suspense } from "react";
import { useTranslation } from "react-i18next";
import type { ResourcePath } from "@/lib/resourceRequest";
import { normalizeTablePreviewDelimiter } from "@/lib/tablePreview";
import type {
	ArchiveFilenameEncoding,
	ArchivePreviewManifest,
	FileInfo,
	FileListItem,
} from "@/types/api";
import { BlobImagePreview } from "./BlobImagePreview";
import type { detectFilePreviewProfile } from "./file-capabilities";
import type { FilePreviewResources } from "./filePreviewResources";
import { PreviewLoadingState } from "./PreviewLoadingState";
import { PreviewUnavailable } from "./PreviewUnavailable";
import type { OpenWithOption } from "./types";
import { UrlTemplatePreview } from "./UrlTemplatePreview";
import { VideoPreview } from "./VideoPreview";
import { WopiPreview } from "./WopiPreview";
import type { WopiSessionResource } from "./wopiSessionResource";

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

const ArchivePreview = lazy(async () => {
	const module = await import("./ArchivePreview");
	return { default: module.ArchivePreview };
});

type PreviewProfile = ReturnType<typeof detectFilePreviewProfile>;

interface FilePreviewBodyProps {
	file: FileInfo | FileListItem;
	activeOption: OpenWithOption | null;
	profile: PreviewProfile | null;
	previewAppsLoaded: boolean;
	contentResource: ResourcePath | null;
	resources: FilePreviewResources;
	getOptionLabel: (option: OpenWithOption) => string;
	archiveManifestLoader?: (options?: {
		signal?: AbortSignal;
		filenameEncoding?: ArchiveFilenameEncoding;
	}) => Promise<ArchivePreviewManifest>;
	wopiSessionResource?: WopiSessionResource | null;
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
	contentResource,
	resources,
	getOptionLabel,
	archiveManifestLoader,
	wopiSessionResource,
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
		if (!contentResource) return previewLoadingState;
		return (
			<Suspense fallback={previewLoadingState}>
				<PdfPreview resource={contentResource} fileName={file.name} />
			</Suspense>
		);
	}

	if (activeOption.mode === "image") {
		return (
			<BlobImagePreview
				file={file}
				fillContainer={isExpanded}
				resource={contentResource}
			/>
		);
	}

	if (activeOption.mode === "video") {
		return (
			<VideoPreview
				file={file}
				resource={contentResource}
				createMediaStreamSession={resources.actions?.createMediaStreamSession}
			/>
		);
	}

	if (activeOption.mode === "url_template") {
		return (
			<UrlTemplatePreview
				file={file}
				downloadPath={resources.paths.download}
				label={getOptionLabel(activeOption)}
				optionKey={activeOption.key}
				rawConfig={activeOption.config ?? null}
				createExternalPreviewLink={resources.actions?.createExternalPreviewLink}
			/>
		);
	}

	if (activeOption.mode === "wopi") {
		if (!wopiSessionResource) {
			return <PreviewUnavailable />;
		}
		return (
			<WopiPreview
				label={getOptionLabel(activeOption)}
				rawConfig={activeOption.config ?? null}
				sessionResource={wopiSessionResource}
			/>
		);
	}

	if (activeOption.mode === "markdown") {
		if (!contentResource) return previewLoadingState;
		return (
			<Suspense fallback={previewLoadingState}>
				<MarkdownPreview resource={contentResource} />
			</Suspense>
		);
	}

	if (activeOption.mode === "table") {
		if (!contentResource) return previewLoadingState;
		const delimiter = normalizeTablePreviewDelimiter(
			activeOption.config?.delimiter,
		);

		return (
			<Suspense fallback={previewLoadingState}>
				<CsvTablePreview resource={contentResource} delimiter={delimiter} />
			</Suspense>
		);
	}

	if (activeOption.mode === "formatted") {
		if (!contentResource) return previewLoadingState;
		if (formattedCategory === "xml") {
			return (
				<Suspense fallback={previewLoadingState}>
					<XmlPreview resource={contentResource} mode="formatted" />
				</Suspense>
			);
		}

		return (
			<Suspense fallback={previewLoadingState}>
				<JsonPreview resource={contentResource} />
			</Suspense>
		);
	}

	if (activeOption.mode === "code") {
		if (!contentResource) return previewLoadingState;
		return (
			<Suspense fallback={previewLoadingState}>
				<TextCodePreview
					file={file}
					modeLabel={getOptionLabel(activeOption)}
					resource={contentResource}
					onFileUpdated={onFileUpdated}
					onDirtyChange={onDirtyChange}
					editable={editable}
				/>
			</Suspense>
		);
	}

	if (activeOption.mode === "archive") {
		return (
			<Suspense fallback={previewLoadingState}>
				<ArchivePreview loadManifest={archiveManifestLoader} />
			</Suspense>
		);
	}

	return <PreviewUnavailable />;
}
