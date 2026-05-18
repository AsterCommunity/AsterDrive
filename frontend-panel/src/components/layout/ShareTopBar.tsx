import { useTranslation } from "react-i18next";
import { AsterDriveWordmark } from "@/components/common/AsterDriveWordmark";
import { TopBarShell } from "@/components/layout/TopBarShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { useMusicPlayerStore } from "@/stores/musicPlayerStore";

export function ShareTopBar() {
	const { t } = useTranslation();
	const musicQueue = useMusicPlayerStore((s) => s.queue);
	const musicIsPlaying = useMusicPlayerStore((s) => s.isPlaying);
	const toggleMusicPanel = useMusicPlayerStore((s) => s.togglePanel);

	return (
		<TopBarShell
			heightClassName="h-14"
			left={
				<AsterDriveWordmark
					alt={t("app_name")}
					className="h-11 w-auto shrink-0 sm:h-12"
				/>
			}
			right={
				<div className="flex items-center gap-2">
					{musicQueue.length > 0 ? (
						<Tooltip>
							<TooltipTrigger
								render={
									<Button
										type="button"
										variant="ghost"
										size="icon-sm"
										className="rounded-full"
										onClick={toggleMusicPanel}
										aria-label={t("files:music_player_open")}
										data-music-player-trigger
									/>
								}
							>
								<Icon
									name={musicIsPlaying ? "MusicNotes" : "VinylRecord"}
									className={
										musicIsPlaying ? "h-4 w-4 text-primary" : "h-4 w-4"
									}
								/>
							</TooltipTrigger>
							<TooltipContent>{t("files:music_player_open")}</TooltipContent>
						</Tooltip>
					) : null}
					<span className="sr-only">{t("files:share")}</span>
				</div>
			}
		/>
	);
}
