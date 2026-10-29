import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkspaceActivity } from "@/types/api";
import { WorkspaceActivityFeed } from "./WorkspaceActivityFeed";

const mock = vi.hoisted(() => ({
	state: {
		items: [] as WorkspaceActivity[],
		next: null as number | null,
		loading: false,
		failed: false,
		retry: vi.fn(),
		page: 1,
		previousPage: vi.fn(),
		nextPage: vi.fn(),
	},
	exportCsv: vi.fn(),
	handleError: vi.fn(),
	canExport: false,
	loadLocales: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key, i18n: { language: "en" } }),
}));
vi.mock("@/i18n", () => ({ ensureI18nNamespaces: mock.loadLocales }));
vi.mock("@/hooks/useWorkspaceActivities", () => ({
	useWorkspaceActivities: () => mock.state,
}));
vi.mock("@/hooks/useApiError", () => ({ handleApiError: mock.handleError }));
vi.mock("@/services/workspaceActivityService", () => ({
	createWorkspaceActivityResource: () => ({
		key: "team:1",
		list: vi.fn(),
		export: mock.canExport ? mock.exportCsv : undefined,
	}),
}));
vi.mock("@/lib/audit", () => ({
	formatAuditAction: (_t: unknown, action: string) => action,
}));
vi.mock("@/lib/format", () => ({ formatDateAbsolute: (time: string) => time }));
const target = { kind: "team", teamId: 1 } as const;
const event: WorkspaceActivity = {
	id: 1,
	event_id: "event-1",
	operation_id: "op-1",
	action: "file_rename",
	actor: { user_id: 9, name: "Alice" },
	entity_type: "file",
	entity_id: 2,
	entity_name: "new.txt",
	source: "rest",
	details: {
		previous_name: "old.txt",
		next_name: "new.txt",
		secret: "must-not-render",
	},
	created_at: "2026-10-30T00:00:00Z",
};
describe("WorkspaceActivityFeed", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		Object.assign(mock.state, {
			items: [],
			next: null,
			loading: false,
			failed: false,
			page: 1,
		});
		mock.canExport = false;
	});
	it("loads only shared audit messages for an opened activity feed", () => {
		render(<WorkspaceActivityFeed target={target} />);
		expect(mock.loadLocales).toHaveBeenCalledWith(
			[{ namespace: "admin", parts: ["audit"] }],
			"en",
		);
	});
	it("does not load deferred audit messages while the feed is closed", () => {
		render(<WorkspaceActivityFeed target={target} enabled={false} />);
		expect(mock.loadLocales).not.toHaveBeenCalled();
	});
	it("renders a readable historical rename without exposing arbitrary details", () => {
		mock.state.items = [event];
		render(<WorkspaceActivityFeed target={target} />);
		expect(screen.getByText("Alice")).toBeInTheDocument();
		expect(screen.getByText("old.txt → new.txt")).toBeInTheDocument();
		expect(screen.queryByText("must-not-render")).not.toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "activity_export" }),
		).not.toBeInTheDocument();
	});
	it("distinguishes unavailable history from an empty history and offers retry", () => {
		mock.state.failed = true;
		render(<WorkspaceActivityFeed target={target} />);
		expect(screen.getByRole("alert")).toHaveTextContent("activity_load_failed");
		expect(screen.queryByText("activity_empty")).not.toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "activity_retry" }));
		expect(mock.state.retry).toHaveBeenCalledOnce();
	});
	it("shows loading and prevents a concurrent next-page request", () => {
		mock.state.loading = true;
		mock.state.next = 1;
		mock.state.items = [event];
		render(<WorkspaceActivityFeed target={target} />);
		expect(screen.getByRole("status")).toHaveTextContent("activity_loading");
		expect(
			screen.getByRole("button", { name: "activity_next" }),
		).toBeDisabled();
		expect(
			screen.getByRole("button", { name: "activity_previous" }),
		).toBeDisabled();
	});
	it("reports export failures and re-enables the export action", async () => {
		mock.canExport = true;
		const failure = new Error("network");
		mock.exportCsv.mockRejectedValueOnce(failure);
		render(<WorkspaceActivityFeed target={target} />);
		const button = screen.getByRole("button", { name: "activity_export" });
		fireEvent.click(button);
		await waitFor(() => expect(mock.handleError).toHaveBeenCalledWith(failure));
		expect(button).not.toBeDisabled();
	});
	it("navigates one page at a time and disables navigation at the boundaries", () => {
		mock.state.items = [event];
		mock.state.next = 1;
		const { rerender } = render(<WorkspaceActivityFeed target={target} />);
		expect(
			screen.getByRole("button", { name: "activity_previous" }),
		).toBeDisabled();
		fireEvent.click(screen.getByRole("button", { name: "activity_next" }));
		expect(mock.state.nextPage).toHaveBeenCalledOnce();
		mock.state.page = 2;
		mock.state.next = null;
		rerender(<WorkspaceActivityFeed target={target} />);
		expect(
			screen.getByRole("button", { name: "activity_next" }),
		).toBeDisabled();
		fireEvent.click(screen.getByRole("button", { name: "activity_previous" }));
		expect(mock.state.previousPage).toHaveBeenCalledOnce();
	});
});
