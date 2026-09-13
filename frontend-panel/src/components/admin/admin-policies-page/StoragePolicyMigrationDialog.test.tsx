import { render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { StoragePolicyMigrationDialog } from "@/components/admin/admin-policies-page/StoragePolicyMigrationDialog";
import type { StoragePolicy, StoragePolicyMigrationDryRun } from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
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
		...props
	}: {
		children?: ReactNode;
		[key: string]: unknown;
	}) => <button {...props}>{children}</button>,
}));
vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <i>{name}</i>,
}));
vi.mock("@/components/ui/label", () => ({
	Label: ({ children }: { children: ReactNode }) => <span>{children}</span>,
}));
vi.mock("@/components/ui/select", () => ({
	Select: ({ children }: { children: ReactNode }) => <div>{children}</div>,
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

const policies = [
	{ id: 1, name: "Source" },
	{ id: 2, name: "Target" },
] as StoragePolicy[];

function dryRun(reason: "provider_limits" | "buffered_heap_budget" | null) {
	return {
		can_start: reason === null,
		content_sha256_blob_count: 0,
		estimated_copy_blob_count: 1,
		opaque_blob_count: 0,
		opaque_key_conflict_count: 0,
		source_blob_count: 1,
		source_policy_id: 1,
		source_total_bytes: 1024,
		target_capacity: null,
		target_capacity_check: "sufficient",
		target_connection_ok: true,
		target_matching_blob_count: 0,
		target_policy_id: 2,
		target_supports_stream_upload: true,
		warnings: [],
		multipart_plan: {
			blob_size: 1024,
			can_start: reason === null,
			heap_budget: 64 * 1024 * 1024,
			part_count: 1,
			part_size: 1024,
			provider_max_parts: 10_000,
			provider_max_part_size: null,
			reason,
			upload_mode: "native_streaming",
		},
	} as StoragePolicyMigrationDryRun;
}

function renderDialog(dryRunValue: StoragePolicyMigrationDryRun) {
	return render(
		<StoragePolicyMigrationDialog
			dryRun={dryRunValue}
			dryRunLoading={false}
			open
			policies={policies}
			sourcePolicyId="1"
			submitting={false}
			targetPolicyId="2"
			onOpenChange={vi.fn()}
			onDryRun={vi.fn()}
			onSourcePolicyChange={vi.fn()}
			onSubmit={vi.fn()}
			onTargetPolicyChange={vi.fn()}
		/>,
	);
}

describe("StoragePolicyMigrationDialog", () => {
	it("renders native streaming multipart plan details", () => {
		renderDialog(dryRun(null));
		expect(
			screen.getByText("policy_migration_multipart_plan"),
		).toBeInTheDocument();
		expect(
			screen.getByText("policy_migration_multipart_mode_native_streaming"),
		).toBeInTheDocument();
	});

	it("renders the translated multipart block reason", () => {
		renderDialog(dryRun("buffered_heap_budget"));
		expect(
			screen.getByText(
				"policy_migration_multipart_reason_buffered_heap_budget",
			),
		).toBeInTheDocument();
	});
});
