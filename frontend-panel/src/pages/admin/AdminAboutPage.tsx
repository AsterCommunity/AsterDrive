import { useCallback, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { AsterDriveWordmark } from "@/components/common/AsterDriveWordmark";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { AdminSurface } from "@/components/layout/AdminSurface";
import { Badge } from "@/components/ui/badge";
import { badgeVariants } from "@/components/ui/badgeVariants";
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
const VERSION_EASTER_EGG_CLICKS = 5;
const VERSION_EASTER_EGG_MESSAGES = [
	"ESAP-TY-0001 initialized.",
	"愿 AptS 与你同在。",
	"48 was here.",
	"猫猫，版本号不是按钮。算了，这次算你找到了。",
	"AsterCommunity 正在招人！",
	"AsterCommunity 缺人，缺文档，缺测试，最缺不跑路的人。",
	"AsterCommunity 招募中：会写代码优先，会写测试更优先。",
	"AsterCommunity 招募中：不怕 Rust 编译器的人请靠近。",
	"AsterCommunity welcomes stubborn builders.",
	"AsterDrive: self-hosted, slightly stubborn.",
	"AsterDrive is watching the quota counters.",
	"AsterDrive 没有云厂商味，至少我们努力没有。",
	"AsterDrive needs fewer mysteries and more maintainers.",
	"Archive task queued. Coffee recommended.",
	"Backup first. Be dramatic later.",
	"Blob dedup says hello.",
	"Blob ref_count balanced. Nobody touch anything.",
	"Build green, sleep better.",
	"Cache invalidated. Hope was not cached.",
	"CI passed? Suspicious, but acceptable.",
	"Cloud storage, but make it yours.",
	"Config loaded. Chaos postponed.",
	"Database migrated. Everyone pretend this was easy.",
	"Do not anger the migration history.",
	"Every ref_count deserves respect.",
	"Feature request accepted. Future 48 is annoyed.",
	"File browser calm. Internals less calm.",
	"Found the version hatch. Don't tell the humans.",
	"Hash matched. Trust carefully.",
	"Hidden route? No. Hidden attitude? Yes.",
	"Human error detected. Blame unavailable.",
	"Local-first mood, remote-capable attitude.",
	"Maintainers wanted. Sanity optional.",
	"May your chunks arrive in order.",
	"Metadata says it is fine. Metadata has lied before.",
	"Migration gods accepted today's offering.",
	"Never trust a silent upload retry.",
	"No files were harmed in this toast.",
	"Open source survives on curious people.",
	"Permission check passed. Suspicion remains.",
	"Quota math has no mercy.",
	"Random channel selected. Reality unchanged.",
	"Refs balanced. Universe temporarily stable.",
	"Release channel shuffled for entertainment purposes.",
	"Resume upload found its way home.",
	"Storage policy aligned. Mood: suspicious.",
	"The drive remembers. The UI pretends not to.",
	"The folder tree is judging your recursion.",
	"The thumbnail worker is probably doing its best.",
	"This badge has no business being clicked five times.",
	"This toast was definitely not in the spec.",
	"Trash cleanup has entered the chat.",
	"Upload session survived. Barely.",
	"Version found. Secrets remain classified.",
	"WebDAV knocked. Nobody panicked. Impressive.",
	"人干坏事的时候就不会闲着.jpg",
	"你小子点得还挺准。",
	"你点版本号干什么，想审计我？",
	"你已经点了五下，系统决定假装没看见。",
	"你再点，版本号就要申请工伤了。",
	"你发现了没用的秘密，但至少很快乐。",
	"你看起来很闲，AsterCommunity 正好缺人。",
	"写测试，不然 48 会盯着你。",
	"别翻了，彩蛋池暂时就这么多。",
	"别问，问就是产品需求。",
	"后端还活着，前端先别庆祝。",
	"坏了，真有人在点版本号。",
	"坏消息：这里没有管理员后门。好消息：这里有一句废话，而且你本来就是管理员。",
	"如果这也算功能，那我可要写测试了。",
	"好消息：这不是 bug。坏消息：这是我故意的。",
	"如果你看到这句，说明随机数今天站你这边。",
	"已经点到这里了，不如顺手写个 PR。",
	"愿你的迁移永不回滚。",
	"愿你的上传任务不要卡在 99%。",
	"愿你的分片一个不少，愿你的哈希一次通过。",
	"愿你的数据库连接池永远够用。",
	"愿你的缓存失效发生在正确的那一秒。",
	"欢迎加入 AsterCommunity，一起把云盘写得不像云盘。",
	"欢迎来到关于页地下二层，这里没有电梯。",
	"猫猫，少点几下，版本号也会累。",
	"猫猫，这个按钮真的只是版本号，至少表面上是。",
	"现在收手还来得及，虽然计数器已经记住你了。",
	"管理员发现了隐藏入口，但权限没有变多。",
	"管理后台保持冷静，彩蛋负责胡说八道。",
	"系统配置说它很稳定，我选择暂时相信。",
	"缓存、索引、权限、配额，我们会让他们都不找你，至少不会炸你。",
	"文件夹树没有迷路，它只是走得比较抽象。",
	"断点续传成功的时候，请对网络说谢谢。",
	"缩略图正在路上，别催，它也有情绪。",
	"迁移文件已经排队，谁插队谁写回滚。",
	"这不是隐藏功能，这是维护者的精神状态。",
	"这条 toast 没有 KPI，但它出现了。",
	"这里没有宝藏，只有还没写完的 issue。",
	"点击版本号不会提升权限，只会暴露你的好奇心。等等，你本来就是管理员？",
] as const;
const VERSION_BADGE_CLASSES = [
	"border-transparent bg-primary text-primary-foreground",
	"border-cyan-200 bg-cyan-50 text-cyan-700 dark:border-cyan-900 dark:bg-cyan-950/60 dark:text-cyan-300",
	"border-rose-200 bg-rose-50 text-rose-700 dark:border-rose-900 dark:bg-rose-950/60 dark:text-rose-300",
	"border-lime-200 bg-lime-50 text-lime-700 dark:border-lime-900 dark:bg-lime-950/60 dark:text-lime-300",
	"border-fuchsia-200 bg-fuchsia-50 text-fuchsia-700 dark:border-fuchsia-900 dark:bg-fuchsia-950/60 dark:text-fuchsia-300",
	"border-orange-200 bg-orange-50 text-orange-700 dark:border-orange-900 dark:bg-orange-950/60 dark:text-orange-300",
	"border-teal-200 bg-teal-50 text-teal-700 dark:border-teal-900 dark:bg-teal-950/60 dark:text-teal-300",
	"border-pink-200 bg-pink-50 text-pink-700 dark:border-pink-900 dark:bg-pink-950/60 dark:text-pink-300",
	"border-indigo-200 bg-indigo-50 text-indigo-700 dark:border-indigo-900 dark:bg-indigo-950/60 dark:text-indigo-300",
	"border-yellow-200 bg-yellow-50 text-yellow-700 dark:border-yellow-900 dark:bg-yellow-950/60 dark:text-yellow-300",
] as const;

// ESAP-TY-0001 passed through here. The about page keeps quiet records.
type ReleaseChannel =
	| "release"
	| "development"
	| "alpha"
	| "beta"
	| "rc"
	| "unknown";

const RELEASE_CHANNELS: readonly ReleaseChannel[] = [
	"release",
	"development",
	"alpha",
	"beta",
	"rc",
	"unknown",
];

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
	const versionClickCountRef = useRef(0);
	const appVersion = config.appVersion;
	const displayVersion = formatDisplayVersion(appVersion);
	const releaseChannel = resolveReleaseChannel(appVersion);
	const [versionBadgeClassIndex, setVersionBadgeClassIndex] = useState(0);
	const [displayReleaseChannel, setDisplayReleaseChannel] =
		useState(releaseChannel);
	const topReleaseChannel = displayReleaseChannel;
	const handleVersionClick = useCallback(() => {
		setVersionBadgeClassIndex(
			(current) => (current + 1) % VERSION_BADGE_CLASSES.length,
		);
		setDisplayReleaseChannel(
			RELEASE_CHANNELS[Math.floor(Math.random() * RELEASE_CHANNELS.length)] ??
				releaseChannel,
		);
		versionClickCountRef.current += 1;
		if (versionClickCountRef.current < VERSION_EASTER_EGG_CLICKS) return;

		versionClickCountRef.current = 0;
		const message =
			VERSION_EASTER_EGG_MESSAGES[
				Math.floor(Math.random() * VERSION_EASTER_EGG_MESSAGES.length)
			];
		toast.info(message);
	}, [releaseChannel]);

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
							{/* If this badge ever claims perfection, check the build pipeline first. */}
							<div className="flex flex-wrap items-center gap-2">
								<Badge variant="outline">{t("about_product_badge")}</Badge>
								<button
									type="button"
									onClick={handleVersionClick}
									className={cn(
										badgeVariants(),
										"cursor-default",
										VERSION_BADGE_CLASSES[versionBadgeClassIndex],
									)}
									aria-label={t("about_version")}
								>
									{displayVersion}
								</button>
								<Badge
									variant="outline"
									className={getChannelBadgeClass(topReleaseChannel)}
								>
									{t(`about_channel_${topReleaseChannel}`)}
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
									{/* A quiet coordinate for anyone reading source instead of clicking links. */}
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
