import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { TFunction } from "i18next";
import { describe, expect, it, vi } from "vitest";
import {
	formatLastChecked,
	getRemoteNodeEnrollmentStatusLabel,
	getRemoteNodeEnrollmentStatusTone,
	getRemoteNodeStatusLabel,
	getRemoteNodeStatusTone,
	getRemoteNodeTransportBadge,
	getRemoteNodeTransportLabel,
	getRemoteNodeTransportTone,
	getRemoteNodeTunnelLabel,
	getRemoteNodeTunnelTone,
	hasCompletedRemoteNodeEnrollment,
	TestConnectionButton,
} from "@/components/admin/admin-remote-nodes-page/shared";
import { getActiveDisplayTimeZone } from "@/stores/displayTimeZoneStore";
import type { RemoteNodeInfo } from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => (
		<span aria-hidden="true" data-testid={name} />
	),
}));

const t = ((key: string) => key) as unknown as TFunction;

describe("admin remote nodes shared helpers", () => {
	const node = (overrides: Partial<RemoteNodeInfo> = {}) =>
		({
			is_enabled: true,
			last_probe_at: "2026-05-29T08:00:00Z",
			last_probe_error: "",
			transport_mode: "direct",
			tunnel: {
				status: "offline",
				runtime_error: "",
				last_handshake_at: null,
			},
			enrollment_status: "not_started",
			...overrides,
		}) as RemoteNodeInfo;

	it("updates the connection test icon for failed and successful results", async () => {
		const results = [false, true];

		render(
			<TestConnectionButton onTest={async () => results.shift() ?? true} />,
		);

		expect(screen.getByTestId("WifiHigh")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "test_connection" }));

		await waitFor(() => {
			expect(screen.getByTestId("WifiX")).toBeInTheDocument();
		});

		fireEvent.click(screen.getByRole("button", { name: "test_connection" }));

		await waitFor(() => {
			expect(screen.getByTestId("Check")).toBeInTheDocument();
		});
	});

	it("maps remote node health states to labels and tones", () => {
		expect(getRemoteNodeStatusLabel(t, node({ is_enabled: false }))).toBe(
			"remote_node_status_disabled",
		);
		expect(getRemoteNodeStatusTone(node({ is_enabled: false }))).toContain(
			"border-slate",
		);
		expect(getRemoteNodeStatusLabel(t, node({ last_probe_at: null }))).toBe(
			"remote_node_status_pending",
		);
		expect(getRemoteNodeStatusTone(node({ last_probe_at: null }))).toContain(
			"border-blue",
		);
		expect(
			getRemoteNodeStatusLabel(t, node({ last_probe_error: "timeout" })),
		).toBe("remote_node_status_degraded");
		expect(
			getRemoteNodeStatusTone(node({ last_probe_error: "timeout" })),
		).toContain("border-amber");
		expect(getRemoteNodeStatusLabel(t, node())).toBe(
			"remote_node_status_enabled",
		);
		expect(getRemoteNodeStatusTone(node())).toContain("border-emerald");
	});

	it("formats the last checked timestamp in the browser locale and timezone", () => {
		const value = "2026-04-21T06:45:30Z";

		expect(formatLastChecked(t, value)).toBe(
			new Date(value).toLocaleString(undefined, {
				hour12: false,
				hourCycle: "h23",
				timeZone: getActiveDisplayTimeZone(),
			}),
		);
	});

	it("falls back to the never checked label when no timestamp exists", () => {
		expect(formatLastChecked(t, null)).toBe("remote_node_never_checked");
		expect(formatLastChecked(t, undefined)).toBe("remote_node_never_checked");
	});

	it("maps enrollment statuses to dedicated labels", () => {
		expect(getRemoteNodeEnrollmentStatusLabel(t, "not_started")).toBe(
			"remote_node_enrollment_status_not_started",
		);
		expect(getRemoteNodeEnrollmentStatusLabel(t, "pending")).toBe(
			"remote_node_enrollment_status_pending",
		);
		expect(getRemoteNodeEnrollmentStatusLabel(t, "redeemed")).toBe(
			"remote_node_enrollment_status_redeemed",
		);
		expect(getRemoteNodeEnrollmentStatusLabel(t, "completed")).toBe(
			"remote_node_enrollment_status_completed",
		);
		expect(getRemoteNodeEnrollmentStatusLabel(t, "expired")).toBe(
			"remote_node_enrollment_status_expired",
		);
	});

	it("maps enrollment statuses to dedicated tones", () => {
		expect(getRemoteNodeEnrollmentStatusTone("not_started")).toContain(
			"border-slate",
		);
		expect(getRemoteNodeEnrollmentStatusTone("pending")).toContain(
			"border-blue",
		);
		expect(getRemoteNodeEnrollmentStatusTone("redeemed")).toContain(
			"border-cyan",
		);
		expect(getRemoteNodeEnrollmentStatusTone("completed")).toContain(
			"border-emerald",
		);
		expect(getRemoteNodeEnrollmentStatusTone("expired")).toContain(
			"border-amber",
		);
	});

	it("maps transport modes to dedicated labels", () => {
		expect(getRemoteNodeTransportLabel(t, "direct")).toBe(
			"remote_node_transport_direct",
		);
		expect(getRemoteNodeTransportLabel(t, "reverse_tunnel")).toBe(
			"remote_node_transport_reverse_tunnel",
		);
		expect(getRemoteNodeTransportLabel(t, "auto")).toBe(
			"remote_node_transport_auto",
		);
	});

	it("maps transport modes to dedicated tones", () => {
		expect(getRemoteNodeTransportTone("direct")).toContain("border-blue");
		expect(getRemoteNodeTransportTone("reverse_tunnel")).toContain(
			"border-cyan",
		);
		expect(getRemoteNodeTransportTone("auto")).toContain("border-violet");
	});

	it("marks reverse tunnel as a test transport", () => {
		expect(getRemoteNodeTransportBadge(t, "direct")).toBeNull();
		expect(getRemoteNodeTransportBadge(t, "reverse_tunnel")).toBe(
			"remote_node_transport_test_badge",
		);
		expect(getRemoteNodeTransportBadge(t, "auto")).toBeNull();
	});

	it("maps tunnel status from node transport state", () => {
		expect(
			getRemoteNodeTunnelLabel(t, {
				transport_mode: "direct",
				tunnel: {
					status: "online",
					runtime_error: "",
					last_handshake_at: "2026-05-29T08:00:00Z",
				},
			} as RemoteNodeInfo),
		).toBe("remote_node_tunnel_not_used");
		expect(
			getRemoteNodeTunnelLabel(t, {
				transport_mode: "reverse_tunnel",
				tunnel: {
					status: "online",
					runtime_error: "",
					last_handshake_at: "2026-05-29T08:00:00Z",
				},
			} as RemoteNodeInfo),
		).toBe("remote_node_tunnel_online");
		expect(
			getRemoteNodeTunnelLabel(t, {
				transport_mode: "auto",
				tunnel: {
					status: "offline",
					runtime_error: "poll timeout",
					last_handshake_at: null,
				},
			} as RemoteNodeInfo),
		).toBe("remote_node_tunnel_offline");
	});

	it("maps tunnel status to dedicated tones", () => {
		expect(
			getRemoteNodeTunnelTone(
				node({
					transport_mode: "direct",
					tunnel: {
						status: "online",
						runtime_error: "",
						last_handshake_at: "2026-05-29T08:00:00Z",
					},
				}),
			),
		).toContain("border-slate");
		expect(
			getRemoteNodeTunnelTone(
				node({
					transport_mode: "reverse_tunnel",
					tunnel: {
						status: "online",
						runtime_error: "",
						last_handshake_at: "2026-05-29T08:00:00Z",
					},
				}),
			),
		).toContain("border-emerald");
		expect(
			getRemoteNodeTunnelTone(
				node({
					transport_mode: "auto",
					tunnel: {
						status: "offline",
						runtime_error: "poll timeout",
						last_handshake_at: null,
					},
				}),
			),
		).toContain("border-amber");
	});

	it("detects completed enrollment separately from health status", () => {
		const node = {
			enrollment_status: "completed",
		} as RemoteNodeInfo;

		expect(hasCompletedRemoteNodeEnrollment(node)).toBe(true);
		expect(
			hasCompletedRemoteNodeEnrollment({
				enrollment_status: "redeemed",
			} as RemoteNodeInfo),
		).toBe(false);
	});
});
