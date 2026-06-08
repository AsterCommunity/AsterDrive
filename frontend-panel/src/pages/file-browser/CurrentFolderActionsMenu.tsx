import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import {
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuSeparator,
} from "@/components/ui/context-menu";
import {
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
} from "@/components/ui/dropdown-menu";
import { Icon, type IconName } from "@/components/ui/icon";

export interface CurrentFolderActionsMenuProps {
	uploadReady: boolean;
	onCreateFile: () => void;
	onCreateFolder: () => void;
	onManageTagLibrary?: () => void;
	onOfflineDownload: () => void;
	onRefresh: () => void | Promise<void>;
	onTriggerFileUpload: () => void;
	onTriggerFolderUpload: () => void;
}

type CurrentFolderAction =
	| {
			type: "item";
			key: string;
			icon: IconName;
			label: string;
			disabled?: boolean;
			onSelect: () => void;
	  }
	| {
			type: "separator";
			key: string;
	  };

function buildCurrentFolderActions(
	t: TFunction,
	{
		onCreateFile,
		onCreateFolder,
		onManageTagLibrary,
		onOfflineDownload,
		onRefresh,
		onTriggerFileUpload,
		onTriggerFolderUpload,
		uploadReady,
	}: CurrentFolderActionsMenuProps,
): CurrentFolderAction[] {
	return [
		{
			type: "item",
			key: "upload-file",
			icon: "Upload",
			label: t("upload_file"),
			disabled: !uploadReady,
			onSelect: onTriggerFileUpload,
		},
		{
			type: "item",
			key: "upload-folder",
			icon: "FolderOpen",
			label: t("upload_folder"),
			disabled: !uploadReady,
			onSelect: onTriggerFolderUpload,
		},
		{ type: "separator", key: "create-separator" },
		{
			type: "item",
			key: "new-folder",
			icon: "FolderPlus",
			label: t("new_folder"),
			onSelect: onCreateFolder,
		},
		{
			type: "item",
			key: "new-file",
			icon: "FilePlus",
			label: t("new_file"),
			onSelect: onCreateFile,
		},
		{
			type: "item",
			key: "offline-download",
			icon: "LinkSimple",
			label: t("tasks:offline_download_action"),
			onSelect: onOfflineDownload,
		},
		...(onManageTagLibrary
			? [
					{ type: "separator" as const, key: "workspace-tools-separator" },
					{
						type: "item" as const,
						key: "manage-tag-library",
						icon: "Tag" as const,
						label: t("tag_library_manage"),
						onSelect: onManageTagLibrary,
					},
				]
			: []),
		{ type: "separator", key: "refresh-separator" },
		{
			type: "item",
			key: "refresh",
			icon: "ArrowsClockwise",
			label: t("core:refresh"),
			onSelect: () => void onRefresh(),
		},
	];
}

export function CurrentFolderContextMenuContent(
	props: CurrentFolderActionsMenuProps,
) {
	const { t } = useTranslation(["files", "tasks"]);
	const actions = buildCurrentFolderActions(t, props);

	return (
		<ContextMenuContent>
			{actions.map((action) =>
				action.type === "separator" ? (
					<ContextMenuSeparator key={action.key} />
				) : (
					<ContextMenuItem
						key={action.key}
						disabled={action.disabled}
						onClick={action.onSelect}
					>
						<Icon name={action.icon} className="mr-2 size-4" />
						{action.label}
					</ContextMenuItem>
				),
			)}
		</ContextMenuContent>
	);
}

export function CurrentFolderDropdownMenuContent(
	props: CurrentFolderActionsMenuProps,
) {
	const { t } = useTranslation(["files", "tasks"]);
	const actions = buildCurrentFolderActions(t, props);

	return (
		<DropdownMenuContent align="end" className="w-auto min-w-44">
			{actions.map((action) =>
				action.type === "separator" ? (
					<DropdownMenuSeparator key={action.key} />
				) : (
					<DropdownMenuItem
						key={action.key}
						disabled={action.disabled}
						onClick={action.onSelect}
					>
						<Icon name={action.icon} className="size-4 text-muted-foreground" />
						{action.label}
					</DropdownMenuItem>
				),
			)}
		</DropdownMenuContent>
	);
}
