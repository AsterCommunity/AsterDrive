import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiSubcode, ErrorCode } from "@/types/api-helpers";

const mockState = vi.hoisted(() => {
	class MockApiError extends Error {
		code: number;
		subcode?: string;

		constructor(code: number, message: string, subcode?: string) {
			super(message);
			this.code = code;
			this.subcode = subcode;
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
		t: mockState.translate,
	},
}));

vi.mock("@/services/http", () => ({
	ApiError: mockState.ApiError,
}));

describe("handleApiError", () => {
	beforeEach(() => {
		mockState.isAxiosError.mockClear();
		mockState.toastError.mockReset();
		mockState.translate.mockClear();
	});

	it("maps known ApiError codes to translated messages", async () => {
		const { handleApiError } = await import("@/hooks/useApiError");

		handleApiError(new mockState.ApiError(ErrorCode.Forbidden, "raw message"));
		handleApiError(
			new mockState.ApiError(ErrorCode.PendingActivation, "pending"),
		);
		handleApiError(new mockState.ApiError(ErrorCode.TokenMissing, "missing"));
		handleApiError(
			new mockState.ApiError(ErrorCode.CredentialsFailed, "credentials"),
		);
		handleApiError(new mockState.ApiError(ErrorCode.MfaFailed, "mfa failed"));
		handleApiError(
			new mockState.ApiError(ErrorCode.RefreshTokenStale, "stale"),
		);
		handleApiError(
			new mockState.ApiError(
				ErrorCode.RefreshTokenReuseDetected,
				"reuse detected",
			),
		);

		expect(mockState.translate).toHaveBeenCalledWith("errors:forbidden");
		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:pending_activation",
		);
		expect(mockState.translate).toHaveBeenCalledWith("errors:token_missing");
		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:credentials_failed",
		);
		expect(mockState.translate).toHaveBeenCalledWith("errors:mfa_failed");
		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:refresh_token_stale",
		);
		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:refresh_token_reuse_detected",
		);
		expect(mockState.toastError).toHaveBeenCalledWith(
			"translated:errors:forbidden",
		);
		expect(mockState.toastError).toHaveBeenCalledWith(
			"translated:errors:pending_activation",
		);
		expect(mockState.toastError).toHaveBeenCalledWith(
			"translated:errors:token_missing",
		);
		expect(mockState.toastError).toHaveBeenCalledWith(
			"translated:errors:refresh_token_stale",
		);
		expect(mockState.toastError).toHaveBeenCalledWith(
			"translated:errors:refresh_token_reuse_detected",
		);
		expect(mockState.toastError).toHaveBeenCalledWith(
			"translated:errors:credentials_failed",
		);
		expect(mockState.toastError).toHaveBeenCalledWith(
			"translated:errors:mfa_failed",
		);
	});

	it("falls back to subcode translations when the top-level code is generic", async () => {
		const { handleApiError } = await import("@/hooks/useApiError");

		handleApiError(
			new mockState.ApiError(
				ErrorCode.StorageDriverError,
				"Storage Driver Error",
				ApiSubcode.StorageTransient,
			),
		);

		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:storage_transient_failure",
		);
		expect(mockState.toastError).toHaveBeenCalledWith(
			"translated:errors:storage_transient_failure",
		);
	});

	it("maps managed ingress precondition subcodes", async () => {
		const { handleApiError } = await import("@/hooks/useApiError");

		handleApiError(
			new mockState.ApiError(
				ErrorCode.PreconditionFailed,
				"managed ingress required",
				ApiSubcode.ManagedIngressRequired,
			),
		);

		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:managed_ingress_required",
		);
		expect(mockState.toastError).toHaveBeenCalledWith(
			"translated:errors:managed_ingress_required",
		);
	});

	it("prefers auth security subcodes over generic forbidden errors", async () => {
		const { handleApiError } = await import("@/hooks/useApiError");

		handleApiError(
			new mockState.ApiError(
				ErrorCode.Forbidden,
				"untrusted request origin",
				ApiSubcode.AuthRequestOriginUntrusted,
			),
		);
		handleApiError(
			new mockState.ApiError(
				ErrorCode.Forbidden,
				"invalid CSRF token",
				ApiSubcode.AuthCsrfTokenInvalid,
			),
		);
		handleApiError(
			new mockState.ApiError(
				ErrorCode.Forbidden,
				"new user registration is disabled",
				ApiSubcode.AuthRegistrationDisabled,
			),
		);

		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:auth_request_origin_untrusted",
		);
		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:auth_csrf_token_invalid",
		);
		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:auth_registration_disabled",
		);
		expect(mockState.toastError).toHaveBeenNthCalledWith(
			1,
			"translated:errors:auth_request_origin_untrusted",
		);
		expect(mockState.toastError).toHaveBeenNthCalledWith(
			2,
			"translated:errors:auth_csrf_token_invalid",
		);
		expect(mockState.toastError).toHaveBeenNthCalledWith(
			3,
			"translated:errors:auth_registration_disabled",
		);
	});

	it("maps team and workspace authorization subcodes", async () => {
		const { handleApiError } = await import("@/hooks/useApiError");

		handleApiError(
			new mockState.ApiError(
				ErrorCode.Forbidden,
				"team owner role is required",
				ApiSubcode.TeamOwnerRequired,
			),
		);
		handleApiError(
			new mockState.ApiError(
				ErrorCode.Forbidden,
				"resource outside workspace scope",
				ApiSubcode.WorkspaceScopeDenied,
			),
		);
		handleApiError(
			new mockState.ApiError(
				ErrorCode.Forbidden,
				"team owner or admin role is required",
				ApiSubcode.TeamAdminOrOwnerRequired,
			),
		);

		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:team_owner_required",
		);
		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:workspace_scope_denied",
		);
		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:team_admin_or_owner_required",
		);
		expect(mockState.toastError).toHaveBeenNthCalledWith(
			1,
			"translated:errors:team_owner_required",
		);
		expect(mockState.toastError).toHaveBeenNthCalledWith(
			2,
			"translated:errors:workspace_scope_denied",
		);
		expect(mockState.toastError).toHaveBeenNthCalledWith(
			3,
			"translated:errors:team_admin_or_owner_required",
		);
	});

	it("maps validation subcodes over generic bad request errors", async () => {
		const { handleApiError } = await import("@/hooks/useApiError");

		handleApiError(
			new mockState.ApiError(
				ErrorCode.BadRequest,
				"invalid Origin header",
				ApiSubcode.ValidationRequestOriginInvalid,
			),
		);

		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:validation_request_origin_invalid",
		);
		expect(mockState.toastError).toHaveBeenCalledWith(
			"translated:errors:validation_request_origin_invalid",
		);
	});

	it("prefers subcode translations over generic upload codes", async () => {
		const { handleApiError } = await import("@/hooks/useApiError");

		handleApiError(
			new mockState.ApiError(
				ErrorCode.FileUploadFailed,
				"Upload Failed",
				ApiSubcode.UploadTempFileWriteFailed,
			),
		);

		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:upload_temp_file_write_failed",
		);
		expect(mockState.toastError).toHaveBeenCalledWith(
			"translated:errors:upload_temp_file_write_failed",
		);
	});

	it("uses subcode translations for structured conflict errors", async () => {
		const { handleApiError } = await import("@/hooks/useApiError");

		handleApiError(
			new mockState.ApiError(
				ErrorCode.Conflict,
				"email already exists",
				ApiSubcode.AuthEmailExists,
			),
		);

		expect(mockState.translate).toHaveBeenCalledWith(
			"errors:auth_email_exists",
		);
		expect(mockState.toastError).toHaveBeenCalledWith(
			"translated:errors:auth_email_exists",
		);
	});

	it("ignores unknown dynamic subcodes and falls back to the top-level code", async () => {
		const { handleApiError } = await import("@/hooks/useApiError");

		handleApiError(
			new mockState.ApiError(
				ErrorCode.Forbidden,
				"remote denied this operation",
				"remote.dynamic",
			),
		);

		expect(mockState.translate).toHaveBeenCalledWith("errors:forbidden");
		expect(mockState.toastError).toHaveBeenCalledWith(
			"translated:errors:forbidden",
		);
	});

	it("falls back to the raw message for known subcodes without a local message key", async () => {
		const { handleApiError } = await import("@/hooks/useApiError");

		handleApiError(
			new mockState.ApiError(
				ErrorCode.DatabaseError,
				"remote enrollment is required",
				ApiSubcode.RemoteNodeEnrollmentRequired,
			),
		);

		expect(mockState.toastError).toHaveBeenCalledWith(
			"remote enrollment is required",
		);
	});

	it("falls back to the raw message for unknown errors", async () => {
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

	it("treats blank messages as unexpected errors", async () => {
		const { handleApiError } = await import("@/hooks/useApiError");

		handleApiError(new mockState.ApiError(ErrorCode.Conflict, "   "));
		handleApiError(new Error("\n\t"));

		expect(mockState.toastError).toHaveBeenNthCalledWith(
			1,
			"translated:errors:unexpected_error",
		);
		expect(mockState.toastError).toHaveBeenNthCalledWith(
			2,
			"translated:errors:unexpected_error",
		);
	});
});
