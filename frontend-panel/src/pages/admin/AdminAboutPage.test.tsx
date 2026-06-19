import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AdminAboutPage from "@/pages/admin/AdminAboutPage";

const mockState = {
	formatDateTime: vi.fn((value: string) => `formatted:${value}`),
	getInfo: vi.fn(),
	toastInfo: vi.fn(),
};

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/lib/format", () => ({
	formatDateTime: (value: string) => mockState.formatDateTime(value),
}));

vi.mock("sonner", () => ({
	toast: {
		info: (...args: unknown[]) => mockState.toastInfo(...args),
	},
}));

vi.mock("@/services/adminService", () => ({
	adminSystemService: {
		getInfo: (...args: unknown[]) => mockState.getInfo(...args),
	},
}));

vi.mock("@/config/app", async (importOriginal) => {
	const actual = await importOriginal<typeof import("@/config/app")>();
	return {
		...actual,
		config: {
			...actual.config,
			appName: "AsterDrive",
			appVersion: "0.0.1-alpha.11",
		},
	};
});

vi.mock("@/components/layout/AdminLayout", () => ({
	AdminLayout: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/layout/AdminPageHeader", () => ({
	AdminPageHeader: ({
		title,
		description,
	}: {
		title: string;
		description?: string;
	}) => (
		<div>
			<h1>{title}</h1>
			<p>{description}</p>
		</div>
	),
}));

vi.mock("@/components/layout/AdminPageShell", () => ({
	AdminPageShell: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/layout/AdminSurface", () => ({
	AdminSurface: ({ children }: { children: React.ReactNode }) => (
		<section>{children}</section>
	),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({ children }: { children: React.ReactNode }) => (
		<span>{children}</span>
	),
}));

vi.mock("@/components/ui/card", () => ({
	Card: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
	CardContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	CardDescription: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	CardHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	CardTitle: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

describe("AdminAboutPage", () => {
	beforeEach(() => {
		mockState.formatDateTime.mockReset();
		mockState.formatDateTime.mockImplementation(
			(value: string) => `formatted:${value}`,
		);
		mockState.getInfo.mockReset();
		mockState.getInfo.mockRejectedValue(new Error("offline"));
		mockState.toastInfo.mockReset();
	});

	it("renders the injected app version, release channel, and resource links", () => {
		render(<AdminAboutPage />);

		expect(screen.getByRole("heading", { name: "about" })).toBeInTheDocument();
		expect(screen.getByRole("img", { name: "AsterDrive" })).toBeInTheDocument();
		expect(screen.getAllByText("v0.0.1-alpha.11")).toHaveLength(2);
		expect(screen.getAllByText("about_channel_alpha")).toHaveLength(2);
		expect(screen.getByText("about_build_time_unknown")).toBeInTheDocument();
		expect(
			screen.getByRole("link", { name: /about_open_docs/i }),
		).toHaveAttribute("href", "https://drive.astercosm.com/");
		expect(
			screen.getByRole("link", { name: /about_view_repository/i }),
		).toHaveAttribute("href", "https://github.com/AsterCommunity/AsterDrive");
	});

	it("loads backend version and build time from admin system info", async () => {
		mockState.getInfo.mockResolvedValueOnce({
			version: "0.3.0",
			build_time: "2026-06-13T00:00:00Z",
		});

		render(<AdminAboutPage />);

		expect(await screen.findAllByText("v0.3.0")).toHaveLength(2);
		expect(screen.queryByText("2026-06-13T00:00:00Z")).not.toBeInTheDocument();
		expect(
			screen.getByText("formatted:2026-06-13T00:00:00Z"),
		).toBeInTheDocument();
	});

	it("falls back when backend build time is invalid or cannot be formatted", async () => {
		mockState.getInfo.mockResolvedValueOnce({
			version: "0.3.0",
			build_time: "not-a-date",
		});
		const { unmount } = render(<AdminAboutPage />);

		expect(await screen.findAllByText("v0.3.0")).toHaveLength(2);
		expect(screen.getByText("about_build_time_unknown")).toBeInTheDocument();
		expect(mockState.formatDateTime).not.toHaveBeenCalled();
		unmount();

		mockState.getInfo.mockResolvedValueOnce({
			version: "0.3.1",
			build_time: "2026-06-13T00:00:00Z",
		});
		mockState.formatDateTime.mockImplementationOnce(() => {
			throw new Error("unsupported locale");
		});
		render(<AdminAboutPage />);

		expect(await screen.findAllByText("v0.3.1")).toHaveLength(2);
		expect(screen.getByText("about_build_time_unknown")).toBeInTheDocument();
	});

	it("reveals a version easter egg after five version badge clicks", () => {
		vi.spyOn(Math, "random").mockReturnValue(0);
		render(<AdminAboutPage />);

		const versionButton = screen.getByRole("button", { name: "about_version" });
		for (let i = 0; i < 4; i += 1) {
			fireEvent.click(versionButton);
		}

		expect(mockState.toastInfo).not.toHaveBeenCalled();

		fireEvent.click(versionButton);

		expect(mockState.toastInfo).toHaveBeenCalledWith(
			"ESAP-TY-0001 initialized.",
		);
	});

	it("cycles the version badge color and randomizes the top channel badge", () => {
		vi.spyOn(Math, "random").mockReturnValue(0.75);
		render(<AdminAboutPage />);

		const versionButton = screen.getByRole("button", { name: "about_version" });
		expect(versionButton).toHaveClass("bg-primary");

		fireEvent.click(versionButton);

		expect(versionButton).toHaveClass("bg-cyan-50");
		expect(screen.getByText("about_channel_rc")).toBeInTheDocument();
		expect(screen.getAllByText("about_channel_alpha")).toHaveLength(1);
		expect(mockState.toastInfo).not.toHaveBeenCalled();
	});

	it("reveals one expanded easter egg message after five clicks", () => {
		vi.spyOn(Math, "random").mockReturnValue(0.5);
		render(<AdminAboutPage />);

		const versionButton = screen.getByRole("button", { name: "about_version" });
		for (let i = 0; i < 5; i += 1) {
			fireEvent.click(versionButton);
		}

		expect(mockState.toastInfo).toHaveBeenCalledTimes(1);
		expect(mockState.toastInfo).toHaveBeenCalledWith(expect.any(String));
	});
});
