import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import {
	buildCreateRemoteNodePayload,
	emptyRemoteNodeForm,
	getRemoteNodeBaseUrlValidationMessage,
	type RemoteNodeFormData,
} from "@/components/admin/remoteNodePageShared";
import { getApiErrorMessage, handleApiError } from "@/hooks/useApiError";
import { usePageTitle } from "@/hooks/usePageTitle";
import { invalidateAdminRemoteNodeLookup } from "@/lib/adminRemoteNodeLookup";
import { adminRemoteNodeService } from "@/services/adminService";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";

export function useAdminRemoteNodeCreateController() {
	const { t } = useTranslation("admin");
	const navigate = useNavigate();
	const primarySiteUrl = useFrontendConfigStore((state) => state.siteUrl);
	const [form, setForm] = useState<RemoteNodeFormData>(emptyRemoteNodeForm);
	const [submitting, setSubmitting] = useState(false);
	const initialForm = useRef(JSON.stringify(emptyRemoteNodeForm));
	const baseUrlValidationMessage = getRemoteNodeBaseUrlValidationMessage(
		form.base_url,
		t,
	);
	usePageTitle(t("create_remote_node"));

	const setField = <K extends keyof RemoteNodeFormData>(
		key: K,
		value: RemoteNodeFormData[K],
	) => setForm((current) => ({ ...current, [key]: value }));

	const submit = async () => {
		if (
			submitting ||
			!form.name.trim() ||
			baseUrlValidationMessage ||
			!primarySiteUrl
		) {
			return;
		}
		setSubmitting(true);
		try {
			const created = await adminRemoteNodeService.create(
				buildCreateRemoteNodePayload(form),
			);
			invalidateAdminRemoteNodeLookup();
			try {
				const command = await adminRemoteNodeService.createEnrollmentCommand(
					created.id,
				);
				toast.success(t("remote_node_enrollment_prepared"));
				navigate(`/admin/remote-nodes/${created.id}`, {
					replace: true,
					state: { enrollmentCommand: command },
					viewTransition: false,
				});
			} catch (error) {
				const message = getApiErrorMessage(error);
				handleApiError(error);
				navigate(`/admin/remote-nodes/${created.id}`, {
					replace: true,
					state: { enrollmentError: message },
					viewTransition: false,
				});
			}
		} catch (error) {
			handleApiError(error);
		} finally {
			setSubmitting(false);
		}
	};

	return {
		baseUrlValidationMessage,
		createAllowed: Boolean(primarySiteUrl),
		createDirty: !submitting && JSON.stringify(form) !== initialForm.current,
		form,
		navigateBack: () =>
			navigate("/admin/remote-nodes", { viewTransition: false }),
		t,
		setField,
		submit,
		submitting,
	};
}
