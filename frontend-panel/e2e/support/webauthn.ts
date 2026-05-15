import type { Page, Route } from "@playwright/test";

const TEST_CHALLENGE = "AQIDBA";
const TEST_CREDENTIAL_ID = "credential-e2e";
const TEST_FLOW_ID = "passkey-flow-e2e";
const TEST_USER_HANDLE = "dXNlci1oYW5kbGUtZTJl";

type JsonBody = Record<string, unknown>;

interface MockPasskey {
	backup_eligible: boolean;
	backed_up: boolean;
	created_at: string;
	id: number;
	last_used_at: string | null;
	name: string;
	sign_count: number;
	transports: string[];
	updated_at: string;
}

interface CapturedPasskeyMutation {
	body: JsonBody;
	id: number;
	method: string;
	url: string;
}

function apiResponse(data: unknown) {
	return {
		code: 0,
		data,
		msg: "",
	};
}

async function readJsonBody(route: Route): Promise<JsonBody> {
	const raw = route.request().postData();
	return raw ? (JSON.parse(raw) as JsonBody) : {};
}

export async function installPasskeyBrowserMock(
	page: Page,
	options: {
		conditionalAvailable?: boolean;
		resolveGetManually?: boolean;
		supportCreate?: boolean;
	} = {},
) {
	await page.addInitScript(
		({ conditionalAvailable, resolveGetManually, supportCreate }) => {
			const encoder = new TextEncoder();

			class MockAuthenticatorAttestationResponse {
				readonly attestationObject =
					encoder.encode("attestation-object").buffer;
				readonly clientDataJSON = encoder.encode(
					"registration-client-data",
				).buffer;

				getTransports() {
					return ["internal"];
				}
			}

			class MockAuthenticatorAssertionResponse {
				readonly authenticatorData =
					encoder.encode("authenticator-data").buffer;
				readonly clientDataJSON = encoder.encode("client-data").buffer;
				readonly signature = encoder.encode("signature").buffer;
				readonly userHandle = encoder.encode("user-handle-e2e").buffer;
			}

			class MockPublicKeyCredential {
				static async isConditionalMediationAvailable() {
					return conditionalAvailable;
				}

				readonly id = "credential-e2e";
				readonly rawId = encoder.encode("credential-e2e").buffer;
				readonly response = new MockAuthenticatorAssertionResponse();
				readonly type = "public-key";

				getClientExtensionResults() {
					return {};
				}
			}

			class MockRegistrationPublicKeyCredential extends MockPublicKeyCredential {
				readonly id = "registration-credential-e2e";
				readonly rawId = encoder.encode("registration-credential-e2e").buffer;
				readonly response = new MockAuthenticatorAttestationResponse();
			}

			const createCredential = () => new MockPublicKeyCredential();

			Object.defineProperty(window, "AuthenticatorAttestationResponse", {
				configurable: true,
				value: MockAuthenticatorAttestationResponse,
			});
			Object.defineProperty(window, "AuthenticatorAssertionResponse", {
				configurable: true,
				value: MockAuthenticatorAssertionResponse,
			});
			Object.defineProperty(window, "PublicKeyCredential", {
				configurable: true,
				value: MockPublicKeyCredential,
			});
			Object.defineProperty(window.navigator, "credentials", {
				configurable: true,
				value: {
					create: async (creationOptions: CredentialCreationOptions) => {
						window.dispatchEvent(
							new CustomEvent("asterdrive-webauthn-create", {
								detail: {
									hasPublicKey: !!creationOptions.publicKey,
								},
							}),
						);
						return supportCreate
							? new MockRegistrationPublicKeyCredential()
							: null;
					},
					get: async (requestOptions: CredentialRequestOptions) => {
						window.dispatchEvent(
							new CustomEvent("asterdrive-webauthn-get", {
								detail: {
									mediation: requestOptions.mediation ?? null,
									hasSignal: !!requestOptions.signal,
								},
							}),
						);
						if (!resolveGetManually) {
							return createCredential();
						}
						return new Promise((resolve, reject) => {
							const abort = () => {
								requestOptions.signal?.removeEventListener("abort", abort);
								reject(
									new DOMException("The operation was aborted.", "AbortError"),
								);
							};
							requestOptions.signal?.addEventListener("abort", abort, {
								once: true,
							});
							window.__asterResolvePasskeyGet = () => {
								requestOptions.signal?.removeEventListener("abort", abort);
								resolve(createCredential());
							};
						});
					},
				},
			});
		},
		{
			conditionalAvailable: options.conditionalAvailable ?? false,
			resolveGetManually: options.resolveGetManually ?? false,
			supportCreate: options.supportCreate ?? false,
		},
	);
}

export async function disablePasskeyBrowserSupport(page: Page) {
	await page.addInitScript(() => {
		Object.defineProperty(window, "PublicKeyCredential", {
			configurable: true,
			value: undefined,
		});
	});
}

export async function capturePasskeyGetCalls(page: Page) {
	await page.addInitScript(() => {
		window.__asterPasskeyGetCalls = [];
		window.__asterPasskeyCreateCalls = [];
		window.addEventListener("asterdrive-webauthn-create", (event) => {
			window.__asterPasskeyCreateCalls.push((event as CustomEvent).detail);
		});
		window.addEventListener("asterdrive-webauthn-get", (event) => {
			window.__asterPasskeyGetCalls.push((event as CustomEvent).detail);
		});
	});
}

export async function readPasskeyGetCalls(page: Page) {
	return page.evaluate(() => window.__asterPasskeyGetCalls);
}

export async function readPasskeyCreateCalls(page: Page) {
	return page.evaluate(() => window.__asterPasskeyCreateCalls);
}

export async function resolvePendingPasskeyGet(page: Page) {
	await page.evaluate(() => window.__asterResolvePasskeyGet?.());
}

export async function mockPasskeyLoginEndpoints(
	page: Page,
	options: {
		expectStartPayload?: (payload: JsonBody) => void;
	} = {},
) {
	const startPayloads: JsonBody[] = [];
	const finishPayloads: JsonBody[] = [];

	await page.route("**/api/v1/auth/passkeys/login/start", async (route) => {
		const payload = await readJsonBody(route);
		startPayloads.push(payload);
		options.expectStartPayload?.(payload);
		await route.fulfill({
			contentType: "application/json",
			status: 200,
			body: JSON.stringify(
				apiResponse({
					flow_id: TEST_FLOW_ID,
					public_key: {
						publicKey: {
							allowCredentials: [],
							challenge: TEST_CHALLENGE,
							rpId: "127.0.0.1",
							timeout: 60_000,
							userVerification: "required",
						},
					},
				}),
			),
		});
	});

	await page.route("**/api/v1/auth/passkeys/login/finish", async (route) => {
		const payload = await readJsonBody(route);
		const credential = payload.credential as {
			id?: string;
			response?: { userHandle?: string };
		};
		if (
			payload.flow_id !== TEST_FLOW_ID ||
			credential.id !== TEST_CREDENTIAL_ID ||
			credential.response?.userHandle !== TEST_USER_HANDLE
		) {
			await route.fulfill({
				contentType: "application/json",
				status: 400,
				body: JSON.stringify({
					error: "unexpected passkey login finish payload",
					payload,
				}),
			});
			return;
		}

		finishPayloads.push(payload);
		await route.fulfill({
			contentType: "application/json",
			headers: {
				"Set-Cookie": "aster_csrf=passkey-e2e-csrf; Path=/; SameSite=Lax",
			},
			status: 200,
			body: JSON.stringify(apiResponse({ expires_in: 900 })),
		});
	});

	return {
		finishPayloads,
		startPayloads,
	};
}

export async function mockPasskeyRegistrationEndpoints(page: Page) {
	const startPayloads: JsonBody[] = [];
	const finishPayloads: JsonBody[] = [];
	const patchRequests: CapturedPasskeyMutation[] = [];
	const deleteRequests: CapturedPasskeyMutation[] = [];
	const passkeys = new Map<number, MockPasskey>();
	let nextPasskeyId = 7;
	const now = "2026-05-15T16:00:00Z";

	await page.route("**/api/v1/auth/passkeys", async (route) => {
		if (route.request().method() !== "GET") {
			await route.fallback();
			return;
		}
		await route.fulfill({
			contentType: "application/json",
			status: 200,
			body: JSON.stringify(apiResponse(Array.from(passkeys.values()))),
		});
	});

	await page.route("**/api/v1/auth/passkeys/register/start", async (route) => {
		const payload = await readJsonBody(route);
		startPayloads.push(payload);
		await route.fulfill({
			contentType: "application/json",
			status: 200,
			body: JSON.stringify(
				apiResponse({
					flow_id: "registration-flow-e2e",
					public_key: {
						publicKey: {
							authenticatorSelection: {
								requireResidentKey: true,
								residentKey: "required",
								userVerification: "required",
							},
							challenge: TEST_CHALLENGE,
							pubKeyCredParams: [{ alg: -7, type: "public-key" }],
							rp: { id: "127.0.0.1", name: "AsterDrive" },
							user: {
								displayName: "admin",
								id: "YWRtaW4",
								name: "admin",
							},
						},
					},
				}),
			),
		});
	});

	await page.route("**/api/v1/auth/passkeys/register/finish", async (route) => {
		const payload = await readJsonBody(route);
		finishPayloads.push(payload);
		const name = typeof payload.name === "string" ? payload.name : "Passkey";
		const id = nextPasskeyId;
		nextPasskeyId += 1;
		const passkey: MockPasskey = {
			backup_eligible: true,
			backed_up: true,
			created_at: now,
			id,
			last_used_at: null,
			name,
			sign_count: 0,
			transports: ["internal"],
			updated_at: now,
		};
		passkeys.set(id, passkey);
		await route.fulfill({
			contentType: "application/json",
			status: 200,
			body: JSON.stringify(apiResponse(passkey)),
		});
	});

	await page.route("**/api/v1/auth/passkeys/*", async (route) => {
		const id = Number(new URL(route.request().url()).pathname.split("/").pop());
		const passkey = passkeys.get(id);
		if (!Number.isSafeInteger(id) || !passkey) {
			await route.fulfill({
				contentType: "application/json",
				status: 404,
				body: JSON.stringify({ error: "passkey not found" }),
			});
			return;
		}

		if (route.request().method() === "PATCH") {
			const payload = await readJsonBody(route);
			const name = typeof payload.name === "string" ? payload.name : "Passkey";
			patchRequests.push({
				body: payload,
				id,
				method: route.request().method(),
				url: route.request().url(),
			});
			const updated = {
				...passkey,
				name,
				updated_at: now,
			};
			passkeys.set(id, updated);
			await route.fulfill({
				contentType: "application/json",
				status: 200,
				body: JSON.stringify(apiResponse(updated)),
			});
			return;
		}
		if (route.request().method() === "DELETE") {
			deleteRequests.push({
				body: {},
				id,
				method: route.request().method(),
				url: route.request().url(),
			});
			passkeys.delete(id);
			await route.fulfill({
				contentType: "application/json",
				status: 200,
				body: JSON.stringify(apiResponse(null)),
			});
			return;
		}
		await route.fallback();
	});

	return {
		deleteRequests,
		finishPayloads,
		patchRequests,
		startPayloads,
	};
}

declare global {
	interface Window {
		__asterPasskeyCreateCalls: Array<{
			hasPublicKey: boolean;
		}>;
		__asterPasskeyGetCalls: Array<{
			hasSignal: boolean;
			mediation: CredentialMediationRequirement | null;
		}>;
		__asterResolvePasskeyGet?: () => void;
	}
}
