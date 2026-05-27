import { ErrorCode } from "@/types/api-helpers";

function readApiCode(value: unknown): number | null {
	if (typeof value !== "object" || value === null) {
		return null;
	}

	const code = "code" in value ? value.code : null;
	return typeof code === "number" ? code : null;
}

function readApiResponseCode(error: unknown): number | null {
	if (typeof error !== "object" || error === null || !("response" in error)) {
		return null;
	}

	const response = error.response;
	if (
		typeof response !== "object" ||
		response === null ||
		!("data" in response)
	) {
		return null;
	}

	return readApiCode(response.data);
}

export function isTokenAuthError(error: unknown): boolean {
	const code = readApiCode(error) ?? readApiResponseCode(error);
	return (
		code === ErrorCode.TokenExpired ||
		code === ErrorCode.TokenInvalid ||
		code === ErrorCode.TokenMissing ||
		code === ErrorCode.RefreshTokenReuseDetected
	);
}

export function isStaleRefreshTokenError(error: unknown): boolean {
	const code = readApiCode(error) ?? readApiResponseCode(error);
	return code === ErrorCode.RefreshTokenStale;
}
