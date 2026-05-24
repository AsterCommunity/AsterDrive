import { afterEach, describe, expect, it, vi } from "vitest";
import {
	actionReducer,
	createSetupState,
	downloadRecoveryCodes,
	EMPTY_ACTION_STATE,
	formatRecoveryCodesFile,
	setupReducer,
	stepIndex,
} from "./mfaTypes";

describe("mfaTypes", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.restoreAllMocks();
	});

	it("normalizes setup state transitions and one-time codes", () => {
		const setup = {
			expires_in: 300,
			flow_token: "flow-1",
			otpauth_uri: "otpauth://totp/AsterDrive:alice",
			secret: "SECRET123",
		};

		let state = createSetupState();
		expect(state.step).toBeNull();
		expect(state.recoveryCodes).toEqual([]);

		state = setupReducer(state, { type: "intro" });
		expect(state.step).toBe("intro");

		state = setupReducer(state, { type: "start_busy" });
		expect(state).toMatchObject({ busy: true, step: "intro" });

		state = setupReducer(state, { type: "start_success", setup });
		expect(state).toMatchObject({ busy: false, setup, step: "scan" });

		state = setupReducer(state, { type: "set_code", code: " 12a34 5678 " });
		expect(state.code).toBe("123456");

		state = setupReducer(state, { type: "set_name", name: "My phone" });
		state = setupReducer(state, { type: "toggle_secret" });
		expect(state.name).toBe("My phone");
		expect(state.showSecret).toBe(true);

		state = setupReducer(state, { type: "finish_busy" });
		expect(state.finishBusy).toBe(true);

		state = setupReducer(state, {
			type: "finish_success",
			recoveryCodes: ["AAAA-BBBB", "CCCC-DDDD"],
		});
		expect(state).toMatchObject({
			code: "",
			finishBusy: false,
			recoveryCodes: ["AAAA-BBBB", "CCCC-DDDD"],
			recoveryConfirmed: false,
			setup: null,
			step: "recovery",
		});

		state = setupReducer(state, { type: "toggle_recovery_confirmed" });
		expect(state.recoveryConfirmed).toBe(true);

		state = setupReducer(state, { type: "reset" });
		expect(state).toEqual(createSetupState());
	});

	it("keeps busy flags recoverable after setup failures", () => {
		const state = setupReducer(
			createSetupState({ finishBusy: true, step: "verify" }),
			{ type: "finish_error" },
		);
		expect(state).toMatchObject({ finishBusy: false, step: "verify" });

		const startErrorState = setupReducer(
			createSetupState({ busy: true, step: "intro" }),
			{ type: "start_error" },
		);
		expect(startErrorState).toMatchObject({ busy: false, step: "intro" });
	});

	it("tracks sensitive MFA action form state", () => {
		let state = actionReducer(EMPTY_ACTION_STATE, {
			type: "open",
			kind: "disable",
		});
		expect(state).toEqual({ busy: false, code: "", kind: "disable" });

		state = actionReducer(state, { type: "code", code: "123456" });
		state = actionReducer(state, { type: "busy", busy: true });
		expect(state).toEqual({ busy: true, code: "123456", kind: "disable" });

		state = actionReducer(state, { type: "reset" });
		expect(state).toBe(EMPTY_ACTION_STATE);
	});

	it("formats and downloads recovery codes as a text file", () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-05-24T08:30:00.000Z"));

		const file = formatRecoveryCodesFile(["AAAA-BBBB", "CCCC-DDDD"]);
		expect(file).toContain("AsterDrive MFA recovery codes");
		expect(file).toContain("Generated at: 2026-05-24T08:30:00.000Z");
		expect(file).toContain("AAAA-BBBB\nCCCC-DDDD");

		const appendSpy = vi.spyOn(document.body, "append");
		const clickSpy = vi
			.spyOn(HTMLAnchorElement.prototype, "click")
			.mockImplementation(() => undefined);
		const createObjectUrlSpy = vi
			.spyOn(URL, "createObjectURL")
			.mockReturnValue("blob:recovery-codes");
		const revokeObjectUrlSpy = vi
			.spyOn(URL, "revokeObjectURL")
			.mockImplementation(() => undefined);

		downloadRecoveryCodes(file);

		expect(createObjectUrlSpy).toHaveBeenCalledWith(expect.any(Blob));
		expect(appendSpy).toHaveBeenCalledWith(expect.any(HTMLAnchorElement));
		const link = appendSpy.mock.calls[0]?.[0] as HTMLAnchorElement;
		expect(link.href).toBe("blob:recovery-codes");
		expect(link.download).toBe("asterdrive-mfa-recovery-codes.txt");
		expect(clickSpy).toHaveBeenCalledTimes(1);
		expect(revokeObjectUrlSpy).toHaveBeenCalledWith("blob:recovery-codes");
	});

	it("returns the configured step index", () => {
		expect(stepIndex("intro")).toBe(0);
		expect(stepIndex("scan")).toBe(1);
		expect(stepIndex("verify")).toBe(2);
		expect(stepIndex("recovery")).toBe(3);
	});
});
