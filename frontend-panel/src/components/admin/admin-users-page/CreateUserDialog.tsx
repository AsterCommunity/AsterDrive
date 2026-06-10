import type { FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type { CreateUserReq } from "@/types/api";

interface CreateUserDialogProps {
	createErrors: Partial<CreateUserReq>;
	creating: boolean;
	form: CreateUserReq;
	open: boolean;
	onFieldChange: (key: keyof CreateUserReq, value: boolean | string) => void;
	onFieldValidate: (field: keyof CreateUserReq, value: string) => void;
	onOpenChange: (open: boolean) => void;
	onSubmit: (event: FormEvent<HTMLFormElement>) => void;
}

export function CreateUserDialog({
	createErrors,
	creating,
	form,
	open,
	onFieldChange,
	onFieldValidate,
	onOpenChange,
	onSubmit,
}: CreateUserDialogProps) {
	const { t } = useTranslation(["admin", "core"]);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent keepMounted className="sm:max-w-md">
				<form onSubmit={onSubmit} autoComplete="off" className="space-y-4">
					<DialogHeader>
						<DialogTitle>{t("create_user")}</DialogTitle>
						<DialogDescription>{t("create_user_desc")}</DialogDescription>
					</DialogHeader>
					<div className="space-y-2">
						<Label htmlFor="create-user-username">{t("core:username")}</Label>
						<Input
							id="create-user-username"
							name="admin-create-user-username"
							value={form.username}
							onChange={(event) => {
								const value = event.target.value;
								onFieldChange("username", value);
								onFieldValidate("username", value.trim());
							}}
							autoComplete="off"
							required
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							aria-invalid={!!createErrors.username}
						/>
						{createErrors.username ? (
							<p className="text-xs text-destructive">
								{createErrors.username}
							</p>
						) : null}
					</div>
					<div className="space-y-2">
						<Label htmlFor="create-user-email">{t("core:email")}</Label>
						<Input
							id="create-user-email"
							name="admin-create-user-email"
							value={form.email}
							onChange={(event) => {
								const value = event.target.value;
								onFieldChange("email", value);
								onFieldValidate("email", value.trim());
							}}
							autoComplete="off"
							required
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							aria-invalid={!!createErrors.email}
						/>
						{createErrors.email ? (
							<p className="text-xs text-destructive">{createErrors.email}</p>
						) : null}
					</div>
					<div className="space-y-2">
						<Label htmlFor="create-user-password">
							{t("create_user_password")}
						</Label>
						<Input
							id="create-user-password"
							name="admin-create-user-password"
							type="password"
							value={form.password ?? ""}
							onChange={(event) => {
								const value = event.target.value;
								onFieldChange("password", value);
								onFieldValidate("password", value);
							}}
							autoComplete="new-password"
							placeholder={t("create_user_password_placeholder")}
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							aria-invalid={!!createErrors.password}
						/>
						{createErrors.password ? (
							<p className="text-xs text-destructive">
								{createErrors.password}
							</p>
						) : null}
						<p className="text-xs text-muted-foreground">
							{t("create_user_password_hint")}
						</p>
					</div>
					<div className="flex items-center justify-between gap-3 rounded-md border p-3">
						<div className="min-w-0 space-y-1">
							<Label htmlFor="create-user-must-change-password">
								{t("force_password_change")}
							</Label>
							<p className="text-xs text-muted-foreground">
								{t("create_user_force_password_change_desc")}
							</p>
						</div>
						<Switch
							id="create-user-must-change-password"
							checked={Boolean(form.must_change_password)}
							onCheckedChange={(value) => {
								onFieldChange("must_change_password", value);
							}}
							aria-label={t("force_password_change")}
						/>
					</div>
					<DialogFooter>
						<Button
							type="button"
							variant="outline"
							onClick={() => onOpenChange(false)}
							disabled={creating}
						>
							{t("core:cancel")}
						</Button>
						<Button type="submit" disabled={creating}>
							{creating ? (
								<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
							) : (
								<Icon name="Plus" className="mr-1 size-4" />
							)}
							{t("core:create")}
						</Button>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>
	);
}
