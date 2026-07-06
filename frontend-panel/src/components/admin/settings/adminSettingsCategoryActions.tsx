import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import type { ConfigActionDescriptor, ConfigSchemaItem } from "@/types/api";

type TranslationFn = (key: string, options?: Record<string, unknown>) => string;
type CategoryActionRendererProps = {
	action: ConfigActionDescriptor;
	onOpenTestEmailDialog: () => void;
	t: TranslationFn;
};
type CategoryActionRenderer = (props: CategoryActionRendererProps) => ReactNode;

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
}: CategoryActionRendererProps) {
	const renderer = ADMIN_SETTINGS_CATEGORY_ACTION_RENDERERS[action.action];
	return renderer?.({ action, onOpenTestEmailDialog, t }) ?? null;
}

const ADMIN_SETTINGS_CATEGORY_ACTION_RENDERERS: Record<
	string,
	CategoryActionRenderer
> = {
	send_test_email: ({ action, onOpenTestEmailDialog, t }) => (
		<div className="flex flex-col items-start gap-2 lg:items-end">
			<Button variant="outline" size="sm" onClick={onOpenTestEmailDialog}>
				<Icon name="EnvelopeSimple" className="size-4" />
				{t(action.label_i18n_key)}
			</Button>
			<p className="max-w-xs text-xs text-muted-foreground lg:text-right">
				{t("mail_send_test_email_hint")}
			</p>
		</div>
	),
};

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
