import type { FileBrowserSelectionDownloadAction } from "@/components/files/FileBrowserContext";
import type { IconName } from "@/components/ui/icon";

export type BuiltinFileActionId =
	| "open"
	| "choose_open_method"
	| "download"
	| "archive_extract"
	| "archive_compress"
	| "archive_download"
	| "share_page"
	| "share_direct"
	| "copy"
	| "move"
	| "folder_policy"
	| "go_to_location"
	| "rename"
	| "manage_tags"
	| "versions"
	| "info"
	| "toggle_lock"
	| "delete";

export type PluginFileActionId = `plugin:${string}`;

export type FileActionId = BuiltinFileActionId | PluginFileActionId;

export type FileActionPresentationGroup =
	| "open"
	| "transfer"
	| "organize"
	| "metadata"
	| "danger"
	| "plugin";

export type FileActionScope = "file" | "folder" | "mixed";

export interface FileActionDescriptor {
	id: FileActionId;
	icon: IconName | ((context: FileActionAvailabilityContext) => IconName);
	labelKey: string | ((context: FileActionAvailabilityContext) => string);
	presentation: {
		group: FileActionPresentationGroup;
		order: number;
	};
	scope: FileActionScope;
	availability: (
		context: FileActionAvailabilityContext,
	) => FileActionAvailability;
}

export interface FileActionAvailability {
	disabled?: boolean;
	onClick?: () => void;
}

export interface FileActionAvailabilityContext {
	downloadAction?: FileBrowserSelectionDownloadAction;
	isFolder: boolean;
	isLocked: boolean;
	selectionCount?: number;
	handlers: Partial<Record<FileActionId, () => void>>;
}

export interface ResolvedFileAction {
	descriptor: FileActionDescriptor;
	disabled: boolean;
	icon: IconName;
	id: FileActionId;
	labelKey: string;
	onClick: () => void;
	presentation: FileActionDescriptor["presentation"];
}

function handlerAvailability(id: BuiltinFileActionId) {
	return (context: FileActionAvailabilityContext): FileActionAvailability => ({
		onClick: context.handlers[id],
	});
}

const singleFileActions: FileActionDescriptor[] = [
	{
		id: "open",
		icon: "Eye",
		labelKey: "open",
		presentation: { group: "open", order: 10 },
		scope: "mixed",
		availability: handlerAvailability("open"),
	},
	{
		id: "choose_open_method",
		icon: "ListBullets",
		labelKey: "open_with_action",
		presentation: { group: "open", order: 20 },
		scope: "file",
		availability: handlerAvailability("choose_open_method"),
	},
	{
		id: "download",
		icon: "Download",
		labelKey: "download",
		presentation: { group: "transfer", order: 10 },
		scope: "file",
		availability: handlerAvailability("download"),
	},
	{
		id: "archive_extract",
		icon: "FolderOpen",
		labelKey: "tasks:archive_extract_action",
		presentation: { group: "transfer", order: 20 },
		scope: "file",
		availability: handlerAvailability("archive_extract"),
	},
	{
		id: "archive_compress",
		icon: "FileZip",
		labelKey: "tasks:archive_compress_action",
		presentation: { group: "transfer", order: 30 },
		scope: "mixed",
		availability: handlerAvailability("archive_compress"),
	},
	{
		id: "archive_download",
		icon: "Download",
		labelKey: "tasks:archive_download_action",
		presentation: { group: "transfer", order: 40 },
		scope: "folder",
		availability: handlerAvailability("archive_download"),
	},
	{
		id: "share_page",
		icon: "Link",
		labelKey: "share",
		presentation: { group: "transfer", order: 50 },
		scope: "mixed",
		availability: handlerAvailability("share_page"),
	},
	{
		id: "share_direct",
		icon: "LinkSimple",
		labelKey: "share:share_direct_link_action",
		presentation: { group: "transfer", order: 60 },
		scope: "file",
		availability: handlerAvailability("share_direct"),
	},
	{
		id: "copy",
		icon: "Copy",
		labelKey: "copy_to",
		presentation: { group: "organize", order: 10 },
		scope: "mixed",
		availability: handlerAvailability("copy"),
	},
	{
		id: "move",
		icon: "ArrowsOutCardinal",
		labelKey: "move_to",
		presentation: { group: "organize", order: 20 },
		scope: "mixed",
		availability: handlerAvailability("move"),
	},
	{
		id: "folder_policy",
		icon: "HardDrive",
		labelKey: "folder_policy",
		presentation: { group: "organize", order: 30 },
		scope: "folder",
		availability: handlerAvailability("folder_policy"),
	},
	{
		id: "go_to_location",
		icon: "FolderOpen",
		labelKey: "go_to_file_location",
		presentation: { group: "organize", order: 40 },
		scope: "file",
		availability: handlerAvailability("go_to_location"),
	},
	{
		id: "rename",
		icon: "PencilSimple",
		labelKey: "rename",
		presentation: { group: "organize", order: 50 },
		scope: "mixed",
		availability: handlerAvailability("rename"),
	},
	{
		id: "manage_tags",
		icon: "Tag",
		labelKey: "tag_manage",
		presentation: { group: "organize", order: 60 },
		scope: "mixed",
		availability: handlerAvailability("manage_tags"),
	},
	{
		id: "versions",
		icon: "Clock",
		labelKey: "versions",
		presentation: { group: "organize", order: 70 },
		scope: "file",
		availability: handlerAvailability("versions"),
	},
	{
		id: "info",
		icon: "Info",
		labelKey: "info",
		presentation: { group: "metadata", order: 10 },
		scope: "mixed",
		availability: handlerAvailability("info"),
	},
	{
		id: "toggle_lock",
		icon: (context) => (context.isLocked ? "LockOpen" : "Lock"),
		labelKey: (context) => (context.isLocked ? "unlock" : "lock"),
		presentation: { group: "metadata", order: 20 },
		scope: "mixed",
		availability: handlerAvailability("toggle_lock"),
	},
	{
		id: "delete",
		icon: "Trash",
		labelKey: "core:delete",
		presentation: { group: "danger", order: 10 },
		scope: "mixed",
		availability: (context) => ({
			disabled: context.isLocked,
			onClick: context.handlers.delete,
		}),
	},
];

const selectionActions: FileActionDescriptor[] = [
	{
		id: "download",
		icon: "Download",
		labelKey: (context) =>
			context.downloadAction?.kind === "file"
				? "download"
				: "tasks:archive_download_action",
		presentation: { group: "transfer", order: 10 },
		scope: "mixed",
		availability: (context) => ({
			onClick: context.downloadAction?.onClick,
		}),
	},
	{
		id: "archive_compress",
		icon: "FileZip",
		labelKey: "tasks:archive_compress_action",
		presentation: { group: "transfer", order: 20 },
		scope: "mixed",
		availability: handlerAvailability("archive_compress"),
	},
	{
		id: "copy",
		icon: "Copy",
		labelKey: "copy_to",
		presentation: { group: "organize", order: 10 },
		scope: "mixed",
		availability: handlerAvailability("copy"),
	},
	{
		id: "move",
		icon: "ArrowsOutCardinal",
		labelKey: "move_to",
		presentation: { group: "organize", order: 20 },
		scope: "mixed",
		availability: handlerAvailability("move"),
	},
	{
		id: "manage_tags",
		icon: "Tag",
		labelKey: "tag_manage",
		presentation: { group: "organize", order: 30 },
		scope: "mixed",
		availability: handlerAvailability("manage_tags"),
	},
	{
		id: "delete",
		icon: "Trash",
		labelKey: "core:delete",
		presentation: { group: "danger", order: 10 },
		scope: "mixed",
		availability: handlerAvailability("delete"),
	},
];

export const BUILTIN_FILE_ACTION_DESCRIPTORS = singleFileActions;
export const BUILTIN_FILE_SELECTION_ACTION_DESCRIPTORS = selectionActions;

const FILE_ACTION_GROUP_ORDER: Record<FileActionPresentationGroup, number> = {
	open: 10,
	transfer: 20,
	organize: 30,
	metadata: 40,
	danger: 50,
	plugin: 60,
};

function resolveDescriptorValue<T>(
	value: T | ((context: FileActionAvailabilityContext) => T),
	context: FileActionAvailabilityContext,
) {
	return typeof value === "function"
		? (value as (context: FileActionAvailabilityContext) => T)(context)
		: value;
}

function actionScopeMatchesContext(
	scope: FileActionScope,
	context: FileActionAvailabilityContext,
) {
	return (
		scope === "mixed" ||
		(scope === "folder" && context.isFolder) ||
		(scope === "file" && !context.isFolder)
	);
}

export function resolveFileActions(
	descriptors: readonly FileActionDescriptor[],
	context: FileActionAvailabilityContext,
): ResolvedFileAction[] {
	return descriptors
		.flatMap((descriptor) => {
			if (!actionScopeMatchesContext(descriptor.scope, context)) {
				return [];
			}

			const availability = descriptor.availability(context);
			if (!availability.onClick) {
				return [];
			}

			return [
				{
					descriptor,
					disabled: availability.disabled ?? false,
					icon: resolveDescriptorValue(descriptor.icon, context),
					id: descriptor.id,
					labelKey: resolveDescriptorValue(descriptor.labelKey, context),
					onClick: availability.onClick,
					presentation: descriptor.presentation,
				},
			];
		})
		.toSorted(
			(left, right) =>
				FILE_ACTION_GROUP_ORDER[left.presentation.group] -
					FILE_ACTION_GROUP_ORDER[right.presentation.group] ||
				left.presentation.order - right.presentation.order ||
				left.id.localeCompare(right.id),
		);
}
