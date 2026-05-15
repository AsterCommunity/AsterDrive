type JsonRecord = Record<string, unknown>;

export class WebAuthnUnsupportedError extends Error {
	constructor() {
		super("WebAuthn is not supported by this browser");
		this.name = "WebAuthnUnsupportedError";
	}
}

export class WebAuthnCancelledError extends Error {
	constructor(message = "WebAuthn ceremony was cancelled") {
		super(message);
		this.name = "WebAuthnCancelledError";
	}
}

export class WebAuthnOptionsError extends Error {
	constructor(message = "Invalid WebAuthn options from server") {
		super(message);
		this.name = "WebAuthnOptionsError";
	}
}

export function isWebAuthnSupported(): boolean {
	return (
		typeof window !== "undefined" &&
		typeof window.PublicKeyCredential !== "undefined" &&
		typeof navigator !== "undefined" &&
		typeof navigator.credentials?.create === "function" &&
		typeof navigator.credentials?.get === "function"
	);
}

export async function isConditionalPasskeyLoginAvailable(): Promise<boolean> {
	if (!isWebAuthnSupported()) {
		return false;
	}
	if (
		typeof PublicKeyCredential.isConditionalMediationAvailable !== "function"
	) {
		return false;
	}
	return PublicKeyCredential.isConditionalMediationAvailable();
}

function ensureSupported() {
	if (!isWebAuthnSupported()) {
		throw new WebAuthnUnsupportedError();
	}
}

function isRecord(value: unknown): value is JsonRecord {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function base64UrlToArrayBuffer(value: string): ArrayBuffer {
	const padded = value
		.replace(/-/g, "+")
		.replace(/_/g, "/")
		.padEnd(Math.ceil(value.length / 4) * 4, "=");
	const binary = window.atob(padded);
	const bytes = new Uint8Array(binary.length);
	for (let index = 0; index < binary.length; index += 1) {
		bytes[index] = binary.charCodeAt(index);
	}
	return bytes.buffer;
}

function arrayBufferToBase64Url(buffer: ArrayBuffer): string {
	const bytes = new Uint8Array(buffer);
	const binary = Array.from(bytes, (byte) => String.fromCharCode(byte)).join(
		"",
	);
	return window
		.btoa(binary)
		.replace(/\+/g, "-")
		.replace(/\//g, "_")
		.replace(/=+$/g, "");
}

function decodeCredentialDescriptorIds(value: unknown): unknown {
	if (!Array.isArray(value)) return value;
	return value.map((item) => {
		if (!isRecord(item) || typeof item.id !== "string") {
			return item;
		}
		return {
			...item,
			id: base64UrlToArrayBuffer(item.id),
		};
	});
}

function registrationOptionsFromServer(
	options: unknown,
): PublicKeyCredentialCreationOptions {
	if (!isRecord(options) || !isRecord(options.publicKey)) {
		throw new WebAuthnOptionsError();
	}

	const publicKey = options.publicKey;
	const user = isRecord(publicKey.user) ? publicKey.user : {};
	return {
		...publicKey,
		challenge:
			typeof publicKey.challenge === "string"
				? base64UrlToArrayBuffer(publicKey.challenge)
				: publicKey.challenge,
		excludeCredentials: decodeCredentialDescriptorIds(
			publicKey.excludeCredentials,
		) as PublicKeyCredentialDescriptor[] | undefined,
		user: {
			...user,
			id:
				typeof user.id === "string" ? base64UrlToArrayBuffer(user.id) : user.id,
		} as PublicKeyCredentialUserEntity,
	} as PublicKeyCredentialCreationOptions;
}

function authenticationOptionsFromServer(
	options: unknown,
): CredentialRequestOptions {
	if (!isRecord(options) || !isRecord(options.publicKey)) {
		throw new WebAuthnOptionsError();
	}

	const publicKey = options.publicKey;
	const credentialOptions: CredentialRequestOptions = {
		publicKey: {
			...publicKey,
			allowCredentials: decodeCredentialDescriptorIds(
				publicKey.allowCredentials,
			) as PublicKeyCredentialDescriptor[] | undefined,
			challenge:
				typeof publicKey.challenge === "string"
					? base64UrlToArrayBuffer(publicKey.challenge)
					: publicKey.challenge,
		} as PublicKeyCredentialRequestOptions,
	};
	if (typeof options.mediation === "string") {
		credentialOptions.mediation =
			options.mediation as CredentialMediationRequirement;
	}
	return credentialOptions;
}

function getCredentialTransports(
	response: AuthenticatorAttestationResponse,
): string[] | undefined {
	if (typeof response.getTransports !== "function") {
		return undefined;
	}
	return response.getTransports();
}

function serializeRegistrationCredential(
	credential: PublicKeyCredential,
): JsonRecord {
	const response = credential.response;
	if (!(response instanceof AuthenticatorAttestationResponse)) {
		throw new WebAuthnCancelledError("Invalid registration response");
	}

	return {
		id: credential.id,
		rawId: arrayBufferToBase64Url(credential.rawId),
		response: {
			attestationObject: arrayBufferToBase64Url(response.attestationObject),
			clientDataJSON: arrayBufferToBase64Url(response.clientDataJSON),
			transports: getCredentialTransports(response),
		},
		type: credential.type,
		clientExtensionResults: credential.getClientExtensionResults(),
	};
}

function serializeAuthenticationCredential(
	credential: PublicKeyCredential,
): JsonRecord {
	const response = credential.response;
	if (!(response instanceof AuthenticatorAssertionResponse)) {
		throw new WebAuthnCancelledError("Invalid authentication response");
	}

	return {
		id: credential.id,
		rawId: arrayBufferToBase64Url(credential.rawId),
		response: {
			authenticatorData: arrayBufferToBase64Url(response.authenticatorData),
			clientDataJSON: arrayBufferToBase64Url(response.clientDataJSON),
			signature: arrayBufferToBase64Url(response.signature),
			userHandle: response.userHandle
				? arrayBufferToBase64Url(response.userHandle)
				: undefined,
		},
		type: credential.type,
		clientExtensionResults: credential.getClientExtensionResults(),
	};
}

function normalizeCredentialResult(
	value: Credential | null,
): PublicKeyCredential {
	if (!value || !(value instanceof PublicKeyCredential)) {
		throw new WebAuthnCancelledError();
	}
	return value;
}

function normalizeWebAuthnError(error: unknown): never {
	if (error instanceof WebAuthnUnsupportedError) {
		throw error;
	}
	if (error instanceof DOMException) {
		if (error.name !== "NotAllowedError" && error.name !== "AbortError") {
			throw error;
		}
		throw new WebAuthnCancelledError(error.message);
	}
	throw error;
}

export async function createPasskeyCredential(
	options: unknown,
): Promise<JsonRecord> {
	try {
		ensureSupported();
		const credential = await navigator.credentials.create({
			publicKey: registrationOptionsFromServer(options),
		});
		return serializeRegistrationCredential(
			normalizeCredentialResult(credential),
		);
	} catch (error) {
		normalizeWebAuthnError(error);
	}
}

export async function getPasskeyCredential(
	options: unknown,
	mediation?: CredentialMediationRequirement,
	signal?: AbortSignal,
): Promise<JsonRecord> {
	try {
		ensureSupported();
		const requestOptions = authenticationOptionsFromServer(options);
		if (mediation) {
			requestOptions.mediation = mediation;
		}
		if (signal) {
			requestOptions.signal = signal;
		}
		const credential = await navigator.credentials.get(requestOptions);
		return serializeAuthenticationCredential(
			normalizeCredentialResult(credential),
		);
	} catch (error) {
		normalizeWebAuthnError(error);
	}
}
