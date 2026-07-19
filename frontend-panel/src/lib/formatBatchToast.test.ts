import { describe, expect, it } from "vitest";
import { formatBatchToast } from "@/lib/formatBatchToast";

function t(key: string, options?: Record<string, unknown>) {
	if (!options) return key;
	return `${key}:${JSON.stringify(options)}`;
}

describe("formatBatchToast", () => {
	it("returns a success toast when every item succeeds", () => {
		expect(
			formatBatchToast(t, "copy", {
				succeeded: 3,
				failed: 0,
				errors: [],
			}),
		).toEqual({
			variant: "success",
			title: 'files:batch_copy_success:{"count":3}',
		});
	});

	it("returns an error toast with truncated details when every item fails", () => {
		const toast = formatBatchToast(t, "delete", {
			succeeded: 0,
			failed: 4,
			errors: [
				{ entity_type: "file", entity_id: 1, error: "e1" },
				{ entity_type: "folder", entity_id: 2, error: "e2" },
				{ entity_type: "other", entity_id: 3, error: "e3" },
				{ entity_type: "file", entity_id: 4, error: "e4" },
			],
		});

		expect(toast.variant).toBe("error");
		expect(toast.title).toBe("files:batch_delete_failed");
		expect(toast.description).toContain(
			'files:batch_error_detail:{"entityType":"core:file"',
		);
		expect(toast.description).toContain(
			'files:batch_error_detail:{"entityType":"core:folder"',
		);
		expect(toast.description).toContain(
			'files:batch_error_detail:{"entityType":"other"',
		);
		expect(toast.description).toContain('files:batch_error_more:{"count":1}');
	});

	it("returns a partial-success toast when both success and failures exist", () => {
		expect(
			formatBatchToast(t, "restore", {
				succeeded: 2,
				failed: 1,
				errors: [{ entity_type: "file", entity_id: 1, error: "denied" }],
			}),
		).toEqual({
			variant: "success",
			title: 'files:batch_restore_partial:{"succeeded":2,"failed":1}',
			description:
				'files:batch_error_detail:{"entityType":"core:file","entityId":1,"error":"denied"}',
		});
	});

	it("treats a partial move as an error so a failed source item is not reported as complete", () => {
		expect(
			formatBatchToast(t, "move", {
				succeeded: 1,
				failed: 1,
				errors: [{ entity_type: "file", entity_id: 1, error: "denied" }],
			}),
		).toEqual({
			variant: "error",
			title: 'files:batch_move_partial:{"succeeded":1,"failed":1}',
			description:
				'files:batch_error_detail:{"entityType":"core:file","entityId":1,"error":"denied"}',
		});
	});
});
