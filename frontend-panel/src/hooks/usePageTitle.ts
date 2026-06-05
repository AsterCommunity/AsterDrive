import { useEffect } from "react";
import { formatDocumentTitle } from "@/lib/branding";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";

export function usePageTitle(pageTitle?: string | null) {
	const appTitle = useFrontendConfigStore((state) => state.branding.title);

	useEffect(() => {
		if (typeof document === "undefined") return;
		document.title = formatDocumentTitle(appTitle, pageTitle);
	}, [appTitle, pageTitle]);
}
