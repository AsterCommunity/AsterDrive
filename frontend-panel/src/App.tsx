import { useEffect } from "react";
import { RouterProvider } from "react-router-dom";
import { Toaster, toast } from "sonner";
import { OfflineBootFallback } from "@/components/layout/OfflineBootFallback";
import { MusicPlayerHost } from "@/components/music/MusicPlayerHost";
import { usePwaUpdate } from "@/hooks/usePwaUpdate";
import { useStorageChangeEvents } from "@/hooks/useStorageChangeEvents";
import i18n from "@/i18n";
import { router } from "@/router";
import { useAuthStore } from "@/stores/authStore";
import { useBrandingStore } from "@/stores/brandingStore";
import {
	resolveActiveDisplayTimeZone,
	useDisplayTimeZoneStore,
} from "@/stores/displayTimeZoneStore";
import { useMediaDataSupportStore } from "@/stores/mediaDataSupportStore";
import { usePreviewAppStore } from "@/stores/previewAppStore";
import { useThemeStore } from "@/stores/themeStore";
import { useThumbnailSupportStore } from "@/stores/thumbnailSupportStore";

function shouldSkipInitialAuthCheck(pathname: string) {
	return pathname === "/login" || pathname.startsWith("/s/");
}

function loadPublicConfig() {
	void useBrandingStore.getState().load();
	void usePreviewAppStore.getState().load();
	void useThumbnailSupportStore.getState().load();
	void useMediaDataSupportStore.getState().load();
}

function consumeExternalAuthSuccessRedirect() {
	const searchParams = new URLSearchParams(window.location.search);
	if (searchParams.get("external_auth") !== "success") return;

	toast.success(i18n.t("auth:login_success"), {
		id: "external-auth-login-success",
	});
	searchParams.delete("external_auth");
	const nextSearch = searchParams.toString();
	window.history.replaceState(
		window.history.state,
		"",
		`${window.location.pathname}${nextSearch ? `?${nextSearch}` : ""}${window.location.hash}`,
	);
}

function App() {
	const checkAuth = useAuthStore((s) => s.checkAuth);
	const isChecking = useAuthStore((s) => s.isChecking);
	const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
	const bootOffline = useAuthStore((s) => s.bootOffline);
	const userRole = useAuthStore((s) => s.user?.role);
	const displayTimeZone = useDisplayTimeZoneStore((s) =>
		resolveActiveDisplayTimeZone(s.preference),
	);
	usePwaUpdate();
	useStorageChangeEvents();

	useEffect(() => {
		loadPublicConfig();
		if (!shouldSkipInitialAuthCheck(window.location.pathname)) {
			checkAuth();
		} else {
			useAuthStore.setState({ isChecking: false });
		}
		useThemeStore.getState().init();
	}, [checkAuth]);

	useEffect(() => {
		if (isChecking || !isAuthenticated) return;

		consumeExternalAuthSuccessRedirect();

		let cancelled = false;

		void import("@/lib/pwaWarmup").then(({ warmupRouteChunks }) => {
			if (cancelled) return;
			warmupRouteChunks(userRole === "admin" ? "admin" : "user");
		});

		return () => {
			cancelled = true;
		};
	}, [isAuthenticated, isChecking, userRole]);

	useEffect(() => {
		document.documentElement.setAttribute(
			"data-display-time-zone",
			displayTimeZone,
		);
		return () => {
			document.documentElement.removeAttribute("data-display-time-zone");
		};
	}, [displayTimeZone]);

	return (
		<>
			{bootOffline ? (
				<OfflineBootFallback />
			) : (
				<RouterProvider router={router} />
			)}
			<Toaster position="bottom-right" richColors swipeDirections={["right"]} />
			<MusicPlayerHost />
		</>
	);
}

export default App;
