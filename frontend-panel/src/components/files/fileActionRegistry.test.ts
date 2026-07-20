import { describe, expect, it, vi } from "vitest";
import {
	BUILTIN_FILE_ACTION_DESCRIPTORS,
	BUILTIN_FILE_SELECTION_ACTION_DESCRIPTORS,
	type BuiltinFileActionId,
	type FileActionAvailabilityContext,
	type FileActionDescriptor,
	resolveFileActions,
} from "@/components/files/fileActionRegistry";

function context(
	overrides: Partial<FileActionAvailabilityContext> = {},
): FileActionAvailabilityContext {
	const handlers = Object.fromEntries(
		[
			"archive_compress",
			"archive_download",
			"archive_extract",
			"choose_open_method",
			"copy",
			"delete",
			"download",
			"folder_policy",
			"go_to_location",
			"info",
			"manage_tags",
			"move",
			"open",
			"rename",
			"share_direct",
			"share_page",
			"toggle_lock",
			"versions",
		].map((id) => [id, vi.fn()]),
	) as Record<BuiltinFileActionId, () => void>;

	return {
		handlers,
		isFolder: false,
		isLocked: false,
		...overrides,
	};
}

function actionIds(context: FileActionAvailabilityContext) {
	return resolveFileActions(BUILTIN_FILE_ACTION_DESCRIPTORS, context).map(
		(action) => action.id,
	);
}

describe("fileActionRegistry", () => {
	it("resolves file action order and file-only actions", () => {
		expect(actionIds(context())).toEqual([
			"open",
			"choose_open_method",
			"download",
			"archive_extract",
			"archive_compress",
			"share_page",
			"share_direct",
			"copy",
			"move",
			"go_to_location",
			"rename",
			"manage_tags",
			"versions",
			"info",
			"toggle_lock",
			"delete",
		]);
	});

	it("resolves folder actions without file-only entries", () => {
		expect(actionIds(context({ isFolder: true }))).toEqual([
			"open",
			"archive_compress",
			"archive_download",
			"share_page",
			"copy",
			"move",
			"folder_policy",
			"rename",
			"manage_tags",
			"info",
			"toggle_lock",
			"delete",
		]);
	});

	it("marks locked delete disabled and switches lock presentation", () => {
		const actions = resolveFileActions(
			BUILTIN_FILE_ACTION_DESCRIPTORS,
			context({ isLocked: true }),
		);

		expect(actions.find((action) => action.id === "delete")).toMatchObject({
			disabled: true,
			labelKey: "core:delete",
		});
		expect(actions.find((action) => action.id === "toggle_lock")).toMatchObject(
			{
				icon: "LockOpen",
				labelKey: "unlock",
			},
		);
	});

	it("omits actions without handlers", () => {
		expect(
			resolveFileActions(
				BUILTIN_FILE_ACTION_DESCRIPTORS,
				context({
					handlers: {
						open: vi.fn(),
						download: vi.fn(),
					},
				}),
			).map((action) => action.id),
		).toEqual(["open", "download"]);
	});

	it("resolves selection actions from selection download metadata", () => {
		const onDownload = vi.fn();
		const actions = resolveFileActions(
			BUILTIN_FILE_SELECTION_ACTION_DESCRIPTORS,
			context({
				downloadAction: {
					kind: "archive",
					onClick: onDownload,
				},
				selectionCount: 3,
			}),
		);

		expect(actions.map((action) => action.id)).toEqual([
			"download",
			"archive_compress",
			"copy",
			"move",
			"manage_tags",
			"delete",
		]);
		expect(actions[0]).toMatchObject({
			icon: "Download",
			labelKey: "tasks:archive_download_action",
			onClick: onDownload,
		});
	});

	it("labels multi-selection downloads as downloads instead of forcing ZIP semantics", () => {
		const onDownload = vi.fn();
		const actions = resolveFileActions(
			BUILTIN_FILE_SELECTION_ACTION_DESCRIPTORS,
			context({
				downloadAction: {
					kind: "selection",
					onClick: onDownload,
				},
				selectionCount: 3,
			}),
		);

		expect(actions[0]).toMatchObject({
			icon: "Download",
			labelKey: "download",
			onClick: onDownload,
		});
	});

	it("supports plugin descriptors without enabling unavailable actions", () => {
		const runPluginAction = vi.fn();
		const descriptors: FileActionDescriptor[] = [
			...BUILTIN_FILE_ACTION_DESCRIPTORS,
			{
				id: "plugin:send_to_webhook",
				icon: "ArrowSquareOut",
				labelKey: "plugin_send_to_webhook",
				presentation: {
					group: "plugin",
					order: 10,
				},
				scope: "mixed",
				availability: () => ({
					onClick: runPluginAction,
				}),
			},
			{
				id: "plugin:hidden",
				icon: "Question",
				labelKey: "plugin_hidden",
				presentation: {
					group: "plugin",
					order: 20,
				},
				scope: "mixed",
				availability: () => ({}),
			},
		];

		const actions = resolveFileActions(descriptors, context());

		expect(actions.at(-1)).toMatchObject({
			icon: "ArrowSquareOut",
			id: "plugin:send_to_webhook",
			labelKey: "plugin_send_to_webhook",
		});
		expect(actions.map((action) => action.id)).not.toContain("plugin:hidden");
		actions.at(-1)?.onClick();
		expect(runPluginAction).toHaveBeenCalledTimes(1);
	});
});
