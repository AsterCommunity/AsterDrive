import { useEffect } from "react";
import { RouterProvider } from "react-router-dom";
import { Toaster } from "sonner";
import { OfflineBootFallback } from "@/components/layout/OfflineBootFallback";
import { usePwaUpdate } from "@/hooks/usePwaUpdate";
import { useStorageChangeEvents } from "@/hooks/useStorageChangeEvents";
import { router } from "@/router";
import { useAuthStore } from "@/stores/authStore";
import { useBrandingStore } from "@/stores/brandingStore";
import {
	resolveActiveDisplayTimeZone,
	useDisplayTimeZoneStore,
} from "@/stores/displayTimeZoneStore";
import { usePreviewAppStore } from "@/stores/previewAppStore";
import { useThemeStore } from "@/stores/themeStore";
import { useThumbnailSupportStore } from "@/stores/thumbnailSupportStore";

function shouldSkipInitialAuthCheck(pathname: string) {
	return pathname === "/login" || pathname.startsWith("/s/");
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
		void useBrandingStore.getState().load();
		void usePreviewAppStore.getState().load();
		void useThumbnailSupportStore.getState().load();
		if (!shouldSkipInitialAuthCheck(window.location.pathname)) {
			checkAuth();
		} else {
			useAuthStore.setState({ isChecking: false });
		}
		useThemeStore.getState().init();
	}, [checkAuth]);

	useEffect(() => {
		if (typeof window === "undefined" || typeof document === "undefined") {
			return;
		}

		const revalidatePublicConfig = () => {
			if (document.visibilityState !== "visible") {
				return;
			}
			void useBrandingStore.getState().load();
			void usePreviewAppStore.getState().load();
			void useThumbnailSupportStore.getState().load();
		};

		const interval = window.setInterval(revalidatePublicConfig, 60_000);
		window.addEventListener("focus", revalidatePublicConfig);
		document.addEventListener("visibilitychange", revalidatePublicConfig);

		return () => {
			window.clearInterval(interval);
			window.removeEventListener("focus", revalidatePublicConfig);
			document.removeEventListener("visibilitychange", revalidatePublicConfig);
		};
	}, []);

	useEffect(() => {
		if (isChecking || !isAuthenticated) return;

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
		</>
	);
}

export default App;
