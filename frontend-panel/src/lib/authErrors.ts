import { ErrorCode } from "@/types/api-helpers";

function readApiCode(value: unknown): number | string | null {
	if (typeof value !== "object" || value === null) {
		return null;
	}

	const code = "code" in value ? value.code : null;
	return typeof code === "number" || typeof code === "string" ? code : null;
}

function readApiResponseCode(error: unknown): number | string | null {
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
	const numericCode = Number(code);
	return (
		numericCode === ErrorCode.TokenExpired ||
		numericCode === ErrorCode.TokenInvalid ||
		numericCode === ErrorCode.TokenMissing ||
		numericCode === ErrorCode.RefreshTokenReuseDetected
	);
}

export function isStaleRefreshTokenError(error: unknown): boolean {
	const code = readApiCode(error) ?? readApiResponseCode(error);
	return Number(code) === ErrorCode.RefreshTokenStale;
}
