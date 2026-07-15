import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { useMusicPlayerStore } from "@/stores/musicPlayerStore";

export function MusicPlayerHeaderButton() {
	const { t } = useTranslation();
	const musicQueue = useMusicPlayerStore((state) => state.queue);
	const musicIsPlaying = useMusicPlayerStore((state) => state.isPlaying);
	const toggleMusicPanel = useMusicPlayerStore((state) => state.togglePanel);

	if (musicQueue.length === 0) return null;

	return (
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
					className={musicIsPlaying ? "size-4 text-primary" : "size-4"}
				/>
			</TooltipTrigger>
			<TooltipContent>{t("files:music_player_open")}</TooltipContent>
		</Tooltip>
	);
}
