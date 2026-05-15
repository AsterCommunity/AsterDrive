import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
	createPasskeyCredential,
	getPasskeyCredential,
	isConditionalPasskeyLoginAvailable,
	isWebAuthnSupported,
	WebAuthnCancelledError,
	WebAuthnOptionsError,
	WebAuthnUnsupportedError,
} from "@/lib/webauthn";

class MockPublicKeyCredential {
	id: string;
	rawId: ArrayBuffer;
	response: unknown;
	type = "public-key";

	constructor({
		id = "credential-id",
		rawId = bytes(1, 2, 3).buffer,
		response,
	}: {
		id?: string;
		rawId?: ArrayBuffer;
		response: unknown;
	}) {
		this.id = id;
		this.rawId = rawId;
		this.response = response;
	}

	getClientExtensionResults() {
		return { appid: false };
	}

	static isConditionalMediationAvailable = vi.fn();
}

class MockAuthenticatorAttestationResponse {
	attestationObject: ArrayBuffer;
	clientDataJSON: ArrayBuffer;
	transports?: string[];

	constructor({
		attestationObject = bytes(4, 5, 6).buffer,
		clientDataJSON = bytes(7, 8, 9).buffer,
		transports,
	}: {
		attestationObject?: ArrayBuffer;
		clientDataJSON?: ArrayBuffer;
		transports?: string[];
	} = {}) {
		this.attestationObject = attestationObject;
		this.clientDataJSON = clientDataJSON;
		this.transports = transports;
	}

	getTransports() {
		return this.transports ?? ["usb", "internal"];
	}
}

class MockAuthenticatorAssertionResponse {
	authenticatorData: ArrayBuffer;
	clientDataJSON: ArrayBuffer;
	signature: ArrayBuffer;
	userHandle?: ArrayBuffer | null;

	constructor({
		authenticatorData = bytes(10, 11, 12).buffer,
		clientDataJSON = bytes(13, 14, 15).buffer,
		signature = bytes(16, 17, 18).buffer,
		userHandle,
	}: {
		authenticatorData?: ArrayBuffer;
		clientDataJSON?: ArrayBuffer;
		signature?: ArrayBuffer;
		userHandle?: ArrayBuffer | null;
	} = {}) {
		this.authenticatorData = authenticatorData;
		this.clientDataJSON = clientDataJSON;
		this.signature = signature;
		this.userHandle = userHandle;
	}
}

const credentialMocks = {
	create: vi.fn(),
	get: vi.fn(),
};

function bytes(...values: number[]) {
	return Uint8Array.from(values);
}

function decodeBuffer(buffer: ArrayBuffer) {
	return Array.from(new Uint8Array(buffer));
}

function installWebAuthnGlobals() {
	vi.stubGlobal("PublicKeyCredential", MockPublicKeyCredential);
	vi.stubGlobal(
		"AuthenticatorAttestationResponse",
		MockAuthenticatorAttestationResponse,
	);
	vi.stubGlobal(
		"AuthenticatorAssertionResponse",
		MockAuthenticatorAssertionResponse,
	);
	Object.defineProperty(navigator, "credentials", {
		configurable: true,
		value: credentialMocks,
	});
}

describe("webauthn", () => {
	beforeEach(() => {
		credentialMocks.create.mockReset();
		credentialMocks.get.mockReset();
		MockPublicKeyCredential.isConditionalMediationAvailable.mockReset();
		installWebAuthnGlobals();
	});

	afterEach(() => {
		vi.unstubAllGlobals();
		Object.defineProperty(navigator, "credentials", {
			configurable: true,
			value: undefined,
		});
	});

	it("detects WebAuthn and conditional passkey support", async () => {
		MockPublicKeyCredential.isConditionalMediationAvailable.mockResolvedValue(
			true,
		);

		expect(isWebAuthnSupported()).toBe(true);
		await expect(isConditionalPasskeyLoginAvailable()).resolves.toBe(true);
		expect(
			MockPublicKeyCredential.isConditionalMediationAvailable,
		).toHaveBeenCalledWith();
	});

	it("reports unavailable support when required browser APIs are missing", async () => {
		Object.defineProperty(navigator, "credentials", {
			configurable: true,
			value: { create: vi.fn() },
		});

		expect(isWebAuthnSupported()).toBe(false);
		await expect(isConditionalPasskeyLoginAvailable()).resolves.toBe(false);

		installWebAuthnGlobals();
		vi.stubGlobal("PublicKeyCredential", class {});

		expect(isWebAuthnSupported()).toBe(true);
		await expect(isConditionalPasskeyLoginAvailable()).resolves.toBe(false);
	});

	it("creates a passkey by decoding server options and serializing the response", async () => {
		credentialMocks.create.mockResolvedValue(
			new MockPublicKeyCredential({
				rawId: bytes(251, 255).buffer,
				response: new MockAuthenticatorAttestationResponse({
					attestationObject: bytes(1, 2, 3).buffer,
					clientDataJSON: bytes(4, 5).buffer,
					transports: ["hybrid"],
				}),
			}),
		);

		await expect(
			createPasskeyCredential({
				publicKey: {
					challenge: "AQID",
					excludeCredentials: [
						{ id: "BAUG", type: "public-key" },
						{ id: bytes(7).buffer, type: "public-key" },
						"kept-as-is",
					],
					rp: { name: "AsterDrive" },
					user: {
						displayName: "Alice",
						id: "BwgJ",
						name: "alice@example.com",
					},
				},
			}),
		).resolves.toEqual({
			clientExtensionResults: { appid: false },
			id: "credential-id",
			rawId: "-_8",
			response: {
				attestationObject: "AQID",
				clientDataJSON: "BAU",
				transports: ["hybrid"],
			},
			type: "public-key",
		});

		expect(credentialMocks.create).toHaveBeenCalledWith({
			publicKey: expect.objectContaining({
				excludeCredentials: [
					{
						id: expect.any(ArrayBuffer),
						type: "public-key",
					},
					{
						id: bytes(7).buffer,
						type: "public-key",
					},
					"kept-as-is",
				],
				rp: { name: "AsterDrive" },
				user: expect.objectContaining({
					displayName: "Alice",
					id: expect.any(ArrayBuffer),
					name: "alice@example.com",
				}),
			}),
		});
		const createOptions = credentialMocks.create.mock.calls[0]?.[0] as {
			publicKey: PublicKeyCredentialCreationOptions;
		};
		expect(
			decodeBuffer(createOptions.publicKey.challenge as ArrayBuffer),
		).toEqual([1, 2, 3]);
		expect(
			decodeBuffer(createOptions.publicKey.user.id as ArrayBuffer),
		).toEqual([7, 8, 9]);
		expect(
			decodeBuffer(
				createOptions.publicKey.excludeCredentials?.[0]?.id as ArrayBuffer,
			),
		).toEqual([4, 5, 6]);
	});

	it("keeps non-string registration challenges and missing transport helpers intact", async () => {
		const challenge = bytes(8, 9).buffer;
		const userId = bytes(1).buffer;
		const response = new MockAuthenticatorAttestationResponse({
			attestationObject: bytes(2).buffer,
			clientDataJSON: bytes(3).buffer,
		}) as MockAuthenticatorAttestationResponse & {
			getTransports?: () => string[];
		};
		response.getTransports = undefined;
		credentialMocks.create.mockResolvedValue(
			new MockPublicKeyCredential({
				response,
			}),
		);

		await expect(
			createPasskeyCredential({
				publicKey: {
					challenge,
					rp: { name: "AsterDrive" },
					user: {
						displayName: "Alice",
						id: userId,
						name: "alice@example.com",
					},
				},
			}),
		).resolves.toMatchObject({
			response: {
				transports: undefined,
			},
		});

		const createOptions = credentialMocks.create.mock.calls[0]?.[0] as {
			publicKey: PublicKeyCredentialCreationOptions;
		};
		expect(createOptions.publicKey.challenge).toBe(challenge);
		expect(createOptions.publicKey.user.id).toBe(userId);
	});

	it("gets a passkey by merging mediation and abort signal into request options", async () => {
		const controller = new AbortController();
		credentialMocks.get.mockResolvedValue(
			new MockPublicKeyCredential({
				rawId: bytes(170, 187).buffer,
				response: new MockAuthenticatorAssertionResponse({
					authenticatorData: bytes(1).buffer,
					clientDataJSON: bytes(2).buffer,
					signature: bytes(3).buffer,
					userHandle: bytes(4, 5).buffer,
				}),
			}),
		);

		await expect(
			getPasskeyCredential(
				{
					mediation: "optional",
					publicKey: {
						allowCredentials: [{ id: "CQo", type: "public-key" }],
						challenge: "BgcI",
					},
				},
				"conditional",
				controller.signal,
			),
		).resolves.toEqual({
			clientExtensionResults: { appid: false },
			id: "credential-id",
			rawId: "qrs",
			response: {
				authenticatorData: "AQ",
				clientDataJSON: "Ag",
				signature: "Aw",
				userHandle: "BAU",
			},
			type: "public-key",
		});

		const requestOptions = credentialMocks.get.mock.calls[0]?.[0] as {
			mediation: CredentialMediationRequirement;
			publicKey: PublicKeyCredentialRequestOptions;
			signal: AbortSignal;
		};
		expect(requestOptions.mediation).toBe("conditional");
		expect(requestOptions.signal).toBe(controller.signal);
		expect(
			decodeBuffer(requestOptions.publicKey.challenge as ArrayBuffer),
		).toEqual([6, 7, 8]);
		expect(
			decodeBuffer(
				requestOptions.publicKey.allowCredentials?.[0]?.id as ArrayBuffer,
			),
		).toEqual([9, 10]);
	});

	it("serializes authentication responses without a user handle", async () => {
		credentialMocks.get.mockResolvedValue(
			new MockPublicKeyCredential({
				response: new MockAuthenticatorAssertionResponse({
					userHandle: null,
				}),
			}),
		);

		await expect(
			getPasskeyCredential({ publicKey: { challenge: "AQ" } }),
		).resolves.toMatchObject({
			response: {
				userHandle: undefined,
			},
		});
	});

	it("normalizes unsupported, cancelled, null, and invalid WebAuthn results", async () => {
		Object.defineProperty(navigator, "credentials", {
			configurable: true,
			value: { create: undefined, get: undefined },
		});

		await expect(
			createPasskeyCredential({ publicKey: { challenge: "AQ" } }),
		).rejects.toBeInstanceOf(WebAuthnUnsupportedError);

		installWebAuthnGlobals();
		credentialMocks.create.mockRejectedValue(
			new DOMException("User cancelled", "NotAllowedError"),
		);

		await expect(
			createPasskeyCredential({ publicKey: { challenge: "AQ" } }),
		).rejects.toMatchObject({
			message: "User cancelled",
			name: "WebAuthnCancelledError",
		});

		credentialMocks.create.mockRejectedValue(
			new DOMException("Aborted", "AbortError"),
		);
		await expect(
			createPasskeyCredential({ publicKey: { challenge: "AQ" } }),
		).rejects.toMatchObject({
			message: "Aborted",
			name: "WebAuthnCancelledError",
		});

		const browserError = new DOMException(
			"Unknown browser fault",
			"UnknownError",
		);
		credentialMocks.create.mockRejectedValue(browserError);
		await expect(
			createPasskeyCredential({ publicKey: { challenge: "AQ" } }),
		).rejects.toBe(browserError);

		credentialMocks.create.mockResolvedValue(null);
		await expect(
			createPasskeyCredential({ publicKey: { challenge: "AQ" } }),
		).rejects.toBeInstanceOf(WebAuthnCancelledError);

		credentialMocks.create.mockResolvedValue(
			new MockPublicKeyCredential({
				response: new MockAuthenticatorAssertionResponse(),
			}),
		);
		await expect(
			createPasskeyCredential({ publicKey: { challenge: "AQ" } }),
		).rejects.toMatchObject({
			message: "Invalid registration response",
			name: "WebAuthnCancelledError",
		});

		credentialMocks.get.mockResolvedValue(
			new MockPublicKeyCredential({
				response: new MockAuthenticatorAttestationResponse(),
			}),
		);
		await expect(
			getPasskeyCredential({ publicKey: { challenge: "AQ" } }),
		).rejects.toMatchObject({
			message: "Invalid authentication response",
			name: "WebAuthnCancelledError",
		});
	});

	it("throws unsupported errors for malformed server options and rethrows unknown errors", async () => {
		await expect(createPasskeyCredential({})).rejects.toBeInstanceOf(
			WebAuthnOptionsError,
		);
		await expect(
			getPasskeyCredential({ publicKey: null }),
		).rejects.toBeInstanceOf(WebAuthnOptionsError);

		const error = new Error("network");
		credentialMocks.get.mockRejectedValue(error);

		await expect(
			getPasskeyCredential({ publicKey: { challenge: "AQ" } }),
		).rejects.toBe(error);
	});
});
