import { describe, expect, it } from "vitest";
import {
	buildCreateRemoteNodePayload,
	buildUpdateRemoteNodePayload,
	getRemoteNodeBaseUrlValidationMessage,
	getRemoteNodeForm,
	hasRemoteConnectionFieldChanges,
} from "@/components/admin/remoteNodeDialogShared";
import type { RemoteNodeInfo } from "@/types/api";

describe("remoteNodeDialogShared", () => {
	it("maps an existing remote node into form state", () => {
		expect(
			getRemoteNodeForm({
				id: 4,
				name: "Edge Alpha",
				base_url: "https://remote.example.com",
				transport_mode: "reverse_tunnel",
				is_enabled: true,
				last_probe_error: "",
				last_probe_at: null,
				enrollment_status: "completed",
				tunnel: {
					status: "online",
					runtime_error: "",
					last_handshake_at: "2026-05-29T08:00:00Z",
				},
				capabilities: {
					protocol_version: "v1",
					supports_list: true,
					supports_range_read: true,
					supports_stream_upload: true,
				},
				created_at: "",
				updated_at: "",
			} as RemoteNodeInfo),
		).toEqual({
			name: "Edge Alpha",
			base_url: "https://remote.example.com",
			transport_mode: "reverse_tunnel",
			is_enabled: true,
		});
	});

	it("defaults legacy remote nodes without a transport mode to direct", () => {
		expect(
			getRemoteNodeForm({
				name: "Legacy Edge",
				base_url: "https://legacy.example.com",
				transport_mode: null,
				is_enabled: false,
			} as unknown as RemoteNodeInfo),
		).toEqual({
			name: "Legacy Edge",
			base_url: "https://legacy.example.com",
			transport_mode: "direct",
			is_enabled: false,
		});
	});

	it("builds create payloads", () => {
		expect(
			buildCreateRemoteNodePayload({
				name: "Edge Alpha",
				base_url: "https://remote.example.com",
				transport_mode: "auto",
				is_enabled: true,
			}),
		).toEqual({
			name: "Edge Alpha",
			base_url: "https://remote.example.com",
			transport_mode: "auto",
			is_enabled: true,
		});
	});

	it("omits empty base URLs from create payloads", () => {
		expect(
			buildCreateRemoteNodePayload({
				name: "Tunnel Edge",
				base_url: "",
				transport_mode: "reverse_tunnel",
				is_enabled: true,
			}),
		).toEqual({
			name: "Tunnel Edge",
			base_url: undefined,
			transport_mode: "reverse_tunnel",
			is_enabled: true,
		});
	});

	it("builds update payloads without managed credentials", () => {
		expect(
			buildUpdateRemoteNodePayload({
				name: "Edge Alpha",
				base_url: "",
				transport_mode: "reverse_tunnel",
				is_enabled: false,
			}),
		).toEqual({
			name: "Edge Alpha",
			base_url: "",
			transport_mode: "reverse_tunnel",
			is_enabled: false,
		});
	});

	it("detects transport mode as a connection field change", () => {
		const node = {
			base_url: "",
			transport_mode: "direct",
		} as RemoteNodeInfo;

		expect(
			hasRemoteConnectionFieldChanges(
				{
					name: "Edge Alpha",
					base_url: "",
					transport_mode: "reverse_tunnel",
					is_enabled: true,
				},
				node,
			),
		).toBe(true);
		expect(
			hasRemoteConnectionFieldChanges(
				{
					name: "Edge Alpha",
					base_url: "",
					transport_mode: "direct",
					is_enabled: true,
				},
				node,
			),
		).toBe(false);
	});

	it("allows an empty remote node base URL", () => {
		expect(
			getRemoteNodeBaseUrlValidationMessage("   ", (key) => key),
		).toBeNull();
	});

	it("rejects remote node base URLs that are not absolute http or https URLs", () => {
		expect(
			getRemoteNodeBaseUrlValidationMessage("remote.example.com", (key) => key),
		).toBe("remote_node_base_url_invalid");
		expect(
			getRemoteNodeBaseUrlValidationMessage(
				"ftp://remote.example.com",
				(key) => key,
			),
		).toBe("remote_node_base_url_invalid");
	});

	it("accepts absolute http and https remote node base URLs", () => {
		expect(
			getRemoteNodeBaseUrlValidationMessage(
				"https://remote.example.com/api",
				(key) => key,
			),
		).toBeNull();
		expect(
			getRemoteNodeBaseUrlValidationMessage(
				"http://remote.example.com",
				(key) => key,
			),
		).toBeNull();
	});
});
