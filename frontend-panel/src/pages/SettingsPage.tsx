import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Navigate, useNavigate, useParams } from "react-router-dom";
import { SettingsPageIntro } from "@/components/common/SettingsScaffold";
import { AppLayout } from "@/components/layout/AppLayout";
import { InterfaceSettingsView } from "@/components/settings/InterfaceSettingsView";
import { ProfileSettingsView } from "@/components/settings/ProfileSettingsView";
import { SecuritySettingsView } from "@/components/settings/SecuritySettingsView";
import { TeamsSettingsView } from "@/components/settings/TeamsSettingsView";
import { Icon } from "@/components/ui/icon";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { usePageTitle } from "@/hooks/usePageTitle";
import { SETTINGS_PAGE_CONTENT_PADDING_CLASS } from "@/lib/constants";

type SettingsTabKey = "profile" | "interface" | "security" | "teams";

function isSettingsTabKey(value?: string): value is SettingsTabKey {
	return (
		value === "profile" ||
		value === "interface" ||
		value === "security" ||
		value === "teams"
	);
}

function getSettingsSectionTitle(
	section: SettingsTabKey,
	t: ReturnType<typeof useTranslation>["t"],
) {
	switch (section) {
		case "interface":
			return t("settings:settings_interface");
		case "security":
			return t("settings:settings_security");
		case "teams":
			return t("settings:settings_teams");
		default:
			return t("settings:settings_profile");
	}
}

function renderSettingsSection(section: SettingsTabKey) {
	switch (section) {
		case "interface":
			return <InterfaceSettingsView />;
		case "security":
			return <SecuritySettingsView />;
		case "teams":
			return <TeamsSettingsView />;
		default:
			return <ProfileSettingsView />;
	}
}

export default function SettingsPage({
	section: sectionProp,
}: {
	section?: SettingsTabKey;
}) {
	const { t } = useTranslation(["core", "settings"]);
	const navigate = useNavigate();
	const { section: routeSection } = useParams<{ section?: string }>();
	const section = sectionProp ?? routeSection;
	const resolvedSection = isSettingsTabKey(section) ? section : "profile";
	const previousSectionRef = useRef<SettingsTabKey>(resolvedSection);
	const [hasAnimatedSectionChange, setHasAnimatedSectionChange] =
		useState(false);
	const panelAnimationClass = hasAnimatedSectionChange
		? "animate-in fade-in duration-150 motion-reduce:animate-none"
		: "";

	usePageTitle(getSettingsSectionTitle(resolvedSection, t));

	useEffect(() => {
		const previousSection = previousSectionRef.current;
		if (previousSection === resolvedSection) {
			return;
		}

		setHasAnimatedSectionChange(true);
		previousSectionRef.current = resolvedSection;
	}, [resolvedSection]);

	if (!isSettingsTabKey(section)) {
		return <Navigate to="/settings/profile" replace />;
	}

	const handleSectionChange = (value: string) => {
		if (
			(value === "profile" ||
				value === "interface" ||
				value === "security" ||
				value === "teams") &&
			value !== resolvedSection
		) {
			navigate(`/settings/${value}`, { viewTransition: false });
		}
	};

	return (
		<AppLayout>
			<div className="flex min-h-0 flex-1 flex-col overflow-auto">
				<div
					className={`mx-auto flex w-full max-w-6xl flex-col gap-5 ${SETTINGS_PAGE_CONTENT_PADDING_CLASS}`}
				>
					<SettingsPageIntro
						title={t("settings")}
						description={t("settings:settings_page_desc")}
					/>

					<Tabs
						value={resolvedSection}
						onValueChange={handleSectionChange}
						className="flex flex-col gap-4"
					>
						<TabsList
							variant="line"
							className="h-auto w-full gap-5 border-b px-0 pb-2"
						>
							<TabsTrigger
								value="profile"
								className="h-10 min-w-0 rounded-none px-0"
							>
								<Icon name="PencilSimple" className="size-4" />
								<span className="truncate">
									{t("settings:settings_profile")}
								</span>
							</TabsTrigger>
							<TabsTrigger
								value="interface"
								className="h-10 min-w-0 rounded-none px-0"
							>
								<Icon name="Monitor" className="size-4" />
								<span className="truncate">
									{t("settings:settings_interface")}
								</span>
							</TabsTrigger>
							<TabsTrigger
								value="security"
								className="h-10 min-w-0 rounded-none px-0"
							>
								<Icon name="Shield" className="size-4" />
								<span className="truncate">
									{t("settings:settings_security")}
								</span>
							</TabsTrigger>
							<TabsTrigger
								value="teams"
								className="h-10 min-w-0 rounded-none px-0"
							>
								<Icon name="Cloud" className="size-4" />
								<span className="truncate">{t("settings:settings_teams")}</span>
							</TabsTrigger>
						</TabsList>

						<TabsContent value={resolvedSection} className="outline-none">
							<div
								data-testid="settings-panel"
								key={resolvedSection}
								className={panelAnimationClass}
							>
								{renderSettingsSection(resolvedSection)}
							</div>
						</TabsContent>
					</Tabs>
				</div>
			</div>
		</AppLayout>
	);
}
