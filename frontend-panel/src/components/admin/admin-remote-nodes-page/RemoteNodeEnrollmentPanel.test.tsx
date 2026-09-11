import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RemoteNodeEnrollmentPanel } from "@/components/admin/admin-remote-nodes-page/RemoteNodeEnrollmentPanel";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({ children }: { children: React.ReactNode }) => (
		<span>{children}</span>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		onClick,
	}: {
		children: React.ReactNode;
		onClick?: () => void;
	}) => (
		<button type="button" onClick={onClick}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

describe("RemoteNodeEnrollmentPanel", () => {
	it("covers pending, loading, error, and generated command states", () => {
		const onGenerate = vi.fn();
		const onRetry = vi.fn();
		const onCopy = vi.fn().mockResolvedValue(undefined);
		const { rerender } = render(
			<RemoteNodeEnrollmentPanel onCopy={onCopy} onGenerate={onGenerate} />,
		);
		fireEvent.click(screen.getByRole("button", { name: /generate/i }));
		expect(onGenerate).toHaveBeenCalledOnce();

		rerender(<RemoteNodeEnrollmentPanel loading onCopy={onCopy} />);
		expect(
			screen.getAllByText("remote_node_enrollment_command_generating").length,
		).toBeGreaterThan(0);

		rerender(
			<RemoteNodeEnrollmentPanel
				errorMessage="command failed"
				onCopy={onCopy}
				onRetry={onRetry}
			/>,
		);
		fireEvent.click(screen.getByRole("button", { name: /retry/i }));
		expect(onRetry).toHaveBeenCalledOnce();

		rerender(
			<RemoteNodeEnrollmentPanel
				command={{
					command: "asterdrive follower enroll --token abc",
					expires_at: "2026-09-11T00:00:00Z",
				}}
				onCopy={onCopy}
			/>,
		);
		fireEvent.click(screen.getByRole("button", { name: /copy/i }));
		expect(onCopy).toHaveBeenCalledWith(
			"asterdrive follower enroll --token abc",
		);
	});
});
