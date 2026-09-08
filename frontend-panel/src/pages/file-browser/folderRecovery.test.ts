import { describe, expect, it } from "vitest";
import { ApiError } from "@/services/http";
import { ApiErrorCode } from "@/types/api-helpers";
import {
	isFolderUnavailableError,
	resolveFolderRecoveryTarget,
} from "./folderRecovery";

describe("folderRecovery", () => {
	it("falls back to the parent before an unavailable current folder", () => {
		expect(
			resolveFolderRecoveryTarget(
				[
					{ id: null, name: "Root" },
					{ id: 3, name: "Projects" },
					{ id: 7, name: "Reports" },
				],
				7,
			),
		).toEqual({ id: 3, name: "Projects" });
	});

	it("uses the current folder as the parent of a failed child navigation", () => {
		expect(
			resolveFolderRecoveryTarget(
				[
					{ id: null, name: "Root" },
					{ id: 3, name: "Projects" },
				],
				7,
			),
		).toEqual({ id: 3, name: "Projects" });
	});

	it("falls back to root when no valid breadcrumb candidate remains", () => {
		expect(
			resolveFolderRecoveryTarget([{ id: null, name: "Root" }], 7),
		).toEqual({ id: null, name: "Root" });
	});

	it("creates a root target when the breadcrumb is empty", () => {
		expect(resolveFolderRecoveryTarget([], 7)).toEqual({
			id: null,
			name: "Root",
		});
	});

	it("ignores stale breadcrumb entries after the unavailable folder", () => {
		expect(
			resolveFolderRecoveryTarget(
				[
					{ id: null, name: "Root" },
					{ id: 3, name: "Projects" },
					{ id: 7, name: "Removed" },
					{ id: 9, name: "Stale child" },
				],
				7,
			),
		).toEqual({ id: 3, name: "Projects" });
	});

	it("recognizes only the stable folder.not_found API code", () => {
		expect(
			isFolderUnavailableError(
				new ApiError(ApiErrorCode.FolderNotFound, "Folder is gone"),
			),
		).toBe(true);
		expect(
			isFolderUnavailableError(
				new ApiError(ApiErrorCode.NotFound, "Generic missing resource"),
			),
		).toBe(false);
	});
});
