import { AsterDriveWordmark } from "@/components/common/AsterDriveWordmark";

export function LoginBrandPanel() {
	return (
		<div className="relative hidden items-center justify-center overflow-hidden bg-gradient-to-br from-neutral-900 via-neutral-800 to-neutral-900 lg:flex lg:w-1/2">
			<div className="absolute inset-0 opacity-[0.03]">
				<div className="absolute top-1/4 left-1/4 size-96 rounded-full bg-emerald-500 blur-3xl" />
				<div className="absolute right-1/4 bottom-1/4 size-80 rounded-full bg-amber-500 blur-3xl" />
			</div>
			<div className="relative max-w-md px-12 text-center">
				<AsterDriveWordmark
					alt="AsterDrive"
					className="mx-auto h-24 w-auto"
					surfaceTheme="dark"
				/>
				<p className="text-lg leading-relaxed text-white/50">
					Your files, your server, your rules.
				</p>
			</div>
		</div>
	);
}
