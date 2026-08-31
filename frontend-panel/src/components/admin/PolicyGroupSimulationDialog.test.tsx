import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { PolicyGroupSimulationDialog } from "@/components/admin/PolicyGroupSimulationDialog";
import type { StoragePlacementSimulationResult } from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("@/components/ui/badge", () => ({
	Badge: ({ children }: { children: ReactNode }) => <span>{children}</span>,
}));
vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <i>{name}</i>,
}));
vi.mock("@/components/ui/label", () => ({
	Label: ({ children, htmlFor }: { children: ReactNode; htmlFor?: string }) => (
		<label htmlFor={htmlFor}>{children}</label>
	),
}));
vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({ children, open }: { children: ReactNode; open: boolean }) =>
		open ? <div>{children}</div> : null,
	DialogContent: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	DialogDescription: ({ children }: { children: ReactNode }) => (
		<p>{children}</p>
	),
	DialogFooter: ({ children }: { children: ReactNode }) => (
		<footer>{children}</footer>
	),
	DialogHeader: ({ children }: { children: ReactNode }) => (
		<header>{children}</header>
	),
	DialogTitle: ({ children }: { children: ReactNode }) => <h2>{children}</h2>,
}));
vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
		...props
	}: {
		children?: ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		[key: string]: unknown;
	}) => (
		<button disabled={disabled} onClick={onClick} {...props}>
			{children}
		</button>
	),
}));
vi.mock("@/components/ui/input", () => ({
	Input: ({
		onChange,
		...props
	}: {
		onChange?: (event: { target: { value: string } }) => void;
		[key: string]: unknown;
	}) => (
		<input
			onChange={(event) =>
				onChange?.({ target: { value: event.target.value } })
			}
			{...props}
		/>
	),
}));
vi.mock("@/components/ui/select", () => ({
	Select: ({
		children,
		onValueChange,
	}: {
		children: ReactNode;
		onValueChange?: (value: string | null) => void;
	}) => (
		<div data-testid="select">
			{children}
			<button
				type="button"
				aria-label="choose-none"
				onClick={() => onValueChange?.("__none__")}
			/>
			<button
				type="button"
				aria-label="choose-policy"
				onClick={() => onValueChange?.("2")}
			/>
		</div>
	),
	SelectContent: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	SelectItem: ({ children }: { children: ReactNode }) => (
		<span>{children}</span>
	),
	SelectTrigger: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	SelectValue: ({ children }: { children?: ReactNode }) => (
		<span>{children}</span>
	),
}));

const callbacks = () => ({
	filename: vi.fn(),
	size: vi.fn(),
	folder: vi.fn(),
	mime: vi.fn(),
	open: vi.fn(),
	simulate: vi.fn(),
});

function baseProps(overrides: Record<string, unknown> = {}) {
	const cb = callbacks();
	return {
		open: true,
		error: null,
		filename: "photo.jpg",
		fileSizeMb: "2",
		folderPolicyId: "",
		group: null,
		mimeType: "image/jpeg",
		policies: [{ id: 2, name: "Archive", connector_id: "local" }],
		result: null,
		submitting: false,
		onFilenameChange: cb.filename,
		onFileSizeMbChange: cb.size,
		onFolderPolicyIdChange: cb.folder,
		onMimeTypeChange: cb.mime,
		onOpenChange: cb.open,
		onSimulate: cb.simulate,
		...overrides,
	};
}

const result = (overrides: Partial<StoragePlacementSimulationResult> = {}) =>
	({
		admitted: true,
		classification: {
			category: "image",
			compound_extension: null,
			extension: "jpg",
			file_size: 2048,
			filename: "photo.jpg",
		},
		decision: {
			policy_id: 2,
			profile_id: 4,
			revision: 9,
			rule_id: 1,
			selection_mode: "first_available",
			execution_preference: "automatic",
			folder_override: false,
			evaluated_rules: [],
			excluded_targets: [],
		},
		evaluated_rules: [{ rule_id: 1, matched: true, reason_code: null }],
		excluded_targets: [[2, "target_unavailable"]],
		rejection_code: null,
		...overrides,
	}) as StoragePlacementSimulationResult;

describe("PolicyGroupSimulationDialog", () => {
	it("updates inputs, supports folder override, cancel and simulate", () => {
		const props = baseProps({ group: { name: "Group", rules: [] } });
		render(<PolicyGroupSimulationDialog {...props} />);
		fireEvent.change(screen.getByLabelText("policy_group_simulator_filename"), {
			target: { value: "x.pdf" },
		});
		fireEvent.change(screen.getByLabelText("policy_group_simulator_size_mb"), {
			target: { value: "4" },
		});
		fireEvent.change(
			screen.getByLabelText("policy_group_simulator_mime_type"),
			{ target: { value: "application/pdf" } },
		);
		fireEvent.click(screen.getByRole("button", { name: "choose-policy" }));
		fireEvent.click(screen.getByRole("button", { name: "choose-none" }));
		fireEvent.click(screen.getByRole("button", { name: "core:cancel" }));
		fireEvent.click(
			screen.getByRole("button", { name: /policy_group_simulator_run/ }),
		);
		expect(props.onFilenameChange).toHaveBeenCalledWith("x.pdf");
		expect(props.onFileSizeMbChange).toHaveBeenCalledWith("4");
		expect(props.onMimeTypeChange).toHaveBeenCalledWith("application/pdf");
		expect(props.onFolderPolicyIdChange).toHaveBeenNthCalledWith(1, "2");
		expect(props.onFolderPolicyIdChange).toHaveBeenNthCalledWith(2, "");
		expect(props.onOpenChange).toHaveBeenCalledWith(false);
		expect(props.onSimulate).toHaveBeenCalledTimes(1);
	});

	it("renders rejected results, deduplicates excluded targets, and falls back to ids", () => {
		const props = baseProps({
			error: "invalid input",
			result: result({
				admitted: false,
				decision: null,
				evaluated_rules: [],
				excluded_targets: [
					[99, "target_disabled"],
					[99, "target_disabled"],
				],
				rejection_code: "too_large",
			}),
		});
		render(<PolicyGroupSimulationDialog {...props} />);
		expect(screen.getByText("invalid input")).toBeInTheDocument();
		expect(
			screen.getByText("policy_group_simulator_rejected"),
		).toBeInTheDocument();
		expect(
			screen.getByText("policy_group_simulator_no_evaluated_rules"),
		).toBeInTheDocument();
		expect(screen.getByText("#99")).toBeInTheDocument();
		expect(screen.getByText("too_large")).toBeInTheDocument();
	});

	it("shows selected policy details and rule evaluation states", () => {
		const group = {
			name: "Primary group",
			rules: [
				{
					id: 1,
					name: "Images",
					targets: [
						{
							policy_id: 2,
							policy: { id: 2, name: "Group policy", connector_id: "local" },
						},
					],
				},
			],
		};
		render(
			<PolicyGroupSimulationDialog
				{...baseProps({ group, result: result() })}
			/>,
		);
		expect(screen.getByText("Primary group")).toBeInTheDocument();
		expect(screen.getAllByText("Archive").length).toBeGreaterThan(0);
		expect(screen.getByText("Images")).toBeInTheDocument();
		expect(
			screen.getByText("policy_group_simulator_rule_matched"),
		).toBeInTheDocument();
	});
});
