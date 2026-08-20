import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AdminTeamDetailAuditSection } from "@/components/admin/admin-team-detail/AdminTeamDetailAuditSection";

const mocks = vi.hoisted(() => ({
	exportAuditLogs: vi.fn(),
	handleApiError: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mocks.handleApiError(...args),
}));

vi.mock("@/services/adminService", () => ({
	adminTeamService: {
		exportAuditLogs: (...args: unknown[]) => mocks.exportAuditLogs(...args),
	},
}));

function renderSection() {
	return render(
		<AdminTeamDetailAuditSection
			teamId={42}
			auditCurrentPage={1}
			auditEntries={[]}
			auditLoading={false}
			auditOffset={0}
			auditTotal={0}
			auditTotalPages={1}
			nextAuditPageDisabled
			prevAuditPageDisabled
			roleLabel={(role) => role}
			setAuditOffset={vi.fn()}
		/>,
	);
}

describe("AdminTeamDetailAuditSection", () => {
	beforeEach(() => {
		mocks.exportAuditLogs.mockReset();
		mocks.handleApiError.mockReset();
	});

	it("exports the team scope and disables duplicate clicks while pending", async () => {
		let resolveExport!: () => void;
		mocks.exportAuditLogs.mockReturnValue(
			new Promise<void>((resolve) => {
				resolveExport = resolve;
			}),
		);
		renderSection();
		const button = screen.getByRole("button", { name: /core:export_csv/ });
		fireEvent.click(button);

		await waitFor(() => {
			expect(mocks.exportAuditLogs).toHaveBeenCalledWith(42);
			expect(button).toBeDisabled();
		});
		fireEvent.click(button);
		expect(mocks.exportAuditLogs).toHaveBeenCalledTimes(1);
		resolveExport();
		await waitFor(() => expect(button).not.toBeDisabled());
	});

	it("routes export failures through the shared API error handler", async () => {
		const error = new Error("export failed");
		mocks.exportAuditLogs.mockRejectedValue(error);
		renderSection();
		fireEvent.click(screen.getByRole("button", { name: /core:export_csv/ }));
		await waitFor(() =>
			expect(mocks.handleApiError).toHaveBeenCalledWith(error),
		);
	});
});
