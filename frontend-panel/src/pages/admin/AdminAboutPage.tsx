import { useTranslation } from "react-i18next";
import { AsterDriveWordmark } from "@/components/common/AsterDriveWordmark";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { AdminSurface } from "@/components/layout/AdminSurface";
import { Badge } from "@/components/ui/badge";
import { buttonVariants } from "@/components/ui/buttonVariants";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Icon, type IconName } from "@/components/ui/icon";
import { config } from "@/config/app";
import { usePageTitle } from "@/hooks/usePageTitle";
import { cn } from "@/lib/utils";

const REPOSITORY_URL = "https://github.com/AptS-1547/AsterDrive";
const DOCS_URL = "https://drive.astercosm.com/";
const LICENSE_URL = `${REPOSITORY_URL}/blob/master/LICENSE`;

type ReleaseChannel =
	| "release"
	| "development"
	| "alpha"
	| "beta"
	| "rc"
	| "unknown";

function formatDisplayVersion(version: string) {
	if (version === "unknown") return version;
	if (version === "dev") return "dev";
	return version.startsWith("v") ? version : `v${version}`;
}

function resolveReleaseChannel(version: string): ReleaseChannel {
	const normalized = version.toLowerCase();
	if (normalized === "dev") return "development";
	if (normalized.includes("alpha")) return "alpha";
	if (normalized.includes("beta")) return "beta";
	if (normalized.includes("rc")) return "rc";
	if (normalized === "unknown") return "unknown";
	return "release";
}

function getChannelBadgeClass(channel: ReleaseChannel) {
	switch (channel) {
		case "release":
			return "border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900 dark:bg-emerald-950/60 dark:text-emerald-300";
		case "development":
			return "border-sky-200 bg-sky-50 text-sky-700 dark:border-sky-900 dark:bg-sky-950/60 dark:text-sky-300";
		case "alpha":
			return "border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-900 dark:bg-amber-950/60 dark:text-amber-300";
		case "beta":
			return "border-violet-200 bg-violet-50 text-violet-700 dark:border-violet-900 dark:bg-violet-950/60 dark:text-violet-300";
		case "rc":
			return "border-blue-200 bg-blue-50 text-blue-700 dark:border-blue-900 dark:bg-blue-950/60 dark:text-blue-300";
		default:
			return "border-border bg-muted/40 text-muted-foreground";
	}
}

export default function AdminAboutPage() {
	const { t } = useTranslation("admin");
	usePageTitle(t("about"));
	const appVersion = config.appVersion;
	const displayVersion = formatDisplayVersion(appVersion);
	const releaseChannel = resolveReleaseChannel(appVersion);

	const resourceLinks: {
		href: string;
		label: string;
		icon: IconName;
	}[] = [
		{
			href: DOCS_URL,
			label: t("about_open_docs"),
			icon: "Globe",
		},
		{
			href: REPOSITORY_URL,
			label: t("about_view_repository"),
			icon: "LinkSimple",
		},
		{
			href: LICENSE_URL,
			label: t("about_view_license"),
			icon: "FileText",
		},
	];

	return (
		<AdminLayout>
			<AdminPageShell>
				<AdminPageHeader title={t("about")} description={t("about_intro")} />

				<AdminSurface className="flex-none gap-8 overflow-hidden bg-linear-to-br from-primary/[0.07] via-background to-background py-6 md:py-7 lg:flex-row lg:items-start">
					<div className="flex-1 space-y-5">
						<div className="space-y-3">
							<AsterDriveWordmark
								alt={config.appName}
								className="h-auto w-full max-w-[320px] md:max-w-[360px]"
								draggable={false}
							/>
							<div className="flex flex-wrap items-center gap-2">
								<Badge variant="outline">{t("about_product_badge")}</Badge>
								<Badge className="border-transparent bg-primary text-primary-foreground">
									{displayVersion}
								</Badge>
								<Badge
									variant="outline"
									className={getChannelBadgeClass(releaseChannel)}
								>
									{t(`about_channel_${releaseChannel}`)}
								</Badge>
							</div>
							<div className="space-y-2">
								<p className="max-w-2xl text-base text-foreground/85">
									{t("about_tagline")}
								</p>
								<p className="max-w-3xl text-sm leading-6 text-muted-foreground">
									{t("about_summary")}
								</p>
							</div>
						</div>
					</div>

					<Card className="w-full border-0 bg-background/85 py-5 shadow-none ring-1 ring-border/80 lg:max-w-md">
						<CardHeader className="border-b px-5">
							<CardTitle>{t("about_resources")}</CardTitle>
							<CardDescription>{t("about_resources_desc")}</CardDescription>
						</CardHeader>
						<CardContent className="space-y-5 px-5 pt-5">
							<dl className="space-y-3">
								<div className="flex items-start justify-between gap-4">
									<dt className="text-sm text-muted-foreground">
										{t("about_version")}
									</dt>
									<dd className="font-mono text-sm font-medium">
										{displayVersion}
									</dd>
								</div>
								<div className="flex items-start justify-between gap-4">
									<dt className="text-sm text-muted-foreground">
										{t("about_channel")}
									</dt>
									<dd className="text-sm font-medium">
										{t(`about_channel_${releaseChannel}`)}
									</dd>
								</div>
								<div className="flex items-start justify-between gap-4">
									<dt className="text-sm text-muted-foreground">
										{t("about_license")}
									</dt>
									<dd className="text-sm font-medium">MIT</dd>
								</div>
								<div className="flex items-start justify-between gap-4">
									<dt className="text-sm text-muted-foreground">
										{t("about_repository")}
									</dt>
									<dd className="text-right text-sm font-medium">
										AptS-1547/AsterDrive
									</dd>
								</div>
							</dl>

							<div className="grid gap-2 sm:grid-cols-3 lg:grid-cols-1">
								{resourceLinks.map((link) => (
									<a
										key={link.href}
										href={link.href}
										target="_blank"
										rel="noreferrer"
										className={cn(
											buttonVariants({ variant: "outline", size: "lg" }),
											"justify-between rounded-xl",
										)}
									>
										<span className="inline-flex items-center gap-2">
											<Icon name={link.icon} className="size-4" />
											{link.label}
										</span>
										<Icon
											name="ArrowSquareOut"
											className="size-3.5 text-muted-foreground"
										/>
									</a>
								))}
							</div>
						</CardContent>
					</Card>
				</AdminSurface>
			</AdminPageShell>
		</AdminLayout>
	);
}
