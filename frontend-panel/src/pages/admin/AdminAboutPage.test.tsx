import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import AdminAboutPage from "@/pages/admin/AdminAboutPage";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
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
	it("renders the injected app version, release channel, and resource links", () => {
		render(<AdminAboutPage />);

		expect(screen.getByRole("heading", { name: "about" })).toBeInTheDocument();
		expect(screen.getByRole("img", { name: "AsterDrive" })).toBeInTheDocument();
		expect(screen.getAllByText("v0.0.1-alpha.11")).toHaveLength(2);
		expect(screen.getAllByText("about_channel_alpha")).toHaveLength(2);
		expect(
			screen.getByRole("link", { name: /about_open_docs/i }),
		).toHaveAttribute("href", "https://drive.astercosm.com/");
		expect(
			screen.getByRole("link", { name: /about_view_repository/i }),
		).toHaveAttribute("href", "https://github.com/AptS-1547/AsterDrive");
	});
});
