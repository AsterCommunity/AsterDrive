import axios from "axios";
import { toast } from "sonner";
import i18n from "@/i18n";
import { ApiError } from "@/services/http";
import { isApiErrorCode } from "@/types/api-helpers";

function errorCodeToMessageKey(code: string): string {
	return `errors:${code.replaceAll(".", "_")}`;
}

function getErrorCode(error: unknown): string | undefined {
	if (typeof error !== "object" || error === null || !("code" in error)) {
		return undefined;
	}
	return typeof error.code === "string" ? error.code : undefined;
}

function getTrimmedErrorMessage(error: Error): string {
	return error.message.trim();
}

function getTransportErrorMessageKey(error: unknown): string | null {
	const code = getErrorCode(error);
	if (code === "ERR_CANCELED") {
		return null;
	}

	const message =
		error instanceof Error ? getTrimmedErrorMessage(error) : undefined;
	const normalizedMessage = message?.toLowerCase();

	const timedOut =
		code === "ECONNABORTED" ||
		code === "ETIMEDOUT" ||
		normalizedMessage?.includes("timeout") === true;
	if (timedOut) {
		return "errors:request_timeout";
	}

	if (axios.isAxiosError(error) && !error.response) {
		return "errors:network_error";
	}

	if (
		message === "Network Error" ||
		normalizedMessage === "network error" ||
		message === "Failed to fetch" ||
		message === "Load failed"
	) {
		return "errors:network_error";
	}

	return null;
}

export function getApiErrorMessage(error: unknown) {
	if (error instanceof ApiError) {
		const diagnosticMessage = error.diagnostic?.message?.trim();
		if (diagnosticMessage) {
			return diagnosticMessage;
		}
		const key = errorCodeToMessageKey(error.code);
		if (i18n.exists(key)) {
			return i18n.t(key);
		}
		const message = error.message.trim();
		return message || i18n.t("errors:unexpected_error");
	}

	const code = getErrorCode(error);
	if (code && isApiErrorCode(code)) {
		const key = errorCodeToMessageKey(code);
		if (i18n.exists(key)) return i18n.t(key);
	}
	const transportErrorKey = getTransportErrorMessageKey(error);
	if (transportErrorKey) {
		return i18n.t(transportErrorKey);
	}

	if (error instanceof Error) {
		const message = getTrimmedErrorMessage(error);
		return message || i18n.t("errors:unexpected_error");
	}

	return i18n.t("errors:unexpected_error");
}

export function handleApiError(error: unknown) {
	toast.error(getApiErrorMessage(error));
}
