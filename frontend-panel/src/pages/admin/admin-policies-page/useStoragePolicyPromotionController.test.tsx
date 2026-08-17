import { act, renderHook, waitFor } from "@testing-library/react";
import type { Dispatch, SetStateAction } from "react";
import { describe, expect, it, vi } from "vitest";
import {
	getPolicyForm,
	type PolicyFormData,
} from "@/components/admin/storage-policy-dialog/formTypes";
import type { StorageConnectorPromotionCandidate } from "@/components/admin/storage-policy-dialog/policyPromotion";
import type { StorageConnectorDescriptor, StoragePolicy } from "@/types/api";
import { useStoragePolicyPromotionController } from "./useStoragePolicyPromotionController";

const mockState = vi.hoisted(() => ({
	promoteConnector: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("sonner", () => ({ toast: { success: vi.fn() } }));
vi.mock("@/hooks/useApiError", () => ({ handleApiError: vi.fn() }));
vi.mock("@/lib/adminPolicyLookup", () => ({
	invalidateAdminPolicyLookup: vi.fn(),
}));
vi.mock("@/lib/adminStorageConnectorLocalizations", () => ({
	translateStorageConnectorMessage: (
		_t: unknown,
		_connectorId: string,
		key: string,
	) => key,
}));
vi.mock("@/services/adminService", () => ({
	adminPolicyService: {
		promoteConnector: (...args: unknown[]) =>
			mockState.promoteConnector(...args),
	},
}));

const source = {
	capabilities: {
		storage_native_media_metadata: false,
		storage_native_thumbnail: false,
	},
	connector_id: "asterdrive.storage.s3",
	fields: [],
} as StorageConnectorDescriptor;

const target = {
	connector_id: "asterdrive.storage.tencent_cos",
	capabilities: {
		storage_native_media_metadata: false,
		storage_native_thumbnail: false,
	},
	fields: [],
	promotions: [
		{
			config_mappings: [],
			confirmation_key: "promotion_confirm",
			description_key: "promotion_desc",
			promotion_id: "promote_from_s3",
			requirements: [],
			source_connector_id: source.connector_id,
		},
	],
	ui: { label_key: "target_label" },
} as StorageConnectorDescriptor;

const savedPolicy: StoragePolicy = {
	allowed_types: [],
	behavior: {},
	chunk_size: 5 * 1024 * 1024,
	connector_config: {
		connector_id: source.connector_id,
		format_version: 1,
		schema_version: 1,
		values: {},
	},
	connector_id: source.connector_id,
	created_at: "2026-08-17T00:00:00Z",
	id: 7,
	is_default: false,
	max_file_size: 0,
	name: "Policy",
	updated_at: "2026-08-17T00:00:00Z",
};

interface HookProps {
	editingId: number | null;
	editingPolicy: StoragePolicy | null;
	form: PolicyFormData;
	descriptors: StorageConnectorDescriptor[];
}

function controllerInput(props: HookProps) {
	return {
		currentDescriptor: source,
		editingId: props.editingId,
		editingPolicy: props.editingPolicy,
		form: props.form,
		loadPolicyCapacity: vi.fn(),
		onDraftApplied: vi.fn(),
		onPromoted: vi.fn(),
		setEditingPolicy: vi.fn() as Dispatch<SetStateAction<StoragePolicy | null>>,
		setForm: vi.fn() as Dispatch<SetStateAction<PolicyFormData>>,
		setPolicies: vi.fn() as Dispatch<SetStateAction<StoragePolicy[]>>,
		storageDriverDescriptors: props.descriptors,
	};
}

describe("useStoragePolicyPromotionController guards", () => {
	it("does not let a stale response overwrite a newly opened policy", async () => {
		let resolvePromotionA!: (policy: StoragePolicy) => void;
		let resolvePromotionB!: (policy: StoragePolicy) => void;
		const promotionA = new Promise<StoragePolicy>((resolve) => {
			resolvePromotionA = resolve;
		});
		const promotionB = new Promise<StoragePolicy>((resolve) => {
			resolvePromotionB = resolve;
		});
		mockState.promoteConnector.mockReset();
		mockState.promoteConnector
			.mockReturnValueOnce(promotionA)
			.mockReturnValueOnce(promotionB);
		const setEditingPolicy = vi.fn() as Dispatch<
			SetStateAction<StoragePolicy | null>
		>;
		const setForm = vi.fn() as Dispatch<SetStateAction<PolicyFormData>>;
		const setPolicies = vi.fn() as Dispatch<SetStateAction<StoragePolicy[]>>;
		const initialProps: HookProps = {
			editingId: 7,
			editingPolicy: savedPolicy,
			form: getPolicyForm(savedPolicy),
			descriptors: [source, target],
		};
		const { result, rerender } = renderHook(
			(props: HookProps) =>
				useStoragePolicyPromotionController({
					...controllerInput(props),
					setEditingPolicy,
					setForm,
					setPolicies,
				}),
			{ initialProps },
		);
		let confirmationA!: Promise<void>;
		act(() => {
			confirmationA = result.current.confirm(result.current.candidates[0]);
		});

		act(() => result.current.reset());
		const policyB = { ...savedPolicy, id: 8, name: "Policy B" };
		rerender({
			...initialProps,
			editingId: 8,
			editingPolicy: policyB,
			form: getPolicyForm(policyB),
		});
		let confirmationB!: Promise<void>;
		act(() => {
			confirmationB = result.current.confirm(result.current.candidates[0]);
		});
		await act(async () => {
			resolvePromotionA({ ...savedPolicy, name: "Promoted A" });
			await confirmationA;
		});

		expect(setEditingPolicy).not.toHaveBeenCalled();
		expect(setForm).not.toHaveBeenCalled();
		expect(setPolicies).toHaveBeenCalledTimes(1);
		expect(result.current.submittingKey).toBe(
			`${target.connector_id}:promote_from_s3`,
		);

		await act(async () => {
			resolvePromotionB({ ...policyB, name: "Promoted B" });
			await confirmationB;
		});
		expect(setEditingPolicy).toHaveBeenCalledWith({
			...policyB,
			name: "Promoted B",
		});
		expect(result.current.submittingKey).toBeNull();
	});

	it("clears confirmation when the candidate disappears", async () => {
		const initialProps: HookProps = {
			editingId: 7,
			editingPolicy: savedPolicy,
			form: getPolicyForm(savedPolicy),
			descriptors: [source, target],
		};
		const { result, rerender } = renderHook(
			(props: HookProps) =>
				useStoragePolicyPromotionController(controllerInput(props)),
			{ initialProps },
		);
		const candidate = result.current.candidates[0];
		act(() => result.current.request(candidate));
		expect(result.current.confirmKey).toBe(
			`${target.connector_id}:promote_from_s3`,
		);

		rerender({ ...initialProps, descriptors: [] });
		await waitFor(() => expect(result.current.confirmKey).toBeNull());
	});

	it("rejects create, dirty, and stale saved confirmation attempts", async () => {
		mockState.promoteConnector.mockReset();
		const cleanProps: HookProps = {
			editingId: 7,
			editingPolicy: savedPolicy,
			form: getPolicyForm(savedPolicy),
			descriptors: [source, target],
		};
		const { result, rerender } = renderHook(
			(props: HookProps) =>
				useStoragePolicyPromotionController(controllerInput(props)),
			{ initialProps: cleanProps },
		);
		const candidate = result.current.candidates[0];
		const staleCandidate: StorageConnectorPromotionCandidate = {
			...candidate,
			promotion: {
				...candidate.promotion,
				promotion_id: "stale_promotion",
			},
		};

		await act(async () => result.current.confirm(staleCandidate));
		rerender({ ...cleanProps, editingId: null, editingPolicy: null });
		await act(async () => result.current.confirm(candidate));
		rerender({
			...cleanProps,
			form: { ...cleanProps.form, name: "Dirty" },
		});
		await act(async () => result.current.confirm(candidate));

		expect(mockState.promoteConnector).not.toHaveBeenCalled();
	});
});
