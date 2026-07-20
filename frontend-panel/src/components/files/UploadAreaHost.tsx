import type { Dispatch, SetStateAction } from "react";
import { useCallback, useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import { UploadPanel } from "@/components/files/UploadPanel";
import { useUploadAreaManager } from "@/components/files/useUploadAreaManager";
import { BottomRightActivityPortal } from "@/components/layout/BottomRightActivityShell";
import type { Workspace } from "@/lib/workspace";
import { useAuthStore } from "@/stores/authStore";
import { useFileStore } from "@/stores/fileStore";
import {
	TRANSFER_ACTIVITY,
	useTransferActivityStore,
} from "@/stores/transferActivityStore";
import {
	type UploadAreaControls,
	useUploadAreaControlsStore,
} from "@/stores/uploadAreaControlsStore";

interface UploadAreaHostProps {
	workspace: Workspace;
}

export function UploadAreaHost({ workspace }: UploadAreaHostProps) {
	const { t } = useTranslation(["core", "files"]);
	const refresh = useFileStore((state) => state.refresh);
	const currentFolderId = useFileStore((state) => state.currentFolderId);
	const breadcrumb = useFileStore((state) => state.breadcrumb);
	const refreshUser = useAuthStore((state) => state.refreshUser);
	const storageEventStreamEnabled = useAuthStore(
		(state) => state.user?.preferences?.storage_event_stream_enabled !== false,
	);
	const setControls = useUploadAreaControlsStore((state) => state.setControls);
	const setUploadPanelPresence = useUploadAreaControlsStore(
		(state) => state.setUploadPanelPresence,
	);
	const fileInputRef = useRef<HTMLInputElement | null>(null);
	const folderInputRef = useRef<HTMLInputElement | null>(null);
	const resumeFileInputRef = useRef<HTMLInputElement | null>(null);
	const uploadPanelOpen = useTransferActivityStore(
		(state) => state.expandedActivity === TRANSFER_ACTIVITY.upload,
	);
	const setActivityOpen = useTransferActivityStore(
		(state) => state.setActivityOpen,
	);
	const setUploadPanelOpen = useCallback<Dispatch<SetStateAction<boolean>>>(
		(open) => setActivityOpen(TRANSFER_ACTIVITY.upload, open),
		[setActivityOpen],
	);
	const {
		activeCount,
		clearCompletedTasks,
		failedCount,
		hasUploadActivity,
		handleDragEnter,
		handleDragLeave,
		handleDragOver,
		handleDrop,
		handleFileInputChange,
		handleFolderInputChange,
		handleResumeFileChange,
		isDragging,
		overallProgress,
		retryFailedTasks,
		setUploadAutoClearCompleted,
		setUploadConcurrency,
		successCount,
		totalCount,
		uploadAutoClearCompleted,
		uploadConcurrency,
		uploadTasks,
	} = useUploadAreaManager({
		breadcrumb,
		currentFolderId,
		refresh,
		refreshUser,
		resumeFileInputRef,
		setUploadPanelOpen,
		storageEventStreamEnabled,
		workspace,
	});
	const showUploadPanel = hasUploadActivity || uploadTasks.length > 0;

	const triggerFileUpload = useCallback(() => {
		fileInputRef.current?.click();
	}, []);
	const triggerFolderUpload = useCallback(() => {
		folderInputRef.current?.click();
	}, []);

	const controls = useMemo<UploadAreaControls>(
		() => ({
			handleDragEnter,
			handleDragLeave,
			handleDragOver,
			handleDrop,
			isDragging,
			triggerFileUpload,
			triggerFolderUpload,
		}),
		[
			handleDragEnter,
			handleDragLeave,
			handleDragOver,
			handleDrop,
			isDragging,
			triggerFileUpload,
			triggerFolderUpload,
		],
	);

	useEffect(() => {
		setControls(controls);
		return () => {
			if (useUploadAreaControlsStore.getState().controls === controls) {
				setControls(null);
			}
		};
	}, [controls, setControls]);

	useEffect(() => {
		setUploadPanelPresence({
			open: showUploadPanel && uploadPanelOpen,
			visible: showUploadPanel,
		});
		return () => {
			setUploadPanelPresence({
				open: false,
				visible: false,
			});
		};
	}, [setUploadPanelPresence, showUploadPanel, uploadPanelOpen]);

	const uploadSummary =
		totalCount === 0
			? t("files:upload_summary_empty")
			: successCount === totalCount
				? t("files:upload_summary_done", { total: totalCount })
				: t("files:upload_summary", {
						total: totalCount,
						success: successCount,
						failed: failedCount,
						active: activeCount,
					});

	return (
		<>
			<input
				ref={fileInputRef}
				type="file"
				aria-label={t("files:upload_file")}
				data-testid="upload-file-input"
				multiple
				className="hidden"
				onChange={handleFileInputChange}
			/>
			<input
				ref={folderInputRef}
				type="file"
				aria-label={t("files:upload_folder")}
				data-testid="upload-folder-input"
				multiple
				className="hidden"
				// @ts-expect-error webkitdirectory is browser-specific
				webkitdirectory=""
				onChange={handleFolderInputChange}
			/>
			<input
				ref={resumeFileInputRef}
				type="file"
				aria-label={t("files:upload_resume_select")}
				data-testid="resume-input"
				className="hidden"
				onChange={handleResumeFileChange}
			/>
			{showUploadPanel && (
				<BottomRightActivityPortal>
					<UploadPanel
						embedded
						open={uploadPanelOpen}
						onToggle={() => setUploadPanelOpen((prev) => !prev)}
						title={t("files:upload")}
						summary={uploadSummary}
						tasks={uploadTasks}
						emptyText={t("files:upload_empty")}
						totalCount={totalCount}
						successCount={successCount}
						failedCount={failedCount}
						activeCount={activeCount}
						overallProgress={overallProgress}
						concurrency={uploadConcurrency}
						autoClearCompleted={uploadAutoClearCompleted}
						onConcurrencyChange={setUploadConcurrency}
						onAutoClearCompletedChange={setUploadAutoClearCompleted}
						onRetryFailed={retryFailedTasks}
						retryFailedLabel={t("files:upload_retry")}
						onClearCompleted={clearCompletedTasks}
						clearCompletedLabel={t("files:upload_clear_completed")}
					/>
				</BottomRightActivityPortal>
			)}
		</>
	);
}
