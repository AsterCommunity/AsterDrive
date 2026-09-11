import { cleanup, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AdminExternalAuthCreatePage from "@/pages/admin/AdminExternalAuthCreatePage";
import AdminExternalAuthDetailPage from "@/pages/admin/AdminExternalAuthDetailPage";
import AdminPolicyCreatePage from "@/pages/admin/AdminPolicyCreatePage";
import AdminPolicyDetailPage from "@/pages/admin/AdminPolicyDetailPage";
import AdminRemoteNodeCreatePage from "@/pages/admin/AdminRemoteNodeCreatePage";
import AdminRemoteNodeDetailPage from "@/pages/admin/AdminRemoteNodeDetailPage";

const state = vi.hoisted(() => ({
	params: { policyId: "12", providerId: "9", nodeId: "7", section: "overview" },
	createAllowed: true,
	createDirty: false,
	blocker: {
		proceed: vi.fn(),
		reset: vi.fn(),
		state: "unblocked" as "blocked" | "proceeding" | "unblocked",
	},
	confirmProps: null as null | {
		onConfirm: () => void;
		onOpenChange: (open: boolean) => void;
	},
	navigateBack: vi.fn(),
	remoteNodePageProps: null as null | {
		onRunConnectionTest: () => Promise<boolean>;
		onSubmit: () => void;
	},
	setField: vi.fn(),
	submit: vi.fn(),
}));

vi.mock("react-router-dom", () => ({
	Navigate: ({ to }: { to: string }) => <div data-testid="navigate">{to}</div>,
	useBlocker: () => state.blocker,
	useParams: () => state.params,
}));

vi.mock("@/pages/admin/AdminPoliciesPage", () => ({
	default: (props: { variant?: string; detailPolicyId?: number }) => (
		<div data-testid="policies-page">
			{props.variant}:{props.detailPolicyId ?? ""}
		</div>
	),
}));

vi.mock("@/pages/admin/AdminExternalAuthPage", () => ({
	default: (props: { variant?: string; providerId?: number }) => (
		<div data-testid="external-auth-page">
			{props.variant}:{props.providerId ?? ""}
		</div>
	),
}));

vi.mock("@/pages/admin/useAdminRemoteNodeCreateController", () => ({
	useAdminRemoteNodeCreateController: () => ({
		createAllowed: state.createAllowed,
		baseUrlValidationMessage: null,
		form: {},
		navigateBack: state.navigateBack,
		setField: state.setField,
		submit: state.submit,
		t: (key: string) => key,
		submitting: false,
		createDirty: state.createDirty,
	}),
}));

vi.mock("@/pages/admin/useAdminRemoteNodeDetailController", () => ({
	useAdminRemoteNodeDetailController: () => ({
		loading: true,
		t: (key: string) => key,
	}),
}));

vi.mock("@/components/admin/admin-remote-nodes-page/RemoteNodePage", () => ({
	RemoteNodePage: (props: {
		onRunConnectionTest: () => Promise<boolean>;
		onSubmit: () => void;
	}) => {
		state.remoteNodePageProps = props;
		return <div data-testid="remote-node-page" />;
	},
}));
vi.mock(
	"@/components/admin/admin-remote-nodes-page/RemoteNodeEnrollmentPanel",
	() => ({
		RemoteNodeEnrollmentPanel: () => null,
	}),
);
vi.mock("@/components/common/ConfirmDialog", () => ({
	ConfirmDialog: (props: {
		onConfirm: () => void;
		onOpenChange: (open: boolean) => void;
		open: boolean;
	}) => {
		state.confirmProps = props;
		return props.open ? <div data-testid="confirm-dialog" /> : null;
	},
}));
vi.mock("@/components/layout/AdminLayout", () => ({
	AdminLayout: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));
vi.mock("@/components/layout/AdminPageShell", () => ({
	AdminPageShell: ({ children }: { children: React.ReactNode }) => (
		<>{children}</>
	),
}));
vi.mock("@/components/ui/button", () => ({ Button: () => null }));
vi.mock("@/components/ui/icon", () => ({ Icon: () => null }));
vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("@/hooks/usePageTitle", () => ({ usePageTitle: vi.fn() }));

describe("admin detail/create route wrappers", () => {
	beforeEach(() => {
		cleanup();
		state.params = {
			policyId: "12",
			providerId: "9",
			nodeId: "7",
			section: "overview",
		};
		state.createAllowed = true;
		state.createDirty = false;
		state.blocker.proceed.mockReset();
		state.blocker.reset.mockReset();
		state.blocker.state = "unblocked";
		state.confirmProps = null;
		state.navigateBack.mockReset();
		state.remoteNodePageProps = null;
		state.setField.mockReset();
		state.submit.mockReset();
	});

	it("passes create and valid detail routes to their page controllers", () => {
		const { unmount } = render(<AdminPolicyCreatePage />);
		expect(screen.getByTestId("policies-page")).toHaveTextContent("create:");
		unmount();

		render(<AdminPolicyDetailPage />);
		expect(screen.getByTestId("policies-page")).toHaveTextContent("detail:12");
		cleanup();

		render(<AdminExternalAuthCreatePage />);
		expect(screen.getByTestId("external-auth-page")).toHaveTextContent(
			"create:",
		);
		cleanup();

		const last = render(<AdminExternalAuthDetailPage />);
		expect(screen.getByTestId("external-auth-page")).toHaveTextContent(
			"detail:9",
		);
		last.unmount();

		render(<AdminRemoteNodeCreatePage />);
		expect(screen.getByTestId("remote-node-page")).toBeInTheDocument();
		expect(state.remoteNodePageProps?.onRunConnectionTest()).resolves.toBe(
			false,
		);
		state.remoteNodePageProps?.onSubmit();
		expect(state.submit).toHaveBeenCalledOnce();
	});

	it("guards dirty remote-node creation navigation", () => {
		vi.useFakeTimers();
		state.createDirty = true;
		state.blocker.state = "blocked";
		render(<AdminRemoteNodeCreatePage />);
		expect(screen.getByTestId("confirm-dialog")).toBeInTheDocument();
		const beforeUnload = new Event("beforeunload", { cancelable: true });
		window.dispatchEvent(beforeUnload);
		expect(beforeUnload.defaultPrevented).toBe(true);
		state.confirmProps?.onOpenChange(false);
		vi.runAllTimers();
		expect(state.blocker.reset).toHaveBeenCalledOnce();
		state.confirmProps?.onConfirm();
		expect(state.blocker.proceed).toHaveBeenCalledOnce();
		vi.useRealTimers();
	});

	it("redirects invalid detail identifiers", () => {
		state.params = { policyId: "0", providerId: "abc" };
		const first = render(<AdminPolicyDetailPage />);
		expect(screen.getByTestId("navigate")).toHaveTextContent("/admin/policies");
		first.unmount();

		const second = render(<AdminExternalAuthDetailPage />);
		expect(screen.getByTestId("navigate")).toHaveTextContent(
			"/admin/external-auth",
		);
		second.unmount();

		state.params.nodeId = "0";
		const third = render(<AdminRemoteNodeDetailPage />);
		expect(screen.getByTestId("navigate")).toHaveTextContent(
			"/admin/remote-nodes",
		);
		third.unmount();

		state.createAllowed = false;
		render(<AdminRemoteNodeCreatePage />);
		expect(screen.getByTestId("navigate")).toHaveTextContent(
			"/admin/remote-nodes",
		);
	});
});
