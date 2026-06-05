import { getPublicSiteUrls, setPublicSiteUrls } from "@/lib/publicSiteUrl";
import { setFrontendSiteUrlState } from "@/stores/frontendConfigStore";

export function syncPublicSiteUrlsAndUpdateStore(
	value: readonly string[] | null | undefined,
) {
	const siteUrl = setPublicSiteUrls(value);
	setFrontendSiteUrlState(siteUrl);
	return getPublicSiteUrls();
}
