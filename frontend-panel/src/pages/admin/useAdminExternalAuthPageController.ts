import { useCallback, useEffect, useMemo, useReducer, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import {
	connectionRequirementsMissing,
	createPayload,
	DEFAULT_EXTERNAL_AUTH_PAGE_SIZE,
	EXTERNAL_AUTH_PAGE_SIZE_OPTIONS,
	type ExternalAuthProviderFormData,
	emptyForm,
	formatTestResultSummary,
	formConnectionChanged,
	formFromProvider,
	formFromProviderKind,
	requiredFieldsMissing,
	sortExternalAuthProviderKinds,
	testParamsPayload,
	updatePayload,
} from "@/components/admin/admin-external-auth-page/shared";
import { handleApiError } from "@/hooks/useApiError";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { useManagedOffset } from "@/hooks/useManagedAdminList";
import {
	type ManagedListQuerySchema,
	managedOffsetQueryField,
	managedPageSizeQueryField,
	useManagedListQueryState,
} from "@/hooks/useManagedListQueryState";
import { usePageTitle } from "@/hooks/usePageTitle";
import { writeTextToClipboard } from "@/lib/clipboard";
import { parsePageSizeOption } from "@/lib/pagination";
import { adminExternalAuthService } from "@/services/adminService";
import type {
	AdminExternalAuthProviderInfo,
	AdminExternalAuthProviderKindInfo,
	ExternalAuthProviderKind,
	ExternalAuthProviderTestResult,
} from "@/types/api";

type AdminExternalAuthUiState = {
	formTouched: boolean;
	deletingId: number | null;
	editingProvider: AdminExternalAuthProviderInfo | null;
	form: ExternalAuthProviderFormData;
	loading: boolean;
	providerKinds: AdminExternalAuthProviderKindInfo[];
	providers: AdminExternalAuthProviderInfo[];
	submitting: boolean;
	testResult: string | null;
	testingId: number | null;
	total: number;
};

type ManagedExternalAuthQuery = {
	offset: number;
	pageSize: (typeof EXTERNAL_AUTH_PAGE_SIZE_OPTIONS)[number];
};

const MANAGED_EXTERNAL_AUTH_QUERY_DEFAULTS = {
	offset: 0,
	pageSize: DEFAULT_EXTERNAL_AUTH_PAGE_SIZE,
} satisfies ManagedExternalAuthQuery;

const MANAGED_EXTERNAL_AUTH_QUERY_SCHEMA = {
	offset: managedOffsetQueryField(),
	pageSize: managedPageSizeQueryField(
		EXTERNAL_AUTH_PAGE_SIZE_OPTIONS,
		DEFAULT_EXTERNAL_AUTH_PAGE_SIZE,
	),
} satisfies ManagedListQuerySchema<ManagedExternalAuthQuery>;

type SetExternalAuthFormFieldAction<
	K extends
		keyof ExternalAuthProviderFormData = keyof ExternalAuthProviderFormData,
> = {
	key: K;
	type: "set_form_field";
	value: ExternalAuthProviderFormData[K];
};

type AdminExternalAuthUiAction =
	| { loading: boolean; type: "set_loading" }
	| {
			providerKinds: AdminExternalAuthProviderKindInfo[];
			providers: AdminExternalAuthProviderInfo[];
			total: number;
			type: "providers_loaded";
	  }
	| {
			providerKinds: AdminExternalAuthProviderKindInfo[];
			type: "create_provider_kinds_loaded";
	  }
	| {
			form: ExternalAuthProviderFormData;
			type: "initialize_create";
	  }
	| {
			form: ExternalAuthProviderFormData;
			provider: AdminExternalAuthProviderInfo;
			type: "initialize_edit";
	  }
	| SetExternalAuthFormFieldAction
	| {
			form: ExternalAuthProviderFormData;
			type: "replace_create_form";
	  }
	| { touched: boolean; type: "set_form_touched" }
	| { submitting: boolean; type: "set_submitting" }
	| { id: number | null; type: "set_testing_id" }
	| { id: number | null; type: "set_deleting_id" }
	| { result: string | null; type: "set_test_result" }
	| {
			provider: AdminExternalAuthProviderInfo;
			type: "provider_updated";
	  }
	| {
			providerId: number;
			updatedAt: string;
			type: "provider_timestamp_touched";
	  };

function createInitialAdminExternalAuthUiState(): AdminExternalAuthUiState {
	return {
		formTouched: false,
		deletingId: null,
		editingProvider: null,
		form: emptyForm,
		loading: true,
		providerKinds: [],
		providers: [],
		submitting: false,
		testResult: null,
		testingId: null,
		total: 0,
	};
}

function adminExternalAuthUiReducer(
	state: AdminExternalAuthUiState,
	action: AdminExternalAuthUiAction,
): AdminExternalAuthUiState {
	switch (action.type) {
		case "set_loading":
			return { ...state, loading: action.loading };
		case "providers_loaded":
			return {
				...state,
				providerKinds: sortExternalAuthProviderKinds(action.providerKinds),
				providers: action.providers,
				total: action.total,
			};
		case "create_provider_kinds_loaded": {
			const providerKinds = sortExternalAuthProviderKinds(action.providerKinds);
			const nextKind = providerKinds[0];
			return {
				...state,
				form: nextKind ? formFromProviderKind(nextKind) : state.form,
				providerKinds,
			};
		}
		case "initialize_create":
			return {
				...state,
				formTouched: false,
				editingProvider: null,
				form: action.form,
				testResult: null,
			};
		case "initialize_edit":
			return {
				...state,
				formTouched: false,
				editingProvider: action.provider,
				form: action.form,
				testResult: null,
			};
		case "set_form_field":
			return {
				...state,
				form: {
					...state.form,
					[action.key]: action.value,
				} as ExternalAuthProviderFormData,
				testResult: null,
			};
		case "replace_create_form":
			return {
				...state,
				formTouched: false,
				form: action.form,
				testResult: null,
			};
		case "set_form_touched":
			return { ...state, formTouched: action.touched };
		case "set_submitting":
			return { ...state, submitting: action.submitting };
		case "set_testing_id":
			return { ...state, testingId: action.id };
		case "set_deleting_id":
			return { ...state, deletingId: action.id };
		case "set_test_result":
			return { ...state, testResult: action.result };
		case "provider_updated":
			return {
				...state,
				editingProvider:
					state.editingProvider?.id === action.provider.id
						? action.provider
						: state.editingProvider,
				form:
					state.editingProvider?.id === action.provider.id
						? formFromProvider(action.provider)
						: state.form,
				providers: state.providers.map((provider) =>
					provider.id === action.provider.id ? action.provider : provider,
				),
			};
		case "provider_timestamp_touched":
			return {
				...state,
				providers: state.providers.map((provider) =>
					provider.id === action.providerId
						? { ...provider, updated_at: action.updatedAt }
						: provider,
				),
			};
	}
}

export type AdminExternalAuthPageVariant = "create" | "detail" | "list";

export function useAdminExternalAuthPageController({
	providerId,
	variant = "list",
}: {
	providerId?: number;
	variant?: AdminExternalAuthPageVariant;
} = {}) {
	const { t } = useTranslation("admin");
	const navigate = useNavigate();
	const [searchParams, setSearchParams] = useSearchParams();
	const { query, setQuery } = useManagedListQueryState({
		defaults: MANAGED_EXTERNAL_AUTH_QUERY_DEFAULTS,
		schema: MANAGED_EXTERNAL_AUTH_QUERY_SCHEMA,
		searchParams,
		setSearchParams,
	});
	const { offset, pageSize } = query;
	const [uiState, dispatchUi] = useReducer(
		adminExternalAuthUiReducer,
		undefined,
		createInitialAdminExternalAuthUiState,
	);
	const {
		formTouched,
		deletingId,
		editingProvider,
		form,
		loading,
		providerKinds,
		providers,
		submitting,
		testResult,
		testingId,
		total,
	} = uiState;
	const initialCreateFormRef = useRef(JSON.stringify(emptyForm));
	usePageTitle(
		variant === "create"
			? t("external_auth_provider_create")
			: variant === "detail"
				? (editingProvider?.display_name ?? t("external_auth_provider_edit"))
				: t("external_auth"),
	);
	const setOffset = useManagedOffset(setQuery);
	const selectedKind = useMemo(
		() =>
			providerKinds.find((kind) => kind.kind === form.providerKind) ??
			providerKinds[0] ??
			null,
		[form.providerKind, providerKinds],
	);
	const totalPages = Math.max(1, Math.ceil(total / pageSize));
	const currentPage = Math.floor(offset / pageSize) + 1;
	const prevPageDisabled = offset === 0;
	const nextPageDisabled = offset + pageSize >= total;
	const pageSizeOptions = EXTERNAL_AUTH_PAGE_SIZE_OPTIONS.map((size) => ({
		label: t("page_size_option", { count: size }),
		value: String(size),
	}));
	const loadProviders = useCallback(async () => {
		try {
			dispatchUi({ loading: true, type: "set_loading" });
			const [kinds, providerList] = await Promise.all([
				adminExternalAuthService.listKinds(),
				adminExternalAuthService.list({
					limit: pageSize,
					offset,
				}),
			]);
			if (providerList.items.length === 0 && providerList.total > 0) {
				const maxOffset =
					Math.floor((providerList.total - 1) / pageSize) * pageSize;
				if (offset > maxOffset) {
					setOffset(maxOffset);
					return;
				}
			}
			dispatchUi({
				providerKinds: kinds,
				providers: providerList.items,
				total: providerList.total,
				type: "providers_loaded",
			});
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchUi({ loading: false, type: "set_loading" });
		}
	}, [offset, pageSize, setOffset]);

	useEffect(() => {
		if (variant === "list") {
			void loadProviders();
			return;
		}

		let cancelled = false;
		dispatchUi({ loading: true, type: "set_loading" });
		const resource =
			variant === "detail" && providerId != null
				? Promise.all([
						adminExternalAuthService.listKinds(),
						adminExternalAuthService.get(providerId),
					]).then(([kinds, provider]) => {
						if (cancelled) return;
						dispatchUi({
							providerKinds: kinds,
							providers: [provider],
							total: 1,
							type: "providers_loaded",
						});
						dispatchUi({
							form: formFromProvider(provider),
							provider,
							type: "initialize_edit",
						});
					})
				: adminExternalAuthService.listKinds().then((kinds) => {
						if (cancelled) return;
						const sortedKinds = sortExternalAuthProviderKinds(kinds);
						const firstKind = sortedKinds[0];
						const initialForm = firstKind
							? formFromProviderKind(firstKind)
							: emptyForm;
						initialCreateFormRef.current = JSON.stringify(initialForm);
						dispatchUi({
							providerKinds: sortedKinds,
							type: "create_provider_kinds_loaded",
						});
						dispatchUi({
							form: initialForm,
							type: "initialize_create",
						});
					});

		void resource
			.catch((error) => {
				if (!cancelled) handleApiError(error);
			})
			.finally(() => {
				if (!cancelled) dispatchUi({ loading: false, type: "set_loading" });
			});
		return () => {
			cancelled = true;
		};
	}, [loadProviders, providerId, variant]);

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, EXTERNAL_AUTH_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setQuery({ offset: 0, pageSize: next });
	};

	const setField = <K extends keyof ExternalAuthProviderFormData>(
		key: K,
		value: ExternalAuthProviderFormData[K],
	) => {
		dispatchUi({
			key,
			type: "set_form_field",
			value,
		});
	};

	const setProviderKind = (kind: ExternalAuthProviderKind) => {
		if (kind === form.providerKind) {
			return;
		}
		const descriptor = providerKinds.find((item) => item.kind === kind);
		if (!descriptor) return;
		dispatchUi({
			form: formFromProviderKind(descriptor),
			type: "replace_create_form",
		});
	};

	const copyCallbackUrl = async (value: string) => {
		try {
			await writeTextToClipboard(value);
			toast.success(t("core:copied_to_clipboard"));
		} catch {
			toast.error(t("errors:unexpected_error"));
		}
	};

	const navigateBackToProviders = () => {
		navigate("/admin/external-auth", { viewTransition: false });
	};

	const submitProvider = async () => {
		if (submitting) return;

		dispatchUi({ touched: true, type: "set_form_touched" });
		if (
			requiredFieldsMissing(form, selectedKind) ||
			(!editingProvider && providerKinds.length === 0)
		) {
			return;
		}
		dispatchUi({ submitting: true, type: "set_submitting" });
		try {
			if (editingProvider) {
				const updated = await adminExternalAuthService.update(
					editingProvider.id,
					updatePayload(form, selectedKind),
				);
				dispatchUi({ provider: updated, type: "provider_updated" });
				toast.success(t("external_auth_provider_updated"));
			} else {
				const created = await adminExternalAuthService.create(
					createPayload(form, selectedKind),
				);
				toast.success(t("external_auth_provider_created"));
				navigate(`/admin/external-auth/${created.id}`, {
					replace: true,
					viewTransition: false,
				});
			}
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchUi({ submitting: false, type: "set_submitting" });
		}
	};

	const applyTestResult = (
		result: ExternalAuthProviderTestResult,
		options: { touchedProviderId?: number } = {},
	) => {
		dispatchUi({
			result: formatTestResultSummary(t, result),
			type: "set_test_result",
		});
		toast.success(t("external_auth_provider_test_success"));
		if (options.touchedProviderId != null) {
			dispatchUi({
				providerId: options.touchedProviderId,
				type: "provider_timestamp_touched",
				updatedAt: new Date().toISOString(),
			});
		}
	};

	const testFormConnection = async () => {
		dispatchUi({ touched: true, type: "set_form_touched" });
		if (connectionRequirementsMissing(form, selectedKind)) {
			return false;
		}

		try {
			if (
				editingProvider &&
				!formConnectionChanged(form, editingProvider, selectedKind)
			) {
				const result = await adminExternalAuthService.test(editingProvider.id);
				applyTestResult(result, { touchedProviderId: editingProvider.id });
				return true;
			}

			const result = await adminExternalAuthService.testParams(
				testParamsPayload(form, selectedKind),
			);
			applyTestResult(result);
			return true;
		} catch (error) {
			handleApiError(error);
			return false;
		}
	};

	const testProvider = async (provider: AdminExternalAuthProviderInfo) => {
		try {
			dispatchUi({ id: provider.id, type: "set_testing_id" });
			const result = await adminExternalAuthService.test(provider.id);
			applyTestResult(result, { touchedProviderId: provider.id });
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchUi({ id: null, type: "set_testing_id" });
		}
	};

	const deleteProvider = async (id: number) => {
		try {
			dispatchUi({ id, type: "set_deleting_id" });
			await adminExternalAuthService.delete(id);
			const isLastItemOnPage = providers.length === 1;
			const nextOffset =
				isLastItemOnPage && offset > 0
					? Math.max(0, offset - pageSize)
					: offset;
			if (nextOffset !== offset) {
				setOffset(nextOffset);
			} else {
				await loadProviders();
			}
			toast.success(t("external_auth_provider_deleted"));
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchUi({ id: null, type: "set_deleting_id" });
		}
	};
	const {
		confirmId: deleteId,
		requestConfirm,
		dialogProps,
	} = useConfirmDialog<number>(deleteProvider);
	const deleteProviderName =
		deleteId == null
			? ""
			: (providers.find((provider) => provider.id === deleteId)?.display_name ??
				"");
	const createDirty =
		variant === "create" &&
		JSON.stringify(form) !== initialCreateFormRef.current;

	return {
		copyCallbackUrl,
		createDirty,
		formTouched,
		currentPage,
		deleteProviderName,
		deletingId,
		dialogProps,
		editingProvider,
		form,
		navigateBackToProviders,
		handlePageSizeChange,
		loadProviders,
		loading,
		nextPageDisabled,
		pageSize,
		pageSizeOptions,
		prevPageDisabled,
		providerKinds,
		providers,
		requestConfirm,
		setField,
		setOffset,
		setProviderKind,
		submitProvider,
		submitting,
		t,
		testFormConnection,
		testProvider,
		testResult,
		testingId,
		total,
		totalPages,
	};
}
