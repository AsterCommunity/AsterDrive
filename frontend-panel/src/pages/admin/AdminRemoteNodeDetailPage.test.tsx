import { render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AdminRemoteNodeDetailPage from "@/pages/admin/AdminRemoteNodeDetailPage";

const state = vi.hoisted(() => ({
	params: { nodeId: "13", section: "overview" as string | undefined },
	navigate: vi.fn(),
	remoteNodePageProps: null as null | {
		pageTab?: string;
		onPageTabChange?: (value: string) => void;
	},
}));

vi.mock("react-router-dom", () => ({
	Navigate: ({ to }: { to: string }) => <div data-testid="navigate">{to}</div>,
	useLocation: () => ({ state: null }),
	useNavigate: () => state.navigate,
	useParams: () => state.params,
}));

vi.mock("@/hooks/usePageTitle", () => ({ usePageTitle: vi.fn() }));
vi.mock("@/pages/admin/useAdminRemoteNodeDetailController", () => ({
	useAdminRemoteNodeDetailController: () => ({
		baseUrlValidationMessage: null,
		copyToClipboard: vi.fn(),
		enrollmentCommand: null,
		enrollmentCommandError: null,
		enrollmentCommandLoading: false,
		generateEnrollmentCommand: vi.fn(),
		navigateBack: vi.fn(),
		node: {
			id: 13,
			name: "Edge Alpha",
			base_url: "https://edge.example.com",
			is_enabled: true,
			enrollment_status: "completed",
			last_probe_error: "",
			last_probe_at: null,
			capabilities: null,
			created_at: "",
			updated_at: "",
			transport_mode: "direct",
		},
		form: {
			name: "Edge Alpha",
			base_url: "https://edge.example.com",
			is_enabled: true,
			transport_mode: "direct",
		},
		loading: false,
		remoteStorageTargetConnectorDescriptors: [],
		remoteStorageTargetConnectorDescriptorsError: null,
		remoteStorageTargetConnectorDescriptorsLoading: false,
		remoteStorageTargets: [],
		remoteStorageTargetsError: null,
		remoteStorageTargetsLoading: false,
		createRemoteStorageTarget: vi.fn(),
		deleteRemoteStorageTarget: vi.fn(),
		updateRemoteStorageTarget: vi.fn(),
		runConnectionTest: vi.fn(),
		setField: vi.fn(),
		submit: vi.fn(),
		submitting: false,
		t: (key: string) => key,
	}),
}));
vi.mock("@/components/admin/admin-remote-nodes-page/RemoteNodePage", () => ({
	RemoteNodePage: (props: {
		pageTab?: string;
		onPageTabChange?: (value: string) => void;
	}) => {
		state.remoteNodePageProps = props;
		return <div data-testid="remote-node-page">{props.pageTab}</div>;
	},
}));
vi.mock(
	"@/components/admin/admin-remote-nodes-page/RemoteNodeEnrollmentPanel",
	() => ({
		RemoteNodeEnrollmentPanel: () => null,
	}),
);
vi.mock("@/components/layout/AdminLayout", () => ({
	AdminLayout: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));
vi.mock("@/components/layout/AdminPageShell", () => ({
	AdminPageShell: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
}));
vi.mock("@/components/ui/button", () => ({ Button: () => null }));
vi.mock("@/components/ui/icon", () => ({ Icon: () => null }));

describe("AdminRemoteNodeDetailPage", () => {
	beforeEach(() => {
		state.navigate.mockReset();
		state.remoteNodePageProps = null;
		state.params = { nodeId: "13", section: "overview" };
	});

	it("redirects the legacy detail URL to overview", () => {
		state.params.section = undefined;
		render(<AdminRemoteNodeDetailPage />);
		expect(screen.getByTestId("navigate")).toHaveTextContent(
			"/admin/remote-nodes/13/overview",
		);
	});

	it("redirects unknown sections to overview", () => {
		state.params.section = "unknown";
		render(<AdminRemoteNodeDetailPage />);
		expect(screen.getByTestId("navigate")).toHaveTextContent(
			"/admin/remote-nodes/13/overview",
		);
	});

	it("uses the URL section and navigates between detail tabs", () => {
		state.params.section = "storage-targets";
		render(<AdminRemoteNodeDetailPage />);
		expect(screen.getByTestId("remote-node-page")).toHaveTextContent(
			"storage-targets",
		);
		state.remoteNodePageProps?.onPageTabChange?.("overview");
		expect(state.navigate).toHaveBeenCalledWith(
			"/admin/remote-nodes/13/overview",
			{ viewTransition: false },
		);
	});
});
