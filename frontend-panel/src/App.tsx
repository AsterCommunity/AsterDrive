import { lazy, Suspense, useEffect, useState } from "react";
import { RouterProvider } from "react-router-dom";
import { Toaster, toast } from "sonner";
import { usePwaUpdate } from "@/hooks/usePwaUpdate";
import i18n, {
	ensureAllI18nNamespaces,
	ensureAuthenticatedShellI18nNamespaces,
} from "@/i18n";
import { runWhenIdle } from "@/lib/idleTask";
import { logger } from "@/lib/logger";
import { useMusicPlayerHostMountRequested } from "@/lib/musicPlayerMountSignal";
import { router } from "@/router";
import { Loading } from "@/router/Loading";
import { useAuthStore } from "@/stores/authStore";
import {
	resolveActiveDisplayTimeZone,
	useDisplayTimeZoneStore,
} from "@/stores/displayTimeZoneStore";
import { useThemeStore } from "@/stores/themeStore";

const OfflineBootFallback = lazy(() =>
	import("@/components/layout/OfflineBootFallback").then((module) => ({
		default: module.OfflineBootFallback,
	})),
);

const StorageChangeEventsBridge = lazy(() =>
	import("@/hooks/useStorageChangeEvents").then((module) => ({
		default: module.StorageChangeEventsBridge,
	})),
);

const MusicPlayerHost = lazy(() =>
	import("@/components/music/MusicPlayerHost").then((module) => ({
		default: module.MusicPlayerHost,
	})),
);

function shouldSkipInitialAuthCheck(pathname: string) {
	return pathname === "/login" || pathname.startsWith("/s/");
}

function loadPublicConfig() {
	void import("@/stores/frontendConfigStore").then(
		({ initFrontendConfigRuntime, useFrontendConfigStore }) => {
			initFrontendConfigRuntime();
			void useFrontendConfigStore.getState().load();
		},
	);
}

function scheduleSupportConfigLoads() {
	return runWhenIdle(
		() => {
			void import("@/stores/previewAppStore").then(({ usePreviewAppStore }) => {
				void usePreviewAppStore.getState().load();
			});
			void import("@/stores/thumbnailSupportStore").then(
				({ useThumbnailSupportStore }) => {
					void useThumbnailSupportStore.getState().load();
				},
			);
			void import("@/stores/mediaDataSupportStore").then(
				({ useMediaDataSupportStore }) => {
					void useMediaDataSupportStore.getState().load();
				},
			);
		},
		{ fallbackDelayMs: 1_200, timeoutMs: 3_000 },
	);
}

function warmupPwaChunks(role: string | undefined) {
	void import("@/lib/pwaWarmup").then(({ warmupRouteChunks }) => {
		warmupRouteChunks(role === "admin" ? "admin" : "user");
	});
}

async function consumeExternalAuthSuccessRedirect() {
	const searchParams = new URLSearchParams(window.location.search);
	if (searchParams.get("external_auth") !== "success") return;

	await ensureAllI18nNamespaces();
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
	const [authenticatedLocaleReady, setAuthenticatedLocaleReady] =
		useState(false);
	const checkAuth = useAuthStore((s) => s.checkAuth);
	const isChecking = useAuthStore((s) => s.isChecking);
	const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
	const bootOffline = useAuthStore((s) => s.bootOffline);
	const userRole = useAuthStore((s) => s.user?.role);
	const storageEventStreamEnabled = useAuthStore(
		(s) => s.user?.preferences?.storage_event_stream_enabled !== false,
	);
	const displayTimeZone = useDisplayTimeZoneStore((s) =>
		resolveActiveDisplayTimeZone(s.preference),
	);
	const shouldMountMusicPlayer = useMusicPlayerHostMountRequested();
	usePwaUpdate();
	const shouldHoldAuthenticatedRoute =
		isAuthenticated && (isChecking || !authenticatedLocaleReady);

	useEffect(() => {
		const skipInitialAuthCheck = shouldSkipInitialAuthCheck(
			window.location.pathname,
		);
		loadPublicConfig();
		if (!skipInitialAuthCheck) {
			checkAuth();
		} else {
			useAuthStore.setState({ isChecking: false });
		}
		useThemeStore.getState().init();
	}, [checkAuth]);

	useEffect(() => {
		if (isChecking || !isAuthenticated) return;
		return scheduleSupportConfigLoads();
	}, [isAuthenticated, isChecking]);

	useEffect(() => {
		if (isChecking || !isAuthenticated) {
			setAuthenticatedLocaleReady(false);
			return;
		}

		let cancelled = false;
		void (async () => {
			try {
				await ensureAuthenticatedShellI18nNamespaces();
			} catch (error) {
				if (!cancelled) {
					logger.warn("failed to load authenticated locale namespaces", error);
				}
			}
			if (cancelled) return;
			setAuthenticatedLocaleReady(true);
		})();

		return () => {
			cancelled = true;
		};
	}, [isAuthenticated, isChecking]);

	useEffect(() => {
		if (isChecking || !isAuthenticated || !authenticatedLocaleReady) return;
		warmupPwaChunks(userRole);
	}, [authenticatedLocaleReady, isAuthenticated, isChecking, userRole]);

	useEffect(() => {
		if (isChecking || !isAuthenticated) return;

		void consumeExternalAuthSuccessRedirect();
	}, [isAuthenticated, isChecking]);

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
				<Suspense fallback={null}>
					<OfflineBootFallback />
				</Suspense>
			) : shouldHoldAuthenticatedRoute ? (
				<Loading />
			) : (
				<RouterProvider router={router} />
			)}
			{isAuthenticated && !isChecking && storageEventStreamEnabled ? (
				<Suspense fallback={null}>
					<StorageChangeEventsBridge />
				</Suspense>
			) : null}
			<Toaster
				position="bottom-right"
				richColors
				swipeDirections={["right"]}
				style={{ zIndex: "var(--z-toast)" }}
			/>
			{shouldMountMusicPlayer ? (
				<Suspense fallback={null}>
					<MusicPlayerHost />
				</Suspense>
			) : null}
		</>
	);
}

export default App;
