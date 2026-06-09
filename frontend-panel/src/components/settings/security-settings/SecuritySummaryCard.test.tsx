import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SecuritySummaryCard } from "@/components/settings/security-settings/SecuritySummaryCard";
import type { MeResponse } from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) =>
			options ? `${key}:${JSON.stringify(options)}` : key,
	}),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({
		children,
		className,
		variant,
	}: {
		children: React.ReactNode;
		className?: string;
		variant?: string;
	}) => (
		<span data-variant={variant} className={className}>
			{children}
		</span>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span aria-hidden="true">{name}</span>,
}));

function user(overrides: Partial<MeResponse> = {}): MeResponse {
	return {
		access_token_expires_at: 1_800_000_000,
		created_at: "2026-01-01T00:00:00Z",
		email: "alice@example.test",
		email_verified: true,
		id: 7,
		preferences: {},
		profile: {
			avatar: {
				source: "none",
				url_1024: null,
				url_512: null,
				version: 1,
			},
			display_name: "Alice",
		},
		role: "user",
		status: "active",
		storage_quota: 1024,
		storage_used: 128,
		updated_at: "2026-01-01T00:00:00Z",
		username: "alice",
		...overrides,
	};
}

describe("SecuritySummaryCard", () => {
	it("renders verified account, email, and session count summaries", () => {
		render(<SecuritySummaryCard user={user()} sessionCount={3} />);

		expect(screen.getByText("@alice")).toBeInTheDocument();
		expect(screen.getByText("alice@example.test")).toBeInTheDocument();
		expect(screen.getByText("settings:settings_email_verified")).toHaveClass(
			"border-emerald-500/30",
		);
		expect(
			screen.getByText('settings:settings_security_session_count:{"count":3}'),
		).toBeInTheDocument();
	});

	it("renders unverified and empty user fallback values", () => {
		render(
			<SecuritySummaryCard
				user={user({ email: "", email_verified: false, username: "" })}
				sessionCount={0}
			/>,
		);

		expect(screen.getByText("@")).toBeInTheDocument();
		expect(screen.getByText("settings:settings_email_unverified")).toHaveClass(
			"border-amber-500/30",
		);
		expect(
			screen.getByText('settings:settings_security_session_count:{"count":0}'),
		).toBeInTheDocument();
	});
});
