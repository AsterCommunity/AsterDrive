import type { FormEvent } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { SharePublicInfo } from "@/types/api";
import { ShareCenteredPanel, ShareOwnerBanner } from "./ShareViewShell";

export function SharePasswordPanel({
	info,
	password,
	shareOwnerText,
	onPasswordChange,
	onSubmit,
	t,
}: {
	info: SharePublicInfo;
	password: string;
	shareOwnerText: string;
	onPasswordChange: (password: string) => void;
	onSubmit: (event: FormEvent) => void;
	t: (key: string) => string;
}) {
	return (
		<ShareCenteredPanel
			icon="Lock"
			title={info.name}
			description={t("share:password_protected")}
		>
			<div className="space-y-4">
				<ShareOwnerBanner owner={info.shared_by} text={shareOwnerText} />
				<form onSubmit={onSubmit} className="space-y-3">
					<Input
						type="password"
						placeholder={t("core:password")}
						value={password}
						onChange={(event) => onPasswordChange(event.target.value)}
					/>
					<Button type="submit" className="w-full">
						{t("verify")}
					</Button>
				</form>
			</div>
		</ShareCenteredPanel>
	);
}
