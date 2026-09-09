import type {
	CreateRemoteNodeRequest,
	PatchRemoteNodeReq,
	RemoteNodeInfo,
} from "@/types/api";

export type RemoteNodeTransportMode = "direct" | "reverse_tunnel" | "auto";

export interface RemoteNodeFormData {
	name: string;
	base_url: string;
	transport_mode: RemoteNodeTransportMode;
	is_enabled: boolean;
}

export function getRemoteNodeForm(node: RemoteNodeInfo): RemoteNodeFormData {
	return {
		name: node.name,
		base_url: node.base_url,
		transport_mode: node.transport_mode ?? "direct",
		is_enabled: node.is_enabled,
	};
}

export function buildCreateRemoteNodePayload(
	form: RemoteNodeFormData,
): CreateRemoteNodeRequest {
	return {
		name: form.name,
		base_url: form.base_url || undefined,
		transport_mode: form.transport_mode,
		is_enabled: form.is_enabled,
	};
}

export function buildUpdateRemoteNodePayload(
	form: RemoteNodeFormData,
): PatchRemoteNodeReq {
	return {
		name: form.name,
		base_url: form.base_url,
		transport_mode: form.transport_mode,
		is_enabled: form.is_enabled,
	};
}

export function hasRemoteConnectionFieldChanges(
	form: RemoteNodeFormData,
	editingNode: RemoteNodeInfo | null,
) {
	if (!editingNode) {
		return true;
	}

	return (
		form.base_url !== editingNode.base_url ||
		form.transport_mode !== (editingNode.transport_mode ?? "direct")
	);
}

export function getRemoteNodeBaseUrlValidationMessage(
	baseUrl: string,
	t: (key: string) => string,
) {
	const trimmedBaseUrl = baseUrl.trim();
	if (!trimmedBaseUrl) {
		return null;
	}

	let parsedUrl: URL;
	try {
		parsedUrl = new URL(trimmedBaseUrl);
	} catch {
		return t("remote_node_base_url_invalid");
	}

	return parsedUrl.protocol === "http:" || parsedUrl.protocol === "https:"
		? null
		: t("remote_node_base_url_invalid");
}

export const emptyRemoteNodeForm: RemoteNodeFormData = {
	name: "",
	base_url: "",
	transport_mode: "direct",
	is_enabled: true,
};
