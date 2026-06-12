import { lazyWithPreload } from "@/lib/lazyWithPreload";

export const ArchiveTaskNameDialog = lazyWithPreload(async () => {
	const module = await import("@/components/files/ArchiveTaskNameDialog");
	return { default: module.ArchiveTaskNameDialog };
});

export const BatchTargetFolderDialog = lazyWithPreload(async () => {
	const module = await import("@/components/files/BatchTargetFolderDialog");
	return { default: module.BatchTargetFolderDialog };
});

export const CreateFileDialog = lazyWithPreload(async () => {
	const module = await import("@/components/files/CreateFileDialog");
	return { default: module.CreateFileDialog };
});

export const CreateFolderDialog = lazyWithPreload(async () => {
	const module = await import("@/components/files/CreateFolderDialog");
	return { default: module.CreateFolderDialog };
});

export const FileInfoDialog = lazyWithPreload(async () => {
	const module = await import("@/components/files/FileInfoDialog");
	return { default: module.FileInfoDialog };
});

export const FolderPolicyDialog = lazyWithPreload(async () => {
	const module = await import("@/components/files/FolderPolicyDialog");
	return { default: module.FolderPolicyDialog };
});

export const OfflineDownloadDialog = lazyWithPreload(async () => {
	const module = await import("@/components/files/OfflineDownloadDialog");
	return { default: module.OfflineDownloadDialog };
});

export const RenameDialog = lazyWithPreload(async () => {
	const module = await import("@/components/files/RenameDialog");
	return { default: module.RenameDialog };
});

export const ShareDialog = lazyWithPreload(async () => {
	const module = await import("@/components/files/ShareDialog");
	return { default: module.ShareDialog };
});

export const VersionHistoryDialog = lazyWithPreload(async () => {
	const module = await import("@/components/files/VersionHistoryDialog");
	return { default: module.VersionHistoryDialog };
});

export const FILE_BROWSER_LAZY_PRELOADERS = [
	ArchiveTaskNameDialog,
	BatchTargetFolderDialog,
	CreateFileDialog,
	CreateFolderDialog,
	FileInfoDialog,
	FolderPolicyDialog,
	OfflineDownloadDialog,
	RenameDialog,
	ShareDialog,
	VersionHistoryDialog,
] as const;
