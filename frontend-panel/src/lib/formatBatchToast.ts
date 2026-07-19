import type { TFunction } from "i18next";
import type { BatchItemError, BatchResult } from "@/types/api";

export type BatchOperation = "move" | "copy" | "delete" | "restore" | "purge";

export interface BatchToastContent {
	variant: "success" | "error";
	title: string;
	description?: string;
}

function getEntityLabel(t: TFunction, entityType: string): string {
	switch (entityType) {
		case "file":
			return t("core:file");
		case "folder":
			return t("core:folder");
		default:
			return entityType;
	}
}

function formatBatchErrorDescription(
	t: TFunction,
	errors: BatchItemError[],
): string | undefined {
	if (errors.length === 0) return undefined;

	const visibleErrors = errors.slice(0, 3).map((error) =>
		t("files:batch_error_detail", {
			entityType: getEntityLabel(t, error.entity_type),
			entityId: error.entity_id,
			error: error.error,
		}),
	);
	const remainingCount = errors.length - visibleErrors.length;

	if (remainingCount > 0) {
		visibleErrors.push(t("files:batch_error_more", { count: remainingCount }));
	}

	return visibleErrors.join("\n");
}

export function formatBatchToast(
	t: TFunction,
	operation: BatchOperation,
	result: BatchResult,
): BatchToastContent {
	if (result.failed === 0) {
		return {
			variant: "success",
			title: t(`files:batch_${operation}_success`, { count: result.succeeded }),
		};
	}

	if (result.succeeded === 0) {
		return {
			variant: "error",
			title: t(`files:batch_${operation}_failed`),
			description: formatBatchErrorDescription(t, result.errors),
		};
	}

	return {
		variant: operation === "move" ? "error" : "success",
		title: t(`files:batch_${operation}_partial`, {
			succeeded: result.succeeded,
			failed: result.failed,
		}),
		description: formatBatchErrorDescription(t, result.errors),
	};
}
