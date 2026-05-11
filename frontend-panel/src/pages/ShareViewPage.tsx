import type { FormEvent } from "react";
import {
	lazy,
	Suspense,
	useCallback,
	useEffect,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { useParams } from "react-router-dom";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { handleApiError } from "@/hooks/useApiError";
import { usePageTitle } from "@/hooks/usePageTitle";
import { useRetainedDialogValue } from "@/hooks/useRetainedDialogValue";
import { FOLDER_LIMIT } from "@/lib/constants";
import { ApiError } from "@/services/http";
import { shareService } from "@/services/shareService";
import { usePreviewAppStore } from "@/stores/previewAppStore";
import type {
	FileInfo,
	FileListItem,
	FolderContents,
	SharePublicInfo,
} from "@/types/api";
import { ErrorCode } from "@/types/api-helpers";
import { ShareFileView } from "./share-view/ShareFileView";
import { ShareLoadingSkeleton } from "./share-view/ShareFolderSkeleton";
import { ShareFolderView } from "./share-view/ShareFolderView";
import {
	ShareCenteredPanel,
	ShareOwnerBanner,
} from "./share-view/ShareViewShell";
import type { ShareBreadcrumbItem } from "./share-view/types";

const SHARE_PAGE_SIZE = 100;
const sharePageParams = {
	folder_limit: FOLDER_LIMIT,
	file_limit: SHARE_PAGE_SIZE,
};

const FilePreview = lazy(async () => {
	const module = await import("@/components/files/FilePreview");
	return { default: module.FilePreview };
});

export default function ShareViewPage() {
	const { t } = useTranslation(["core", "share", "files", "errors"]);
	const { token } = useParams<{ token: string }>();
	const previewAppsLoaded = usePreviewAppStore((state) => state.isLoaded);
	const loadPreviewApps = usePreviewAppStore((state) => state.load);
	const [info, setInfo] = useState<SharePublicInfo | null>(null);
	const [needsPassword, setNeedsPassword] = useState(false);
	const [passwordVerified, setPasswordVerified] = useState(false);
	const [password, setPassword] = useState("");
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);
	const [folderContents, setFolderContents] = useState<FolderContents | null>(
		null,
	);
	const [viewMode, setViewMode] = useState<"grid" | "list">("grid");
	const [previewFile, setPreviewFile] = useState<
		FileInfo | FileListItem | null
	>(null);
	const {
		retainedValue: retainedPreviewFile,
		handleOpenChangeComplete: handlePreviewOpenChangeComplete,
	} = useRetainedDialogValue(previewFile, previewFile !== null);
	const [breadcrumb, setBreadcrumb] = useState<ShareBreadcrumbItem[]>([]);
	const [navigating, setNavigating] = useState(false);
	const [loadingMore, setLoadingMore] = useState(false);
	const sentinelRef = useRef<HTMLDivElement | null>(null);
	usePageTitle(info?.name ?? t("share:share_mode_page"));

	const hasMoreFiles = folderContents?.next_file_cursor != null;

	const loadInfo = useCallback(async () => {
		if (!token) return;
		try {
			setLoading(true);
			const data = await shareService.getInfo(token);
			setInfo(data);
			setNeedsPassword(data.has_password);

			if (data.share_type === "folder" && !data.has_password) {
				const contents = await shareService.listContent(token, sharePageParams);
				setFolderContents(contents);
				setBreadcrumb([{ id: null, name: data.name }]);
			}
		} catch (e) {
			if (e instanceof ApiError) {
				if (e.code === ErrorCode.ShareExpired) {
					setError(t("errors:share_expired"));
				} else if (e.code === ErrorCode.ShareNotFound) {
					setError(t("errors:share_not_found"));
				} else if (e.code === ErrorCode.ShareDownloadLimitReached) {
					setError(t("share:download_limit_reached"));
				} else {
					setError(e.message);
				}
			} else {
				setError(t("share:failed_to_load_share"));
			}
		} finally {
			setLoading(false);
		}
	}, [token, t]);

	useEffect(() => {
		void loadInfo().catch(() => {});
	}, [loadInfo]);

	useEffect(() => {
		if (previewAppsLoaded) return;
		void loadPreviewApps();
	}, [loadPreviewApps, previewAppsLoaded]);

	const navigateToFolder = useCallback(
		async (folderId: number | null, folderName?: string) => {
			if (!token) return;
			setNavigating(true);
			try {
				const contents =
					folderId === null
						? await shareService.listContent(token, sharePageParams)
						: await shareService.listSubfolderContent(
								token,
								folderId,
								sharePageParams,
							);
				setFolderContents(contents);

				setBreadcrumb((prev) => {
					if (folderId === null) {
						return [prev[0]];
					}
					const existingIndex = prev.findIndex((b) => b.id === folderId);
					if (existingIndex >= 0) {
						return prev.slice(0, existingIndex + 1);
					}
					return [...prev, { id: folderId, name: folderName ?? "" }];
				});
			} catch (e) {
				handleApiError(e);
			} finally {
				setNavigating(false);
			}
		},
		[token],
	);

	const loadMoreShareFiles = useCallback(async () => {
		if (
			!token ||
			!folderContents ||
			loadingMore ||
			!folderContents.next_file_cursor
		)
			return;
		setLoadingMore(true);
		try {
			const currentId = breadcrumb[breadcrumb.length - 1]?.id ?? null;
			const cursor = folderContents.next_file_cursor;
			const contents =
				currentId === null
					? await shareService.listContent(token, {
							folder_limit: 0,
							file_limit: SHARE_PAGE_SIZE,
							file_after_value: cursor.value,
							file_after_id: cursor.id,
						})
					: await shareService.listSubfolderContent(token, currentId, {
							folder_limit: 0,
							file_limit: SHARE_PAGE_SIZE,
							file_after_value: cursor.value,
							file_after_id: cursor.id,
						});
			setFolderContents((prev) =>
				prev
					? {
							...prev,
							files: [...prev.files, ...contents.files],
							next_file_cursor: contents.next_file_cursor,
						}
					: prev,
			);
		} catch (e) {
			handleApiError(e);
		} finally {
			setLoadingMore(false);
		}
	}, [token, folderContents, loadingMore, breadcrumb]);

	useEffect(() => {
		if (!hasMoreFiles || loadingMore) return;
		const el = sentinelRef.current;
		if (!el) return;
		const observer = new IntersectionObserver(
			(entries) => {
				if (entries[0].isIntersecting)
					void loadMoreShareFiles().catch(() => {});
			},
			{ rootMargin: "200px" },
		);
		observer.observe(el);
		return () => observer.disconnect();
	}, [hasMoreFiles, loadingMore, loadMoreShareFiles]);

	const handleVerifyPassword = async (e: FormEvent) => {
		e.preventDefault();
		if (!token) return;
		try {
			await shareService.verifyPassword(token, password);
			setPasswordVerified(true);
			setNeedsPassword(false);
			toast.success(t("share:password_verified"));

			if (info?.share_type === "folder") {
				const contents = await shareService.listContent(token, sharePageParams);
				setFolderContents(contents);
				setBreadcrumb([{ id: null, name: info.name }]);
			}
		} catch (e) {
			handleApiError(e);
		}
	};

	const handleDownload = () => {
		if (!token) return;
		const url = shareService.downloadUrl(token);
		window.open(url, "_blank");
	};

	const handleFolderFileDownload = (file: FileListItem) => {
		if (!token) return;
		const url = shareService.downloadFolderFileUrl(token, file.id);
		window.open(url, "_blank");
	};

	const createVideoStreamLink = useCallback(() => {
		if (!token || !retainedPreviewFile || !info) {
			return Promise.reject(new Error("share video stream is unavailable"));
		}
		return info.share_type === "file"
			? shareService.createStreamSession(token)
			: shareService.createFolderFileStreamSession(
					token,
					retainedPreviewFile.id,
				);
	}, [info, retainedPreviewFile, token]);

	const previewElement = token ? (
		<Suspense fallback={null}>
			{retainedPreviewFile ? (
				<FilePreview
					file={retainedPreviewFile}
					open={previewFile !== null}
					onClose={() => setPreviewFile(null)}
					onOpenChangeComplete={handlePreviewOpenChangeComplete}
					downloadPath={
						info?.share_type === "file"
							? shareService.downloadPath(token)
							: shareService.downloadFolderPath(token, retainedPreviewFile.id)
					}
					editable={false}
					previewLinkFactory={() =>
						info?.share_type === "file"
							? shareService.createPreviewLink(token)
							: shareService.createFolderFilePreviewLink(
									token,
									retainedPreviewFile.id,
								)
					}
					videoStreamLinkFactory={createVideoStreamLink}
				/>
			) : null}
		</Suspense>
	) : null;

	if (loading) {
		return <ShareLoadingSkeleton />;
	}

	if (error) {
		return (
			<ShareCenteredPanel
				icon="Warning"
				title={t("unavailable")}
				description={error}
			/>
		);
	}

	if (!info) return null;
	if (!token) return null;

	const shareOwnerText = t("share:shared_by", {
		name: info.shared_by.name,
	});

	if (needsPassword && !passwordVerified) {
		return (
			<ShareCenteredPanel
				icon="Lock"
				title={info.name}
				description={t("share:password_protected")}
			>
				<div className="space-y-4">
					<ShareOwnerBanner owner={info.shared_by} text={shareOwnerText} />
					<form onSubmit={handleVerifyPassword} className="space-y-3">
						<Input
							type="password"
							placeholder={t("core:password")}
							value={password}
							onChange={(e) => setPassword(e.target.value)}
							autoFocus
						/>
						<Button type="submit" className="w-full">
							{t("verify")}
						</Button>
					</form>
				</div>
			</ShareCenteredPanel>
		);
	}

	if (info.share_type === "file") {
		const singleShareFile =
			info.mime_type && typeof info.size === "number"
				? ({
						id: -1,
						name: info.name,
						mime_type: info.mime_type,
						size: info.size,
						folder_id: null,
						blob_id: 0,
						owner_user_id: null,
						created_by_user_id: null,
						created_by_username: info.shared_by.name,
						team_id: null,
						created_at: new Date().toISOString(),
						updated_at: new Date().toISOString(),
						deleted_at: null,
						is_locked: false,
					} satisfies FileInfo)
				: null;

		return (
			<ShareFileView
				info={info}
				previewElement={previewElement}
				shareOwnerText={shareOwnerText}
				singleShareFile={singleShareFile}
				token={token}
				onDownload={handleDownload}
				onPreviewFile={setPreviewFile}
			/>
		);
	}

	return (
		<ShareFolderView
			breadcrumb={breadcrumb}
			folderContents={folderContents}
			hasMoreFiles={hasMoreFiles}
			info={info}
			loadingMore={loadingMore}
			navigating={navigating}
			previewElement={previewElement}
			sentinelRef={sentinelRef}
			shareOwnerText={shareOwnerText}
			token={token}
			viewMode={viewMode}
			onFileDownload={handleFolderFileDownload}
			onFilePreview={setPreviewFile}
			onNavigateToFolder={navigateToFolder}
			onViewModeChange={setViewMode}
		/>
	);
}
