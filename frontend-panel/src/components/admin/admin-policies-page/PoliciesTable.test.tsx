import { fireEvent, render, screen } from "@testing-library/react";
import type { ComponentProps } from "react";
import { describe, expect, it, vi } from "vitest";
import type { StorageConnectorDescriptor, StoragePolicy } from "@/types/api";
import { PoliciesTable } from "./PoliciesTable";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

function descriptor(): StorageConnectorDescriptor {
	return {
		actions: [],
		authorization_provider: null,
		capabilities: {
			capacity: true,
			efficient_range: true,
			list: true,
			object_naming: "opaque_uuid",
			object_storage_transfer_strategy: false,
			presigned_download: false,
			remote_node_binding: true,
			storage_native_media_metadata: false,
			storage_native_thumbnail: false,
		},
		config_schema_version: 1,
		connector_id: "plugin.archive",
		credential_mode: "none",
		deployment_scope: "instance_local",
		description: "archive",
		fields: [
			{
				kind: "boolean",
				label_key: "enabled",
				name: "enabled",
				required: false,
				scope: "connector_config",
				secret: false,
			},
			{
				kind: "select",
				label_key: "mode",
				name: "mode",
				required: false,
				scope: "connector_config",
				secret: false,
				select: {
					options: [{ label_key: "mode_relay", value: "relay" }],
					value_kind: "string",
				},
			},
			{
				kind: "select",
				label_key: "node",
				name: "node",
				required: false,
				scope: "connector_config",
				secret: false,
				select: {
					data_source: "remote_nodes",
					value_kind: "integer",
				},
			},
			{
				kind: "text",
				label_key: "endpoint",
				name: "endpoint",
				required: false,
				scope: "connector_config",
				secret: false,
			},
		],
		label: "archive",
		related_issues: [],
		requires_authorization: false,
		supports_initial_setup: true,
		ui: {
			badge_rgb: { red: 16, green: 185, blue: 129 },
			base_path_empty_display: "core:root",
			base_path_placeholder: "base_path",
			config_step_description_key: "config_desc",
			config_step_title_key: "config_title",
			description_key: "description",
			edit_context_key: "edit_context",
			helper_key: "helper",
			icon_name: null,
			icon_src: null,
			label_key: "archive_label",
		},
		upload_workflows: {
			frontend_direct_provider_resumable_upload: false,
			object_multipart_upload: false,
			object_multipart_upload_capabilities: null,
			presigned_upload: false,
			provider_resumable_upload: false,
			provider_resumable_upload_capabilities: null,
			simple_upload: true,
			simple_upload_capabilities: {
				max_provider_single_request_size: null,
				policy_limited: true,
				server_side_relay: true,
			},
			stream_upload: true,
		},
	};
}

function policy(
	id: number,
	overrides: Partial<StoragePolicy> = {},
): StoragePolicy {
	return {
		allowed_types: [],
		behavior: {},
		chunk_size: 5 * 1024 * 1024,
		connector_config: {
			connector_id: "plugin.archive",
			format_version: 1,
			schema_version: 1,
			values: {
				enabled: true,
				endpoint: "https://archive.example.test",
				mode: "relay",
				node: 7,
			},
		},
		connector_id: "plugin.archive",
		created_at: "2026-01-01T00:00:00Z",
		id,
		is_default: id === 2,
		max_file_size: 0,
		name: `Policy ${id}`,
		updated_at: "2026-01-01T00:00:00Z",
		...overrides,
	};
}

function renderTable(
	overrides: Partial<ComponentProps<typeof PoliciesTable>> = {},
) {
	const props: ComponentProps<typeof PoliciesTable> = {
		deletingPolicyId: null,
		loading: false,
		onDeletePolicy: vi.fn(),
		onEditPolicy: vi.fn(),
		policies: [
			policy(1),
			policy(2, {
				connector_config: {
					connector_id: "plugin.archive",
					format_version: 1,
					schema_version: 1,
					values: { enabled: false },
				},
			}),
			policy(3, { connector_id: "plugin.unknown" }),
		],
		remoteNodeNameById: new Map([[7, "Node seven"]]),
		sortBy: "id",
		sortOrder: "asc",
		storageDriverDescriptors: [descriptor()],
		onSortChange: vi.fn(),
		...overrides,
	};
	return { ...render(<PoliciesTable {...props} />), props };
}

describe("PoliciesTable", () => {
	it("renders descriptor-driven summaries, fallbacks, and default badges", () => {
		renderTable();

		expect(screen.getAllByText("archive_label")).toHaveLength(2);
		expect(screen.getByText(/enabled: core:yes/)).toBeVisible();
		expect(screen.getByText(/enabled: core:no/)).toBeVisible();
		expect(screen.getByText(/mode: mode_relay/)).toBeVisible();
		expect(screen.getByText(/node: Node seven/)).toBeVisible();
		expect(screen.getByText("plugin.unknown")).toBeVisible();
		expect(screen.getAllByText("is_default")).toHaveLength(2);
	});

	it("renders scalar text values in the configuration summary", () => {
		const textDescriptor = descriptor();
		textDescriptor.fields = [textDescriptor.fields[3]];

		renderTable({
			policies: [policy(1)],
			storageDriverDescriptors: [textDescriptor],
		});

		expect(
			screen.getByText("endpoint: https://archive.example.test"),
		).toBeVisible();
	});

	it("falls back for unresolved remote nodes and missing option labels", () => {
		const fallbackDescriptor = descriptor();
		fallbackDescriptor.fields = [
			fallbackDescriptor.fields[2],
			{
				...fallbackDescriptor.fields[1],
				select: {
					options: [{ value: "relay" } as never],
					value_kind: "string",
				},
			},
		];

		renderTable({
			policies: [policy(1)],
			remoteNodeNameById: new Map(),
			storageDriverDescriptors: [fallbackDescriptor],
		});

		expect(screen.getByText(/node: #7/)).toBeVisible();
		expect(screen.getByText(/mode: mode/)).toBeVisible();
	});

	it("supports row keyboard/edit/delete behavior and deleting states", () => {
		const { props } = renderTable({ deletingPolicyId: 2 });
		const rows = screen.getAllByRole("row");
		fireEvent.click(rows[1]);
		fireEvent.keyDown(rows[1], { key: "Enter" });
		fireEvent.keyDown(rows[1], { key: " " });
		expect(props.onEditPolicy).toHaveBeenCalledTimes(3);

		const buttons = screen.getAllByRole("button");
		const deleteButtons = buttons.filter((button) =>
			button.getAttribute("aria-label"),
		);
		expect(deleteButtons[0]).toBeEnabled();
		expect(deleteButtons[1]).toBeDisabled();
	});

	it("renders loading and empty states and forwards header sorting", () => {
		const loading = renderTable({ loading: true, policies: [] });
		expect(
			loading.container.querySelectorAll('[data-slot="skeleton"]'),
		).not.toHaveLength(0);
		loading.unmount();

		const empty = renderTable({ policies: [] });
		expect(screen.getByText("no_policies")).toBeVisible();
		empty.unmount();

		const view = renderTable();
		fireEvent.click(screen.getByRole("button", { name: /id/ }));
		expect(view.props.onSortChange).toHaveBeenCalledWith("id", "desc");
	});
});
