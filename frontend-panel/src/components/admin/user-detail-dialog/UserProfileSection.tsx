import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import {
	Tooltip,
	TooltipContent,
	TooltipProvider,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type { UserInfo, UserRole, UserStatus } from "@/types/api";

interface SelectOption<TValue extends string> {
	label: string;
	value: TValue;
}

interface UserProfileSectionProps {
	draftEmailVerified: boolean;
	draftRole: UserRole;
	draftStatus: UserStatus;
	emailVerificationOptions: ReadonlyArray<
		SelectOption<"verified" | "unverified">
	>;
	isInitialAdmin: boolean;
	onDraftEmailVerifiedChange: (value: boolean) => void;
	onDraftRoleChange: (value: UserRole) => void;
	onDraftStatusChange: (value: UserStatus) => void;
	roleOptions: ReadonlyArray<SelectOption<UserRole>>;
	savingProfile: boolean;
	statusOptions: ReadonlyArray<SelectOption<UserStatus>>;
	user: UserInfo;
	children: ReactNode;
}

export function UserProfileSection({
	children,
	draftEmailVerified,
	draftRole,
	draftStatus,
	emailVerificationOptions,
	isInitialAdmin,
	onDraftEmailVerifiedChange,
	onDraftRoleChange,
	onDraftStatusChange,
	roleOptions,
	savingProfile,
	statusOptions,
	user,
}: UserProfileSectionProps) {
	const { t } = useTranslation(["admin", "core"]);

	return (
		<section className="rounded-2xl border bg-background/60 p-6 max-lg:p-4">
			<div className="mb-5">
				<h4 className="text-base font-semibold text-foreground">
					{t("user_details")}
				</h4>
				<p className="mt-1 text-sm text-muted-foreground">
					{t("storage_quota_settings_desc")}
				</p>
			</div>
			<div className="grid gap-5 md:grid-cols-2">
				<div className="space-y-2">
					<Label>{t("core:email")}</Label>
					<Input
						value={user.email}
						readOnly
						className={ADMIN_CONTROL_HEIGHT_CLASS}
					/>
				</div>
				<div className="space-y-2">
					<div className="flex items-center justify-between gap-2">
						<Label>{t("email_verification")}</Label>
						{isInitialAdmin ? (
							<span className="text-xs text-muted-foreground">
								{t("initial_admin_protected")}
							</span>
						) : null}
					</div>
					<TooltipProvider>
						<Tooltip>
							<TooltipTrigger>
								<div>
									<Select
										items={emailVerificationOptions}
										value={draftEmailVerified ? "verified" : "unverified"}
										onValueChange={(value) =>
											onDraftEmailVerifiedChange(value === "verified")
										}
										disabled={isInitialAdmin || savingProfile}
									>
										<SelectTrigger className={ADMIN_CONTROL_HEIGHT_CLASS}>
											<SelectValue />
										</SelectTrigger>
										<SelectContent>
											{emailVerificationOptions.map((option) => (
												<SelectItem key={option.value} value={option.value}>
													{option.label}
												</SelectItem>
											))}
										</SelectContent>
									</Select>
								</div>
							</TooltipTrigger>
							{isInitialAdmin ? (
								<TooltipContent>
									{t("initial_admin_email_verification_blocked")}
								</TooltipContent>
							) : null}
						</Tooltip>
					</TooltipProvider>
				</div>
				<div className="space-y-2">
					<Label>{t("core:username")}</Label>
					<Input
						value={user.username}
						readOnly
						className={ADMIN_CONTROL_HEIGHT_CLASS}
					/>
				</div>
				<div className="space-y-2">
					<Label>{t("pending_email")}</Label>
					<Input
						value={user.pending_email ?? t("pending_email_none")}
						readOnly
						className={ADMIN_CONTROL_HEIGHT_CLASS}
					/>
				</div>
				<div className="space-y-2">
					<div className="flex items-center justify-between gap-2">
						<Label>{t("core:status")}</Label>
						{isInitialAdmin ? (
							<span className="text-xs text-muted-foreground">
								{t("initial_admin_protected")}
							</span>
						) : null}
					</div>
					<TooltipProvider>
						<Tooltip>
							<TooltipTrigger>
								<div>
									<Select
										items={statusOptions}
										value={draftStatus}
										onValueChange={(value) =>
											onDraftStatusChange(value as UserStatus)
										}
										disabled={isInitialAdmin || savingProfile}
									>
										<SelectTrigger>
											<SelectValue />
										</SelectTrigger>
										<SelectContent>
											{statusOptions.map((option) => (
												<SelectItem key={option.value} value={option.value}>
													{option.label}
												</SelectItem>
											))}
										</SelectContent>
									</Select>
								</div>
							</TooltipTrigger>
							{isInitialAdmin ? (
								<TooltipContent>
									{t("initial_admin_status_blocked")}
								</TooltipContent>
							) : null}
						</Tooltip>
					</TooltipProvider>
				</div>
				<div className="space-y-2">
					<div className="flex items-center justify-between gap-2">
						<Label>{t("role")}</Label>
						{isInitialAdmin ? (
							<span className="text-xs text-muted-foreground">
								{t("initial_admin_protected")}
							</span>
						) : null}
					</div>
					<TooltipProvider>
						<Tooltip>
							<TooltipTrigger>
								<div>
									<Select
										items={roleOptions}
										value={draftRole}
										onValueChange={(value) =>
											onDraftRoleChange(value as UserRole)
										}
										disabled={isInitialAdmin || savingProfile}
									>
										<SelectTrigger>
											<SelectValue />
										</SelectTrigger>
										<SelectContent>
											{roleOptions.map((option) => (
												<SelectItem key={option.value} value={option.value}>
													{option.label}
												</SelectItem>
											))}
										</SelectContent>
									</Select>
								</div>
							</TooltipTrigger>
							{isInitialAdmin ? (
								<TooltipContent>
									{t("initial_admin_role_blocked")}
								</TooltipContent>
							) : null}
						</Tooltip>
					</TooltipProvider>
				</div>
				<div className="space-y-2 md:col-span-2">{children}</div>
			</div>
		</section>
	);
}
