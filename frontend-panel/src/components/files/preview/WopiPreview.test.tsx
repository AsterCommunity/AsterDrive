import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WopiPreview } from "@/components/files/preview/WopiPreview";
import type { WopiLaunchSession } from "@/types/api";
import { createWopiSessionResource } from "./wopiSessionResource";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) =>
			options?.label ? `${key}:${options.label}` : key,
	}),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

const validSession = (
	overrides: Partial<WopiLaunchSession> = {},
): WopiLaunchSession => ({
	access_token: "token-1",
	access_token_ttl: 3_600,
	action_url: "https://office.example.com/wopi/edit",
	form_fields: {
		user_id: "42",
	},
	mode: "iframe",
	...overrides,
});

describe("WopiPreview", () => {
	beforeEach(() => {
		vi.spyOn(window, "requestAnimationFrame").mockImplementation((callback) => {
			callback(0);
			return 1;
		});
		vi.spyOn(window, "cancelAnimationFrame").mockImplementation(() => {});
		vi.spyOn(window, "open").mockReturnValue(null);
		vi.spyOn(HTMLFormElement.prototype, "submit").mockImplementation(() => {});
	});

	function renderPreview({
		createSession,
		label = "Office",
		rawConfig = null,
	}: {
		createSession: () => Promise<WopiLaunchSession>;
		label?: string;
		rawConfig?: Record<string, unknown> | null;
	}) {
		return render(
			<WopiPreview
				label={label}
				rawConfig={rawConfig}
				sessionResource={createWopiSessionResource(createSession)}
			/>,
		);
	}

	it("shows a loading state while creating the WOPI session", () => {
		renderPreview({
			createSession: () => new Promise(() => undefined),
		});

		expect(screen.getByRole("status")).toHaveTextContent("wopi_loading:Office");
	});

	it("renders the unavailable state for invalid URLs or rejected sessions", async () => {
		const firstView = renderPreview({
			createSession: async () => validSession({ action_url: "ftp://bad" }),
		});

		expect(await screen.findByText("wopi_unavailable")).toBeInTheDocument();

		firstView.unmount();
		renderPreview({
			createSession: async () => {
				throw new Error("session failed");
			},
		});

		expect(await screen.findByText("wopi_unavailable")).toBeInTheDocument();
	});

	it("submits valid iframe sessions into the named preview frame", async () => {
		renderPreview({
			createSession: async () => validSession(),
		});

		const frame = await screen.findByTitle("Office");
		await waitFor(() => {
			expect(HTMLFormElement.prototype.submit).toHaveBeenCalledTimes(1);
		});
		const form = document.querySelector("form");
		if (!(form instanceof HTMLFormElement)) {
			throw new Error("WOPI form not found");
		}

		expect(form.action).toBe("https://office.example.com/wopi/edit");
		expect(form.target).toBe(frame.getAttribute("name"));
		expect(frame).toHaveAttribute(
			"sandbox",
			"allow-scripts allow-forms allow-popups allow-downloads allow-same-origin allow-top-navigation allow-popups-to-escape-sandbox",
		);
		expect(frame).toHaveAttribute(
			"allow",
			"autoplay; fullscreen; picture-in-picture; clipboard-read 'src'; clipboard-write 'src'",
		);
		expect(frame).toHaveAttribute("referrerpolicy", "no-referrer");
		expect(form).toHaveFormValues({
			access_token: "token-1",
			access_token_ttl: "3600",
			user_id: "42",
		});

		fireEvent.load(frame);

		await waitFor(() => {
			expect(screen.queryByRole("status")).not.toBeInTheDocument();
		});
	});

	it("uses raw config for new-tab mode and posts to a generated external target", async () => {
		renderPreview({
			createSession: async () => validSession({ mode: undefined }),
			rawConfig: { mode: "new_tab" },
		});

		expect(await screen.findByText("Office")).toBeInTheDocument();
		fireEvent.click(
			screen.getByRole("button", {
				name: /wopi_open:Office/,
			}),
		);

		expect(window.open).toHaveBeenCalledWith(
			"",
			expect.stringMatching(/^wopi-external-/),
			"noopener,noreferrer",
		);
		expect(HTMLFormElement.prototype.submit).toHaveBeenCalledTimes(1);
		const form = document.querySelector("form");
		if (!(form instanceof HTMLFormElement)) {
			throw new Error("WOPI form not found");
		}
		expect(form.target).toMatch(/^wopi-external-/);
	});

	it("lets the session mode override a raw iframe config", async () => {
		renderPreview({
			createSession: async () => validSession({ mode: "new_tab" }),
			label: "Collabora",
			rawConfig: { mode: "iframe" },
		});

		expect(await screen.findByText("Collabora")).toBeInTheDocument();
		expect(screen.queryByTitle("Collabora")).not.toBeInTheDocument();
	});
});
