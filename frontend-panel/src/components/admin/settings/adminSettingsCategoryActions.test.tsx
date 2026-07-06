import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
	AdminSettingsCategoryActions,
	getAdminSettingsCategoryActions,
} from "@/components/admin/settings/adminSettingsCategoryActions";
import type { ConfigActionDescriptor, ConfigSchemaItem } from "@/types/api";

function createAction(
	overrides: Partial<ConfigActionDescriptor> = {},
): ConfigActionDescriptor {
	return {
		action: "send_test_email",
		label_i18n_key: "mail_send_test_email",
		presentation: {
			category: "mail",
			group: "test",
			order: 10,
			subcategory: "config",
		},
		target_key: "mail",
		...overrides,
	};
}

function createSchema(actions: ConfigActionDescriptor[]): ConfigSchemaItem {
	return {
		actions,
		category: "mail.config",
		description: "",
		description_i18n_key: "settings_item_mail_smtp_host_desc",
		is_sensitive: false,
		key: "mail_smtp_host",
		label_i18n_key: "settings_item_mail_smtp_host_label",
		requires_restart: false,
		value_type: "string",
	};
}

describe("adminSettingsCategoryActions", () => {
	it("selects actions for the active category and subcategory", () => {
		const matchingAction = createAction();
		const actions = getAdminSettingsCategoryActions({
			category: "mail",
			subcategory: "config",
			schemas: [
				createSchema([
					createAction({
						presentation: {
							category: "mail",
							group: "test",
							order: 20,
							subcategory: "template",
						},
					}),
					matchingAction,
				]),
			],
		});

		expect(actions).toEqual([matchingAction]);
	});

	it("renders test email action buttons from descriptors", () => {
		const onOpenTestEmailDialog = vi.fn();
		render(
			<AdminSettingsCategoryActions
				actions={[createAction()]}
				onOpenTestEmailDialog={onOpenTestEmailDialog}
				t={(key) => key}
			/>,
		);

		screen.getByRole("button", { name: "mail_send_test_email" }).click();

		expect(onOpenTestEmailDialog).toHaveBeenCalledTimes(1);
		expect(screen.getByText("mail_send_test_email_hint")).toBeInTheDocument();
	});

	it("ignores unsupported category actions instead of rendering hardcoded UI", () => {
		const { container } = render(
			<AdminSettingsCategoryActions
				actions={[
					createAction({
						action: "test_aria2_rpc",
						target_key: "offline_download_engine_registry_json",
					}),
				]}
				onOpenTestEmailDialog={vi.fn()}
				t={(key) => key}
			/>,
		);

		expect(container).toBeEmptyDOMElement();
	});
});
