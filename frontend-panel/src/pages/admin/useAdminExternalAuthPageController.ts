import type { SetStateAction } from "react";
import {
	useCallback,
	useEffect,
	useMemo,
	useReducer,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import {
	buildManagedExternalAuthSearchParams,
	connectionRequirementsMissing,
	createPayload,
	DEFAULT_EXTERNAL_AUTH_PAGE_SIZE,
	defaultScopesForKind,
	EXTERNAL_AUTH_PAGE_SIZE_OPTIONS,
	type ExternalAuthCreateStep,
	type ExternalAuthProviderFormData,
	emptyForm,
	formatTestResultSummary,
	formConnectionChanged,
	formFromProvider,
	getManagedExternalAuthSearchString,
	isGitHubProviderKind,
	isGoogleProviderKind,
	mergeManagedExternalAuthSearchParams,
	normalizeOffset,
	requiredFieldsMissing,
	testParamsPayload,
	updatePayload,
} from "@/components/admin/admin-external-auth-page/shared";
import { handleApiError } from "@/hooks/useApiError";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { usePageTitle } from "@/hooks/usePageTitle";
import { writeTextToClipboard } from "@/lib/clipboard";
import {
	parseOffsetSearchParam,
	parsePageSizeOption,
	parsePageSizeSearchParam,
} from "@/lib/pagination";
import { adminExternalAuthService } from "@/services/adminService";
import type {
	AdminExternalAuthProviderInfo,
	AdminExternalAuthProviderKindInfo,
	ExternalAuthProviderKind,
	ExternalAuthProviderTestResult,
} from "@/types/api";

type AdminExternalAuthUiState = {
	createdProviderCallback: AdminExternalAuthProviderInfo | null;
	createStep: number;
	createStepTouched: boolean;
	deletingId: number | null;
	dialogOpen: boolean;
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
			type: "open_create";
	  }
	| {
			form: ExternalAuthProviderFormData;
			provider: AdminExternalAuthProviderInfo;
			type: "open_edit";
	  }
	| { open: boolean; type: "set_dialog_open" }
	| SetExternalAuthFormFieldAction
	| {
			kind: ExternalAuthProviderKind;
			patch?: Partial<ExternalAuthProviderFormData>;
			scopes: string;
			type: "set_provider_kind";
	  }
	| { step: number; type: "set_create_step" }
	| { touched: boolean; type: "set_create_step_touched" }
	| { submitting: boolean; type: "set_submitting" }
	| { id: number | null; type: "set_testing_id" }
	| { id: number | null; type: "set_deleting_id" }
	| { result: string | null; type: "set_test_result" }
	| {
			provider: AdminExternalAuthProviderInfo | null;
			type: "set_created_provider_callback";
	  }
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
		createdProviderCallback: null,
		createStep: 0,
		createStepTouched: false,
		deletingId: null,
		dialogOpen: false,
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

function resetDialogFields(state: AdminExternalAuthUiState) {
	return {
		...state,
		createStep: 0,
		createStepTouched: false,
		dialogOpen: false,
		editingProvider: null,
		form: emptyForm,
		submitting: false,
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
				providerKinds: action.providerKinds,
				providers: action.providers,
				total: action.total,
			};
		case "create_provider_kinds_loaded": {
			const nextKind = action.providerKinds[0];
			return {
				...state,
				form: nextKind
					? {
							...state.form,
							providerKind: nextKind.kind,
							scopes: defaultScopesForKind(nextKind),
						}
					: state.form,
				providerKinds: action.providerKinds,
			};
		}
		case "open_create":
			return {
				...state,
				createStep: 0,
				createStepTouched: false,
				dialogOpen: true,
				editingProvider: null,
				form: action.form,
				testResult: null,
			};
		case "open_edit":
			return {
				...state,
				createStep: 0,
				createStepTouched: false,
				dialogOpen: true,
				editingProvider: action.provider,
				form: action.form,
				testResult: null,
			};
		case "set_dialog_open":
			return action.open
				? { ...state, dialogOpen: true }
				: resetDialogFields(state);
		case "set_form_field":
			return {
				...state,
				form: {
					...state.form,
					[action.key]: action.value,
				} as ExternalAuthProviderFormData,
				testResult: null,
			};
		case "set_provider_kind":
			return {
				...state,
				form: {
					...state.form,
					...action.patch,
					providerKind: action.kind,
					scopes: action.scopes,
				},
				testResult: null,
			};
		case "set_create_step":
			return {
				...state,
				createStep: Math.max(0, Math.min(action.step, 2)),
				createStepTouched: false,
			};
		case "set_create_step_touched":
			return { ...state, createStepTouched: action.touched };
		case "set_submitting":
			return { ...state, submitting: action.submitting };
		case "set_testing_id":
			return { ...state, testingId: action.id };
		case "set_deleting_id":
			return { ...state, deletingId: action.id };
		case "set_test_result":
			return { ...state, testResult: action.result };
		case "set_created_provider_callback":
			return { ...state, createdProviderCallback: action.provider };
		case "provider_updated":
			return {
				...state,
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

export function useAdminExternalAuthPageController() {
	const { t } = useTranslation("admin");
	usePageTitle(t("external_auth"));
	const [searchParams, setSearchParams] = useSearchParams();
	const [offset, setOffsetState] = useState(() =>
		normalizeOffset(parseOffsetSearchParam(searchParams.get("offset"))),
	);
	const [pageSize, setPageSize] = useState<
		(typeof EXTERNAL_AUTH_PAGE_SIZE_OPTIONS)[number]
	>(() =>
		parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			EXTERNAL_AUTH_PAGE_SIZE_OPTIONS,
			DEFAULT_EXTERNAL_AUTH_PAGE_SIZE,
		),
	);
	const [uiState, dispatchUi] = useReducer(
		adminExternalAuthUiReducer,
		undefined,
		createInitialAdminExternalAuthUiState,
	);
	const {
		createdProviderCallback,
		createStep,
		createStepTouched,
		deletingId,
		dialogOpen,
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
	const lastWrittenSearchRef = useRef<string | null>(null);
	const createDialogRequestRef = useRef(0);
	const setOffset = useCallback((value: SetStateAction<number>) => {
		setOffsetState((current) =>
			normalizeOffset(typeof value === "function" ? value(current) : value),
		);
	}, []);
	const enabledCount = useMemo(
		() => providers.filter((provider) => provider.enabled).length,
		[providers],
	);
	const providerKindCount = providerKinds.length;
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
	const createSteps: ExternalAuthCreateStep[] = useMemo(
		() => [
			{
				title: t("external_auth_provider_wizard_step_type_title"),
				description: t("external_auth_provider_wizard_step_type_desc"),
			},
			{
				title: t("external_auth_provider_wizard_step_connection_title"),
				description: t("external_auth_provider_wizard_step_connection_desc"),
			},
			{
				title: t("external_auth_provider_wizard_step_rules_title"),
				description: t("external_auth_provider_wizard_step_rules_desc"),
			},
		],
		[t],
	);
	const previousCreateStepRef = useRef(createStep);
	const stepAnimationRef = useRef<{
		direction: "idle" | "forward" | "backward";
		step: number;
	}>({
		direction: "idle",
		step: createStep,
	});
	if (createStep !== previousCreateStepRef.current) {
		stepAnimationRef.current = {
			direction:
				createStep > previousCreateStepRef.current ? "forward" : "backward",
			step: createStep,
		};
	}
	const createStepDirection = stepAnimationRef.current.direction;

	useEffect(() => {
		const managedSearch = getManagedExternalAuthSearchString(searchParams);
		if (managedSearch === lastWrittenSearchRef.current) {
			return;
		}

		const nextOffset = normalizeOffset(
			parseOffsetSearchParam(searchParams.get("offset")),
		);
		const nextPageSize = parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			EXTERNAL_AUTH_PAGE_SIZE_OPTIONS,
			DEFAULT_EXTERNAL_AUTH_PAGE_SIZE,
		);

		setOffsetState((prev) => (prev === nextOffset ? prev : nextOffset));
		setPageSize((prev) => (prev === nextPageSize ? prev : nextPageSize));
	}, [searchParams]);

	useEffect(() => {
		const nextManagedSearchParams = buildManagedExternalAuthSearchParams({
			offset,
			pageSize,
		});
		const nextSearch = nextManagedSearchParams.toString();
		const currentSearch = getManagedExternalAuthSearchString(searchParams);
		if (
			currentSearch !== lastWrittenSearchRef.current &&
			currentSearch !== nextSearch
		) {
			return;
		}

		lastWrittenSearchRef.current = nextSearch;
		if (nextSearch === currentSearch) {
			return;
		}

		setSearchParams(
			mergeManagedExternalAuthSearchParams(
				searchParams,
				nextManagedSearchParams,
			),
			{ replace: true },
		);
	}, [offset, pageSize, searchParams, setSearchParams]);

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
		void loadProviders();
	}, [loadProviders]);

	useEffect(() => {
		if (!dialogOpen || editingProvider) {
			previousCreateStepRef.current = 0;
			stepAnimationRef.current = {
				direction: "idle",
				step: 0,
			};
			return;
		}

		previousCreateStepRef.current = createStep;
	}, [createStep, dialogOpen, editingProvider]);

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, EXTERNAL_AUTH_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setPageSize(next);
		setOffset(0);
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
		const descriptor = providerKinds.find((item) => item.kind === kind);
		const selectedProviderKind = descriptor ?? kind;
		const patch: Partial<ExternalAuthProviderFormData> = isGitHubProviderKind(
			selectedProviderKind,
		)
			? {
					authorizationUrl: "",
					avatarUrlClaim: "",
					displayName: form.displayName.trim() ? form.displayName : "GitHub",
					displayNameClaim: "",
					emailClaim: "",
					emailVerifiedClaim: "",
					groupsClaim: "",
					iconUrl: "",
					issuerUrl: "",
					requireEmailVerified: true,
					subjectClaim: "",
					tokenUrl: "",
					userinfoUrl: "",
					usernameClaim: "",
				}
			: isGoogleProviderKind(selectedProviderKind)
				? {
						authorizationUrl: "",
						avatarUrlClaim: "",
						displayName: form.displayName.trim() ? form.displayName : "Google",
						displayNameClaim: "",
						emailClaim: "",
						emailVerifiedClaim: "",
						groupsClaim: "",
						iconUrl: "",
						issuerUrl: "",
						requireEmailVerified: true,
						subjectClaim: "",
						tokenUrl: "",
						userinfoUrl: "",
						usernameClaim: "",
					}
				: {};
		dispatchUi({
			kind,
			patch,
			scopes: descriptor?.default_scopes || defaultScopesForKind(descriptor),
			type: "set_provider_kind",
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

	const openCreate = () => {
		const requestId = createDialogRequestRef.current + 1;
		createDialogRequestRef.current = requestId;
		const firstKind = providerKinds[0];
		dispatchUi({
			form: {
				...emptyForm,
				providerKind: firstKind?.kind ?? "oidc",
				scopes: defaultScopesForKind(firstKind),
			},
			type: "open_create",
		});
		if (providerKinds.length === 0) {
			void adminExternalAuthService
				.listKinds()
				.then((kinds) => {
					if (createDialogRequestRef.current !== requestId) {
						return;
					}
					dispatchUi({
						providerKinds: kinds,
						type: "create_provider_kinds_loaded",
					});
				})
				.catch(handleApiError);
		}
	};

	const openEdit = (provider: AdminExternalAuthProviderInfo) => {
		createDialogRequestRef.current += 1;
		dispatchUi({
			form: formFromProvider(provider),
			provider,
			type: "open_edit",
		});
	};

	const handleDialogOpenChange = (open: boolean) => {
		dispatchUi({ open, type: "set_dialog_open" });
		if (!open) {
			createDialogRequestRef.current += 1;
		}
	};

	const canAdvanceCreateStep = () => {
		if (createStep === 0) {
			return providerKinds.length > 0;
		}
		if (createStep === 1) {
			return !requiredFieldsMissing(form, selectedKind);
		}
		return true;
	};

	const goCreateNext = () => {
		dispatchUi({ touched: true, type: "set_create_step_touched" });
		if (!canAdvanceCreateStep()) {
			return;
		}
		dispatchUi({
			step: Math.min(createStep + 1, createSteps.length - 1),
			type: "set_create_step",
		});
	};

	const goCreateBack = () => {
		dispatchUi({
			step: Math.max(createStep - 1, 0),
			type: "set_create_step",
		});
	};

	const goCreateStep = (step: number) => {
		dispatchUi({
			step: Math.max(0, Math.min(step, createSteps.length - 1)),
			type: "set_create_step",
		});
	};

	const submitProvider = async () => {
		if (submitting) return;

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
				dispatchUi({
					provider: created,
					type: "set_created_provider_callback",
				});
			}
			await loadProviders();
			handleDialogOpenChange(false);
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
		if (connectionRequirementsMissing(form, selectedKind)) {
			dispatchUi({ touched: true, type: "set_create_step_touched" });
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

	return {
		copyCallbackUrl,
		createStep,
		createStepDirection,
		createStepTouched,
		createSteps,
		currentPage,
		createdProviderCallback,
		deleteProviderName,
		deletingId,
		dialogOpen,
		dialogProps,
		editingProvider,
		enabledCount,
		form,
		goCreateBack,
		goCreateNext,
		goCreateStep,
		handleDialogOpenChange,
		handlePageSizeChange,
		loadProviders,
		loading,
		nextPageDisabled,
		openCreate,
		openEdit,
		pageSize,
		pageSizeOptions,
		prevPageDisabled,
		providerKindCount,
		providerKinds,
		providers,
		requestConfirm,
		setCreatedProviderCallback: (
			provider: AdminExternalAuthProviderInfo | null,
		) =>
			dispatchUi({
				provider,
				type: "set_created_provider_callback",
			}),
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
