import type { ReactNode } from "react";
import { AsterDriveWordmark } from "@/components/common/AsterDriveWordmark";
import { cn } from "@/lib/utils";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import { AuthBrandPanel, useAuthSlogan } from "./AuthBrandPanel";

/**
 * 未认证页统一外壳：左侧品牌区（lg+）+ 右侧 360px 内容列垂直居中。
 * 移动端无品牌区，内容列顶部给 wordmark + slogan 行。
 * `exiting` 供登录页的成功离场过渡使用。
 */
export function AuthPageShell({
	children,
	contentClassName,
	exiting = false,
}: {
	children: ReactNode;
	/** 覆盖内容列宽度等排版（默认 360px），供内容更宽的 setup 页使用 */
	contentClassName?: string;
	exiting?: boolean;
}) {
	const brandTitle = useFrontendConfigStore((s) => s.branding.title);
	const slogan = useAuthSlogan();

	return (
		<div
			className={cn(
				"min-h-screen flex transition-all duration-300 ease-out",
				exiting && "opacity-0 scale-[1.02]",
			)}
		>
			<AuthBrandPanel />

			<div className="flex-1 flex items-center justify-center bg-background p-6">
				<div
					className={cn(
						"login-enter login-enter-delayed w-full max-w-[360px]",
						contentClassName,
					)}
				>
					<div className="mb-8 text-center lg:hidden">
						<AsterDriveWordmark
							alt={brandTitle}
							className="mx-auto h-16 w-auto"
						/>
						<p className="mt-3 text-sm text-muted-foreground">{slogan}</p>
					</div>

					{children}
				</div>
			</div>
		</div>
	);
}
