import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiErrorCode } from "@/types/api-helpers";

const mockState = vi.hoisted(() => {
	class MockApiError extends Error {
		code: string;
		diagnostic?: { message?: string };
		retryable?: boolean;

		constructor(
			code: string,
			message: string,
			details: { diagnostic?: { message?: string }; retryable?: boolean } = {},
		) {
			super(message);
			this.code = code;
			this.diagnostic = details.diagnostic;
			this.retryable = details.retryable;
		}
	}

	return {
		ApiError: MockApiError,
		isAxiosError: vi.fn(
			(error: unknown) =>
				typeof error === "object" &&
				error !== null &&
				"isAxiosError" in error &&
				(error as { isAxiosError?: boolean }).isAxiosError === true,
		),
		toastError: vi.fn(),
		exists: vi.fn(() => true),
		translate: vi.fn((key: string) => `translated:${key}`),
	};
});

vi.mock("axios", () => ({
	default: {
		isAxiosError: mockState.isAxiosError,
	},
	isAxiosError: mockState.isAxiosError,
}));

vi.mock("sonner", () => ({
	toast: {
		error: mockState.toastError,
	},
}));

vi.mock("@/i18n", () => ({
	default: {
		exists: mockState.exists,
		t: mockState.translate,
	},
}));

vi.mock("@/services/http", () => ({
	ApiError: mockState.ApiError,
}));

describe("handleApiError", () => {
	beforeEach(() => {
		mockState.isAxiosError.mockClear();
		mockState.exists.mockReset();
		mockState.exists.mockReturnValue(true);
		mockState.toastError.mockReset();
		mockState.translate.mockClear();
	});

	it("maps ApiError codes to translated messages", async () => {
		const { handleApiError } = await import("@/hooks/useApiError");

		handleApiError(new mockState.ApiError(ApiErrorCode.Forbidden, "raw"));
		handleApiError(
			new mockState.ApiError(ApiErrorCode.PendingActivation, "pending"),
		);
		handleApiError(
			new mockState.ApiError(ApiErrorCode.TokenMissing, "missing"),
		);
		handleApiError(
			new mockState.ApiError(ApiErrorCode.CredentialsFailed, "credentials"),
		);
		handleApiError(
			new mockState.ApiError(ApiErrorCode.UploadHashTempReadFailed, "upload"),
		);

		expect(mockState.translate).toHaveBeenCalledWith("errors:forbidden");
		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:auth_pending_activation",
		);
		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:auth_token_missing",
		);
		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:auth_credentials_failed",
		);
		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:upload_hash_temp_read_failed",
		);
		expect(mockState.toastError).toHaveBeenNthCalledWith(
			1,
			"translated:errors:forbidden",
		);
		expect(mockState.toastError).toHaveBeenNthCalledWith(
			2,
			"translated:errors:auth_pending_activation",
		);
		expect(mockState.toastError).toHaveBeenNthCalledWith(
			3,
			"translated:errors:auth_token_missing",
		);
		expect(mockState.toastError).toHaveBeenNthCalledWith(
			4,
			"translated:errors:auth_credentials_failed",
		);
		expect(mockState.toastError).toHaveBeenNthCalledWith(
			5,
			"translated:errors:upload_hash_temp_read_failed",
		);
	});

	it("falls back to the raw ApiError message when no translation exists", async () => {
		mockState.exists.mockReturnValue(false);
		const { handleApiError } = await import("@/hooks/useApiError");

		handleApiError(
			new mockState.ApiError(
				ApiErrorCode.RemoteNodeEnrollmentRequired,
				"remote enrollment is required",
			),
		);

		expect(mockState.exists).toHaveBeenCalledWith(
			"errors:remote_node_enrollment_required",
		);
		expect(mockState.toastError).toHaveBeenCalledWith(
			"remote enrollment is required",
		);
	});

	it("prefers diagnostic messages from ApiError details", async () => {
		const { handleApiError } = await import("@/hooks/useApiError");
		const diagnosticMessage = "connection test failed";

		handleApiError(
			new mockState.ApiError(
				ApiErrorCode.StorageMisconfigured,
				"Storage Driver Error",
				{
					diagnostic: {
						message: diagnosticMessage,
					},
				},
			),
		);

		expect(mockState.toastError).toHaveBeenCalledWith(diagnosticMessage);
	});

	it("falls back to unexpected error text for blank ApiError messages", async () => {
		mockState.exists.mockReturnValue(false);
		const { handleApiError } = await import("@/hooks/useApiError");

		handleApiError(new mockState.ApiError(ApiErrorCode.Conflict, "   "));

		expect(mockState.toastError).toHaveBeenCalledWith(
			"translated:errors:unexpected_error",
		);
	});

	it("falls back to the raw message for unknown Error instances", async () => {
		const { handleApiError } = await import("@/hooks/useApiError");

		handleApiError(new Error("plain failure"));
		handleApiError("unexpected");

		expect(mockState.toastError).toHaveBeenNthCalledWith(1, "plain failure");
		expect(mockState.toastError).toHaveBeenNthCalledWith(
			2,
			"translated:errors:unexpected_error",
		);
	});

	it("maps transport failures to localized messages", async () => {
		const { handleApiError } = await import("@/hooks/useApiError");

		handleApiError({
			isAxiosError: true,
			message: "Network Error",
		});
		handleApiError(new Error("Failed to fetch"));
		handleApiError(
			Object.assign(new Error("timeout of 30000ms exceeded"), {
				code: "ECONNABORTED",
				isAxiosError: true,
			}),
		);

		expect(mockState.toastError).toHaveBeenNthCalledWith(
			1,
			"translated:errors:network_error",
		);
		expect(mockState.toastError).toHaveBeenNthCalledWith(
			2,
			"translated:errors:network_error",
		);
		expect(mockState.toastError).toHaveBeenNthCalledWith(
			3,
			"translated:errors:request_timeout",
		);
	});

	it("treats blank generic messages as unexpected errors", async () => {
		const { handleApiError } = await import("@/hooks/useApiError");

		handleApiError(new Error("\n\t"));

		expect(mockState.toastError).toHaveBeenCalledWith(
			"translated:errors:unexpected_error",
		);
	});

	it("ignores canceled transport errors", async () => {
		const { handleApiError } = await import("@/hooks/useApiError");

		handleApiError(
			Object.assign(new Error("canceled"), { code: "ERR_CANCELED" }),
		);

		expect(mockState.toastError).toHaveBeenCalledWith("canceled");
	});
});
