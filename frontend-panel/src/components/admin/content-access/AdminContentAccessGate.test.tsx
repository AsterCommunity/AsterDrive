import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AdminContentAccessGate } from "./AdminContentAccessGate";

const mocks = vi.hoisted(() => ({
	status: vi.fn(),
	create: vi.fn(),
	error: vi.fn(),
}));
vi.mock("@/services/adminContentAccessService", () => ({
	adminContentAccessService: mocks,
}));
vi.mock("@/hooks/useApiError", () => ({ handleApiError: mocks.error }));
vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));
// Keep the contract test independent of the component library's portal/focus behavior.
vi.mock("@/components/ui/select", () => ({
	Select: ({
		children,
		value,
		onValueChange,
	}: {
		children: React.ReactNode;
		value: string;
		onValueChange: (value: string) => void;
	}) => (
		<div>
			<select
				aria-label="reason"
				value={value ?? ""}
				onChange={(event) => onValueChange(event.target.value)}
			>
				<option value="" />
				<option value="user_support">Support</option>
			</select>
			{children}
		</div>
	),
	SelectTrigger: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	SelectContent: () => null,
	SelectItem: () => null,
	SelectValue: () => null,
}));
function mount() {
	return render(
		<AdminContentAccessGate scopeKind="personal" targetId={7}>
			{(id, end) => (
				<div>
					<span>authorized:{id}</span>
					<button type="button" onClick={end}>
						end
					</button>
				</div>
			)}
		</AdminContentAccessGate>,
	);
}
describe("administrator content access gate", () => {
	beforeEach(() => {
		vi.resetAllMocks();
		mocks.status.mockResolvedValue({ enabled: true });
		mocks.create.mockResolvedValue({
			id: "context",
			expires_at: new Date(Date.now() + 60_000).toISOString(),
		});
	});
	afterEach(() => {
		vi.useRealTimers();
	});
	it("does not render protected content while checking status or when disabled", async () => {
		mocks.status.mockResolvedValue({ enabled: false });
		mount();
		expect(screen.queryByText(/authorized:/)).not.toBeInTheDocument();
		expect(
			await screen.findByText("content_access_disabled"),
		).toBeInTheDocument();
		expect(mocks.create).not.toHaveBeenCalled();
	});
	it("requires a reason, sends remarks only in the request body and ends access", async () => {
		mount();
		const begin = await screen.findByRole("button", {
			name: "content_access_begin",
		});
		expect(begin).toBeDisabled();
		fireEvent.change(screen.getByLabelText("reason"), {
			target: { value: "user_support" },
		});
		fireEvent.change(screen.getByLabelText("content_access_remark"), {
			target: { value: " case note " },
		});
		fireEvent.click(begin);
		expect(await screen.findByText("authorized:context")).toBeInTheDocument();
		expect(mocks.create).toHaveBeenCalledWith({
			scope_kind: "personal",
			target_id: 7,
			reason: "user_support",
			remark: "case note",
		});
		fireEvent.click(screen.getByRole("button", { name: "end" }));
		expect(await screen.findByLabelText("content_access_remark")).toHaveValue(
			"",
		);
		expect(screen.queryByText(/authorized:/)).not.toBeInTheDocument();
	});
	it("never exposes protected content on audit or status failure", async () => {
		mocks.create.mockRejectedValue(new Error("audit failed"));
		mount();
		await screen.findByRole("button", { name: "content_access_begin" });
		fireEvent.change(screen.getByLabelText("reason"), {
			target: { value: "user_support" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "content_access_begin" }),
		);
		await waitFor(() => expect(mocks.error).toHaveBeenCalledOnce());
		expect(screen.queryByText(/authorized:/)).not.toBeInTheDocument();
	});
	it("unmounts protected content when its grant expires", async () => {
		mocks.create.mockResolvedValue({
			id: "context",
			expires_at: new Date(Date.now() + 1_000).toISOString(),
		});
		mount();
		await screen.findByRole("button", { name: "content_access_begin" });
		fireEvent.change(screen.getByLabelText("reason"), {
			target: { value: "user_support" },
		});
		vi.useFakeTimers();
		await act(async () => {
			fireEvent.click(
				screen.getByRole("button", { name: "content_access_begin" }),
			);
		});
		expect(screen.getByText("authorized:context")).toBeInTheDocument();
		await act(async () => {
			vi.advanceTimersByTime(1_001);
		});
		expect(screen.queryByText(/authorized:/)).not.toBeInTheDocument();
	});
});
