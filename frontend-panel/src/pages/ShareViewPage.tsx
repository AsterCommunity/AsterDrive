import type { FormEvent, ReactNode } from "react";
import {
	Fragment,
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
import { UserAvatarImage } from "@/components/common/UserAvatarImage";
import { ViewToggle } from "@/components/common/ViewToggle";
import { FileThumbnail } from "@/components/files/FileThumbnail";
import { ReadOnlyFileCollection } from "@/components/files/ReadOnlyFileCollection";
import { ShareTopBar } from "@/components/layout/ShareTopBar";
import {
	Breadcrumb,
	BreadcrumbItem,
	BreadcrumbLink,
	BreadcrumbList,
	BreadcrumbPage,
	BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { handleApiError } from "@/hooks/useApiError";
import { usePageTitle } from "@/hooks/usePageTitle";
import { useRetainedDialogValue } from "@/hooks/useRetainedDialogValue";
import { FOLDER_LIMIT, PAGE_SECTION_PADDING_CLASS } from "@/lib/constants";
import { formatBytes, formatDateShort } from "@/lib/format";
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

interface ShareBreadcrumbItem {
	id: number | null;
	name: string;
}

const SHARE_PAGE_SIZE = 100;
const sharePageParams = {
	folder_limit: FOLDER_LIMIT,
	file_limit: SHARE_PAGE_SIZE,
};
const SHARE_GRID_SKELETON_ITEMS = [
	{ key: "alpha", width: "72%" },
	{ key: "bravo", width: "54%" },
	{ key: "charlie", width: "64%" },
	{ key: "delta", width: "48%" },
	{ key: "echo", width: "68%" },
	{ key: "foxtrot", width: "52%" },
	{ key: "golf", width: "58%" },
	{ key: "hotel", width: "46%" },
	{ key: "india", width: "62%" },
	{ key: "juliet", width: "50%" },
	{ key: "kilo", width: "66%" },
	{ key: "lima", width: "56%" },
];
const SHARE_TABLE_SKELETON_WIDTHS = [
	"68%",
	"52%",
	"60%",
	"48%",
	"64%",
	"56%",
	"50%",
	"62%",
];

function SharePageShell({ children }: { children: ReactNode }) {
	return (
		<div className="flex h-screen flex-col bg-background text-foreground">
			<ShareTopBar />
			{children}
		</div>
	);
}

function ShareOwnerBanner({
	owner,
	text,
}: {
	owner: SharePublicInfo["shared_by"];
	text: string;
}) {
	return (
		<div className="flex max-w-full items-center gap-3 rounded-lg border border-border/70 bg-card/70 px-3 py-3 shadow-xs dark:bg-card/45 dark:shadow-none">
			<UserAvatarImage
				avatar={owner.avatar}
				name={owner.name}
				size="sm"
				className="rounded-lg"
			/>
			<div className="min-w-0">
				<div className="truncate text-sm font-medium text-foreground">
					{text}
				</div>
			</div>
		</div>
	);
}

function getDownloadSummary(
	info: SharePublicInfo,
	t: (key: string, options?: Record<string, unknown>) => string,
) {
	const downloadCount = info.download_count ?? 0;
	const maxDownloads = info.max_downloads ?? 0;
	return maxDownloads > 0
		? t("share:n_of_m_downloads", {
				count: downloadCount,
				max: maxDownloads,
			})
		: t("share:n_downloads", { count: downloadCount });
}

function getExpirySummary(
	info: SharePublicInfo,
	t: (key: string, options?: Record<string, unknown>) => string,
) {
	return info.expires_at
		? t("share:expires_date", { date: formatDateShort(info.expires_at) })
		: t("share:never_expires");
}

function getAccessSummary(
	info: SharePublicInfo,
	t: (key: string, options?: Record<string, unknown>) => string,
) {
	return info.has_password
		? t("share:password_required")
		: t("share:public_access");
}

function ShareMetaLine({
	items,
	className = "",
}: {
	items: Array<string | null | undefined | false>;
	className?: string;
}) {
	const visibleItems = items.filter(Boolean);
	return (
		<div
			className={`flex flex-wrap items-center gap-x-2 gap-y-1 text-sm text-muted-foreground ${className}`}
		>
			{visibleItems.map((item, index) => (
				<Fragment key={String(item)}>
					{index > 0 ? (
						<span className="text-muted-foreground/45">·</span>
					) : null}
					<span className="min-w-0">{item}</span>
				</Fragment>
			))}
		</div>
	);
}

function ShareCenteredPanel({
	icon,
	title,
	description,
	children,
}: {
	icon: "Lock" | "Warning";
	title: string;
	description: string;
	children?: ReactNode;
}) {
	return (
		<SharePageShell>
			<main className="flex min-h-0 flex-1 items-center justify-center overflow-auto p-4 sm:p-6">
				<section className="w-full max-w-md rounded-lg border border-border/70 bg-card/85 p-5 shadow-lg shadow-black/5 dark:bg-card/65 dark:shadow-none">
					<div className="text-center">
						<div className="mx-auto flex h-14 w-14 items-center justify-center rounded-lg bg-muted/45 text-muted-foreground">
							<Icon
								name={icon}
								className={
									icon === "Warning" ? "h-7 w-7 text-destructive" : "h-7 w-7"
								}
							/>
						</div>
						<h1 className="mt-4 text-lg font-semibold leading-snug">{title}</h1>
						<p className="mt-2 text-sm leading-6 text-muted-foreground">
							{description}
						</p>
					</div>
					{children ? <div className="mt-5">{children}</div> : null}
				</section>
			</main>
		</SharePageShell>
	);
}

function ShareLoadingSkeleton() {
	return (
		<SharePageShell>
			<main className="flex min-h-0 flex-1 flex-col overflow-hidden">
				<div
					className={`border-b border-border/65 bg-card/55 ${PAGE_SECTION_PADDING_CLASS}`}
				>
					<div className="mx-auto flex w-full max-w-7xl items-center gap-3 py-3">
						<Skeleton className="h-12 w-12 rounded-lg" />
						<div className="min-w-0 flex-1 space-y-2">
							<Skeleton className="h-5 w-3/5 max-w-lg" />
							<Skeleton className="h-3 w-72 max-w-full" />
						</div>
						<Skeleton className="hidden h-8 w-20 rounded-lg sm:block" />
					</div>
				</div>
				<div className={`min-h-0 flex-1 py-3 ${PAGE_SECTION_PADDING_CLASS}`}>
					<section className="mx-auto flex h-full w-full max-w-7xl flex-col overflow-hidden rounded-lg border border-border/70 bg-card/65 shadow-xs dark:bg-card/35 dark:shadow-none">
						<ShareFolderContentSkeleton viewMode="grid" />
					</section>
				</div>
			</main>
		</SharePageShell>
	);
}

function ShareFolderContentSkeleton({
	viewMode,
}: {
	viewMode: "grid" | "list";
}) {
	if (viewMode === "list") {
		return (
			<div className="divide-y divide-border/60">
				<div className="grid grid-cols-[minmax(0,1fr)_6rem_8rem_2rem] gap-3 px-4 py-3">
					<Skeleton className="h-4 w-24" />
					<Skeleton className="h-4 w-14" />
					<Skeleton className="h-4 w-20" />
					<Skeleton className="h-4 w-4" />
				</div>
				{SHARE_TABLE_SKELETON_WIDTHS.map((width) => (
					<div
						key={`share-row-${width}`}
						className="grid grid-cols-[minmax(0,1fr)_6rem_8rem_2rem] items-center gap-3 px-4 py-3"
					>
						<div className="flex min-w-0 items-center gap-2">
							<Skeleton className="h-8 w-8 rounded-lg" />
							<Skeleton className="h-4" style={{ width }} />
						</div>
						<Skeleton className="h-4 w-14" />
						<Skeleton className="h-4 w-20" />
						<Skeleton className="h-7 w-7 rounded-lg" />
					</div>
				))}
			</div>
		);
	}

	return (
		<div className="grid grid-cols-2 gap-3 p-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
			{SHARE_GRID_SKELETON_ITEMS.map((item) => (
				<div
					key={`share-grid-${item.key}`}
					className="rounded-lg border border-border/65 bg-card/70 p-3 shadow-xs dark:bg-card/40 dark:shadow-none"
				>
					<Skeleton className="mb-3 aspect-[4/3] w-full rounded-lg" />
					<Skeleton className="mb-2 h-4" style={{ width: item.width }} />
					<Skeleton className="h-3 w-1/2" />
				</div>
			))}
		</div>
	);
}

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

	// Breadcrumb renderer for folder share
	const breadcrumbElement =
		breadcrumb.length > 1 ? (
			<Breadcrumb>
				<BreadcrumbList className="gap-2">
					{breadcrumb.map((item, i) => (
						<Fragment key={item.id ?? "root"}>
							{i > 0 && (
								<BreadcrumbSeparator className="mx-0.5 text-muted-foreground/45" />
							)}
							<BreadcrumbItem>
								{i < breadcrumb.length - 1 ? (
									<BreadcrumbLink
										className="cursor-pointer rounded-md px-1.5 py-0.5 text-muted-foreground"
										onClick={() => navigateToFolder(item.id, item.name)}
									>
										{item.name}
									</BreadcrumbLink>
								) : (
									<BreadcrumbPage className="text-base font-semibold text-foreground">
										{item.name}
									</BreadcrumbPage>
								)}
							</BreadcrumbItem>
						</Fragment>
					))}
				</BreadcrumbList>
			</Breadcrumb>
		) : null;

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
						user_id: 0,
						created_at: new Date().toISOString(),
						updated_at: new Date().toISOString(),
						deleted_at: null,
						is_locked: false,
					} satisfies FileInfo)
				: null;
		const downloadSummary = getDownloadSummary(info, t);
		const expirySummary = getExpirySummary(info, t);
		const fileMeta = [
			typeof info.size === "number" ? formatBytes(info.size) : null,
			info.mime_type,
			downloadSummary,
			expirySummary,
			getAccessSummary(info, t),
		].filter(Boolean);

		return (
			<SharePageShell>
				<main className="min-h-0 flex-1 overflow-auto">
					<div className="mx-auto w-full max-w-5xl px-4 py-4 sm:py-6 md:px-6">
						<article className="overflow-hidden rounded-lg border border-border/70 bg-card/80 shadow-lg shadow-black/5 dark:bg-card/55 dark:shadow-none">
							<div className="flex min-w-0 items-center gap-3 border-b border-border/65 px-4 py-3 sm:px-5">
								<UserAvatarImage
									avatar={info.shared_by.avatar}
									name={info.shared_by.name}
									size="sm"
									className="rounded-lg"
								/>
								<div className="min-w-0">
									<div className="truncate text-sm font-medium text-foreground">
										{shareOwnerText}
									</div>
								</div>
							</div>
							<div className="grid gap-5 p-4 sm:p-5 md:grid-cols-[14rem_minmax(0,1fr)]">
								<div className="flex h-52 w-full items-center justify-center overflow-hidden rounded-lg border border-border/65 bg-muted/25 sm:h-64 md:aspect-square md:h-auto">
									{singleShareFile ? (
										<FileThumbnail
											file={singleShareFile}
											size="lg"
											thumbnailPath={shareService.thumbnailPath(token)}
										/>
									) : (
										<Icon
											name="File"
											className="h-16 w-16 text-muted-foreground"
										/>
									)}
								</div>
								<div className="flex min-w-0 flex-col justify-center gap-5">
									<div className="min-w-0">
										<h1 className="break-words text-2xl font-semibold leading-tight sm:text-3xl">
											{info.name}
										</h1>
										<ShareMetaLine items={fileMeta} className="mt-2" />
									</div>
									<div className="flex flex-col gap-2 sm:flex-row">
										{singleShareFile ? (
											<Button
												variant="outline"
												size="lg"
												onClick={() => setPreviewFile(singleShareFile)}
												className="w-full sm:w-auto"
											>
												<Icon name="Eye" className="mr-2 h-5 w-5" />
												{t("files:preview")}
											</Button>
										) : null}
										<Button
											size="lg"
											onClick={handleDownload}
											className="w-full sm:w-auto"
										>
											<Icon name="Download" className="mr-2 h-5 w-5" />
											{t("files:download")}
										</Button>
									</div>
								</div>
							</div>
						</article>
					</div>
				</main>
				{previewElement}
			</SharePageShell>
		);
	}

	return (
		<SharePageShell>
			<main className="flex min-h-0 flex-1 flex-col overflow-hidden">
				<div
					className={`border-b border-border/65 bg-card/55 ${PAGE_SECTION_PADDING_CLASS}`}
				>
					<div className="mx-auto flex w-full max-w-7xl flex-col gap-3 py-3 sm:flex-row sm:items-center sm:justify-between">
						<div className="flex min-w-0 items-center gap-3">
							<div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-lg bg-amber-500/12 text-amber-600 dark:text-amber-300">
								<Icon name="Folder" className="h-6 w-6" />
							</div>
							<div className="min-w-0">
								<h1 className="truncate text-lg font-semibold leading-tight sm:text-xl">
									{info.name}
								</h1>
								<div className="mt-1 flex min-w-0 items-center gap-2">
									<UserAvatarImage
										avatar={info.shared_by.avatar}
										name={info.shared_by.name}
										size="sm"
										className="h-5 w-5 rounded-md text-[10px]"
									/>
									<ShareMetaLine
										items={[
											shareOwnerText,
											getDownloadSummary(info, t),
											getExpirySummary(info, t),
											getAccessSummary(info, t),
										]}
										className="min-w-0 text-xs"
									/>
								</div>
							</div>
						</div>
						<ViewToggle value={viewMode} onChange={setViewMode} />
					</div>
				</div>
				<div className={`min-h-0 flex-1 py-3 ${PAGE_SECTION_PADDING_CLASS}`}>
					<section className="mx-auto flex h-full w-full max-w-7xl flex-col overflow-hidden rounded-lg border border-border/70 bg-card/70 shadow-xs dark:bg-card/40 dark:shadow-none">
						{breadcrumbElement ? (
							<div className="flex flex-col gap-3 border-b border-border/65 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
								<div className="flex min-w-0 items-center gap-2">
									<Icon
										name="FolderOpen"
										className="h-5 w-5 shrink-0 text-amber-500"
									/>
									<div className="min-w-0 flex-1">{breadcrumbElement}</div>
								</div>
							</div>
						) : null}
						<div className="min-h-0 flex-1 overflow-auto">
							{navigating ? (
								<ShareFolderContentSkeleton viewMode={viewMode} />
							) : folderContents ? (
								<>
									<ReadOnlyFileCollection
										folders={folderContents.folders}
										files={folderContents.files}
										viewMode={viewMode}
										onFileClick={setPreviewFile}
										onFileDownload={handleFolderFileDownload}
										onFolderClick={(folder) =>
											navigateToFolder(folder.id, folder.name)
										}
										getThumbnailPath={(file) =>
											`/s/${token}/files/${file.id}/thumbnail`
										}
										emptyTitle={t("empty_folder")}
										emptyDescription={t("files:folder_empty_desc")}
									/>
									{hasMoreFiles && (
										<div ref={sentinelRef} className="flex justify-center py-4">
											{loadingMore && (
												<div className="h-5 w-5 animate-spin rounded-full border-2 border-muted-foreground/30 border-t-muted-foreground" />
											)}
										</div>
									)}
								</>
							) : (
								<div className="p-6 text-sm text-muted-foreground">
									{t("loading_contents")}
								</div>
							)}
						</div>
					</section>
				</div>
			</main>
			{previewElement}
		</SharePageShell>
	);
}
