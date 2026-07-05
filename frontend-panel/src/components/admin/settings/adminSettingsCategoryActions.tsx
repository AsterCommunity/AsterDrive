import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import type { ConfigActionDescriptor, ConfigSchemaItem } from "@/types/api";

type TranslationFn = (key: string, options?: Record<string, unknown>) => string;

export function getAdminSettingsCategoryActions({
	category,
	schemas,
	subcategory,
}: {
	category: string;
	schemas: ConfigSchemaItem[];
	subcategory?: string;
}) {
	return schemas
		.flatMap((schema) => schema.actions ?? [])
		.filter((action) => {
			const presentation = action.presentation;
			return (
				presentation.category === category &&
				(presentation.subcategory ?? undefined) === subcategory
			);
		})
		.toSorted((left, right) => {
			return (
				left.presentation.order - right.presentation.order ||
				left.action.localeCompare(right.action)
			);
		});
}

function AdminSettingsCategoryActionButton({
	action,
	onOpenTestEmailDialog,
	t,
}: {
	action: ConfigActionDescriptor;
	onOpenTestEmailDialog: () => void;
	t: TranslationFn;
}) {
	if (action.action !== "send_test_email") {
		return null;
	}

	return (
		<div className="flex flex-col items-start gap-2 lg:items-end">
			<Button variant="outline" size="sm" onClick={onOpenTestEmailDialog}>
				<Icon name="EnvelopeSimple" className="size-4" />
				{t(action.label_i18n_key)}
			</Button>
			<p className="max-w-xs text-xs text-muted-foreground lg:text-right">
				{t("mail_send_test_email_hint")}
			</p>
		</div>
	);
}

export function AdminSettingsCategoryActions({
	actions,
	onOpenTestEmailDialog,
	t,
}: {
	actions: ConfigActionDescriptor[];
	onOpenTestEmailDialog: () => void;
	t: TranslationFn;
}) {
	const renderedActions = actions
		.map((action) => (
			<AdminSettingsCategoryActionButton
				key={`${action.target_key}:${action.action}`}
				action={action}
				onOpenTestEmailDialog={onOpenTestEmailDialog}
				t={t}
			/>
		))
		.filter(Boolean);

	if (renderedActions.length === 0) {
		return null;
	}

	return <>{renderedActions}</>;
}
