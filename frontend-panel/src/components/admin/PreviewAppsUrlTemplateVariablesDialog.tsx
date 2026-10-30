import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { DialogLifecycle } from "@/components/ui/dialog-lifecycle";

type Translate = (
	key: string,
	values?: Record<string, number | string>,
) => string;

type UrlTemplateMagicVariable = {
	descriptionKey: string;
	labelKey: string;
	token: string;
};

const URL_TEMPLATE_MAGIC_VARIABLES: UrlTemplateMagicVariable[] = [
	{
		token: "{{file_id}}",
		labelKey: "preview_apps_url_template_variable_file_id_label",
		descriptionKey: "preview_apps_url_template_variable_file_id_desc",
	},
	{
		token: "{{file_name}}",
		labelKey: "preview_apps_url_template_variable_file_name_label",
		descriptionKey: "preview_apps_url_template_variable_file_name_desc",
	},
	{
		token: "{{mime_type}}",
		labelKey: "preview_apps_url_template_variable_mime_type_label",
		descriptionKey: "preview_apps_url_template_variable_mime_type_desc",
	},
	{
		token: "{{size}}",
		labelKey: "preview_apps_url_template_variable_size_label",
		descriptionKey: "preview_apps_url_template_variable_size_desc",
	},
	{
		token: "{{download_path}}",
		labelKey: "preview_apps_url_template_variable_download_path_label",
		descriptionKey: "preview_apps_url_template_variable_download_path_desc",
	},
	{
		token: "{{download_url}}",
		labelKey: "preview_apps_url_template_variable_download_url_label",
		descriptionKey: "preview_apps_url_template_variable_download_url_desc",
	},
	{
		token: "{{file_preview_url}}",
		labelKey: "preview_apps_url_template_variable_file_preview_url_label",
		descriptionKey: "preview_apps_url_template_variable_file_preview_url_desc",
	},
];

interface PreviewAppsUrlTemplateVariablesDialogProps {
	appName: string;
	open: boolean;
	onOpenChange: (open: boolean) => void;
	t: Translate;
}

export function PreviewAppsUrlTemplateVariablesDialog(
	props: PreviewAppsUrlTemplateVariablesDialogProps,
) {
	return (
		<DialogLifecycle
			open={props.open}
			target={props}
			onOpenChange={props.onOpenChange}
		>
			{({ target, active, onOpenChange }) => (
				<PreviewAppsUrlTemplateVariablesDialogContent
					{...target}
					open={active}
					onOpenChange={onOpenChange}
				/>
			)}
		</DialogLifecycle>
	);
}

function PreviewAppsUrlTemplateVariablesDialogContent({
	appName,
	open,
	onOpenChange,
	t,
}: PreviewAppsUrlTemplateVariablesDialogProps) {
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-w-[calc(100%-1.5rem)] sm:max-w-[min(56rem,calc(100vw-2rem))]">
				<DialogHeader>
					<DialogTitle>
						{t("preview_apps_url_template_variables_title", {
							name: appName,
						})}
					</DialogTitle>
					<DialogDescription>
						{t("preview_apps_url_template_variables_dialog_desc")}
					</DialogDescription>
				</DialogHeader>
				<div className="max-h-[min(70vh,40rem)] space-y-3 overflow-y-auto py-2 pr-1">
					{URL_TEMPLATE_MAGIC_VARIABLES.map((variable) => (
						<div
							key={variable.token}
							className="rounded-xl border border-border/60 bg-card/40 p-4"
						>
							<div className="flex flex-wrap items-center gap-2">
								<code className="break-all rounded bg-muted px-2 py-1 font-mono text-xs">
									{variable.token}
								</code>
								<span className="text-sm font-medium">
									{t(variable.labelKey)}
								</span>
							</div>
							<p className="mt-2 break-words text-sm leading-6 text-muted-foreground">
								{t(variable.descriptionKey)}
							</p>
						</div>
					))}
				</div>
				<DialogFooter showCloseButton />
			</DialogContent>
		</Dialog>
	);
}
