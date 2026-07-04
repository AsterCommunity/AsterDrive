import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { emptyForm } from "@/components/admin/storage-policy-dialog/formTypes";
import { RemoteNodeField } from "@/components/admin/storage-policy-dialog/StoragePolicyRemoteFields";
import type { RemoteNodeInfo, RemoteStorageTargetInfo } from "@/types/api";

vi.mock("@/components/ui/label", () => ({
	Label: ({ children, htmlFor }: { children: ReactNode; htmlFor?: string }) => (
		<label htmlFor={htmlFor}>{children}</label>
	),
}));

vi.mock("@/components/ui/select", () => {
	const { createContext, useContext } =
		require("react") as typeof import("react");

	const SelectContext = createContext<{
		disabled?: boolean;
		onValueChange?: (value: string) => void;
	}>({});

	return {
		Select: ({
			children,
			disabled,
			onValueChange,
		}: {
			children: ReactNode;
			disabled?: boolean;
			onValueChange?: (value: string) => void;
		}) => (
			<SelectContext.Provider value={{ disabled, onValueChange }}>
				<div>{children}</div>
			</SelectContext.Provider>
		),
		SelectContent: ({ children }: { children: ReactNode }) => (
			<div>{children}</div>
		),
		SelectItem: ({
			children,
			value,
		}: {
			children: ReactNode;
			value: string;
		}) => {
			const context = useContext(SelectContext);
			return (
				<button
					type="button"
					aria-label={`select-item:${value}`}
					disabled={context.disabled}
					onClick={() => context.onValueChange?.(value)}
				>
					{children}
				</button>
			);
		},
		SelectTrigger: ({ children }: { children: ReactNode }) => (
			<div>{children}</div>
		),
		SelectValue: () => <span>select-value</span>,
	};
});

const t = (key: string, values?: Record<string, string>) =>
	values?.driver ? `${key}:${values.driver}` : key;

const remoteNodes = [
	{ id: 7, name: "Edge East", base_url: "https://edge.example.com" },
	{ id: 8, name: "Edge West", base_url: "" },
] as RemoteNodeInfo[];

const targets = [
	{
		base_path: "hot",
		driver_type: "local",
		is_default: true,
		name: "Hot",
		target_key: "rst_hot",
	},
	{
		base_path: "",
		driver_type: "s3",
		is_default: false,
		name: "Cold",
		target_key: "rst_cold",
	},
] as RemoteStorageTargetInfo[];

describe("StoragePolicyRemoteFields", () => {
	it("selects explicit targets and clears target selection when the remote node changes", () => {
		const onFieldChange = vi.fn();
		render(
			<RemoteNodeField
				error={null}
				form={{
					...emptyForm,
					remote_node_id: "7",
					remote_storage_target_key: "rst_cold",
				}}
				remoteNodes={remoteNodes}
				remoteStorageTargets={targets}
				t={t}
				onFieldChange={onFieldChange}
			/>,
		);

		expect(screen.getByText("Hot (core:default)")).toBeInTheDocument();
		expect(
			screen.getByText("remote_storage_target_hint:s3"),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "select-item:rst_hot" }),
		);
		expect(onFieldChange).toHaveBeenCalledWith(
			"remote_storage_target_key",
			"rst_hot",
		);

		fireEvent.click(screen.getByRole("button", { name: "select-item:8" }));
		expect(onFieldChange).toHaveBeenCalledWith("remote_node_id", "8");
		expect(onFieldChange).toHaveBeenCalledWith("remote_storage_target_key", "");
	});

	it("shows remote target loading, error, and empty states", () => {
		const { rerender } = render(
			<RemoteNodeField
				error="target required"
				form={{ ...emptyForm, remote_node_id: "7" }}
				remoteNodes={remoteNodes}
				remoteStorageTargets={[]}
				remoteStorageTargetsLoading
				showCreateValidation
				t={t}
				onFieldChange={vi.fn()}
			/>,
		);

		expect(
			screen.getByText("remote_storage_targets_loading"),
		).toBeInTheDocument();
		expect(
			screen
				.getAllByRole("button", { name: "select-item:__none__" })
				.find((button) => button.hasAttribute("disabled")),
		).toBeDisabled();
		expect(screen.getByText("target required")).toBeInTheDocument();

		rerender(
			<RemoteNodeField
				error={null}
				form={{ ...emptyForm, remote_node_id: "7" }}
				remoteNodes={remoteNodes}
				remoteStorageTargets={[]}
				remoteStorageTargetsError="load failed"
				t={t}
				onFieldChange={vi.fn()}
			/>,
		);
		expect(screen.getByText("load failed")).toBeInTheDocument();

		rerender(
			<RemoteNodeField
				error={null}
				form={{ ...emptyForm, remote_node_id: "7" }}
				remoteNodes={remoteNodes}
				remoteStorageTargets={[]}
				t={t}
				onFieldChange={vi.fn()}
			/>,
		);
		expect(
			screen.getByText("remote_storage_targets_empty"),
		).toBeInTheDocument();
	});
});
