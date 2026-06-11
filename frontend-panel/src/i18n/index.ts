import { createInstance, type ResourceKey } from "i18next";
import { initReactI18next } from "react-i18next";

const i18n = createInstance();

type SupportedLanguage = "en" | "zh";

function normalizeLanguage(language?: string | null): SupportedLanguage {
	return language?.startsWith("zh") ? "zh" : "en";
}

function detectLanguage(): SupportedLanguage {
	try {
		const stored = localStorage.getItem("aster-language");
		if (stored === "en" || stored === "zh") return stored;
	} catch {
		// ignore
	}
	return normalizeLanguage(navigator.language);
}

export const ALL_NAMESPACES = [
	"core",
	"files",
	"auth",
	"login",
	"validation",
	"admin",
	"webdav",
	"settings",
	"share",
	"errors",
	"offline",
	"search",
	"tasks",
] as const;
type LocaleLoadRequest =
	| LocaleNamespace
	| { namespace: LocaleNamespace; parts?: readonly string[] };

const INITIAL_LOCALE_REQUESTS: readonly LocaleLoadRequest[] = [
	{ namespace: "core", parts: ["common"] },
	"login",
	"validation",
	{ namespace: "errors", parts: ["generic", "auth"] },
	"offline",
];
const AUTHENTICATED_SHELL_LOCALE_REQUESTS: readonly LocaleLoadRequest[] = [
	"core",
	"files",
	"tasks",
	"share",
	"search",
	"errors",
	"offline",
];
export type LocaleNamespace = (typeof ALL_NAMESPACES)[number];

type LocaleModule = { default: ResourceKey };
type LoadedLocaleRequest = {
	namespace: LocaleNamespace;
	parts?: readonly string[];
};

const FULL_NAMESPACE = "*";
const loadedLocaleParts = new Map<string, Set<string>>();

const FLAT_LOCALE_MODULES = import.meta.glob<LocaleModule>(
	"./locales/*/*.json",
);
const SPLIT_LOCALE_MODULES = import.meta.glob<LocaleModule>(
	"./locales/*/*/*.json",
);

const SPLIT_NAMESPACE_PARTS: Partial<
	Record<LocaleNamespace, readonly string[]>
> = {
	core: ["common", "appearance", "workspace", "browser", "date-time", "status"],
	files: [
		"actions",
		"versions",
		"upload",
		"listing",
		"batch",
		"trash",
		"storage",
		"preview",
		"music",
		"pdf",
		"open-with",
		"office-preview",
		"archive-preview",
		"editor",
		"video",
		"external-preview",
		"clipboard",
		"info",
		"sort",
	],
	auth: [
		"sign-in",
		"setup",
		"passkeys",
		"external-auth",
		"activation",
		"password-reset",
		"contact-verification",
		"navigation",
	],
	errors: [
		"generic",
		"auth",
		"upload",
		"storage",
		"tasks",
		"thumbnails",
		"avatar",
		"managed-ingress",
		"remote-nodes",
		"workspace",
		"external-auth",
		"search",
		"internal-storage",
		"wopi",
		"validation",
		"error-page",
	],
	admin: [
		"navigation",
		"overview",
		"tasks",
		"settings-common",
		"settings-auth",
		"settings-mail",
		"settings-network",
		"settings-operations",
		"settings-storage",
		"preview-apps",
		"settings-branding",
		"media-processing",
		"audit",
		"about",
		"policies",
		"policy-groups",
		"remote-nodes",
		"external-auth",
		"users",
		"teams",
		"shares-locks-trash",
		"common",
	],
	settings: [
		"overview",
		"appearance",
		"profile",
		"avatar",
		"security",
		"mfa",
		"email",
		"password",
		"passkeys",
		"external-auth",
		"sessions",
		"teams",
		"quick-actions",
	],
	share: ["public-share", "share-dialog", "my-shares"],
	tasks: [
		"common",
		"archive-actions",
		"status-kind",
		"progress",
		"steps",
		"summary",
		"pagination",
	],
};

function normalizeLocaleRequest(
	request: LocaleLoadRequest,
): LoadedLocaleRequest {
	return typeof request === "string" ? { namespace: request } : request;
}

function loadedPartsKey(lang: SupportedLanguage, namespace: LocaleNamespace) {
	return `${lang}:${namespace}`;
}

function getLoadedParts(lang: SupportedLanguage, namespace: LocaleNamespace) {
	const key = loadedPartsKey(lang, namespace);
	let loaded = loadedLocaleParts.get(key);
	if (!loaded) {
		loaded = new Set();
		loadedLocaleParts.set(key, loaded);
	}
	return loaded;
}

function rememberLoadedParts(
	lang: SupportedLanguage,
	namespace: LocaleNamespace,
	parts?: readonly string[],
) {
	const splitParts = SPLIT_NAMESPACE_PARTS[namespace];
	const loaded = getLoadedParts(lang, namespace);
	if (!splitParts || !parts) {
		loaded.add(FULL_NAMESPACE);
		return;
	}
	for (const part of parts) loaded.add(part);
	if (splitParts.every((part) => loaded.has(part))) {
		loaded.add(FULL_NAMESPACE);
	}
}

function getMissingParts(
	lang: SupportedLanguage,
	namespace: LocaleNamespace,
	parts?: readonly string[],
) {
	const splitParts = SPLIT_NAMESPACE_PARTS[namespace];
	if (!splitParts) {
		return i18n.hasResourceBundle(lang, namespace) ? [] : undefined;
	}

	const loaded = getLoadedParts(lang, namespace);
	if (loaded.has(FULL_NAMESPACE)) return [];

	const requestedParts = parts ?? splitParts;
	return requestedParts.filter((part) => !loaded.has(part));
}

async function loadJsonModule(
	path: string,
	modules: Record<string, () => Promise<LocaleModule>>,
) {
	const loader = modules[path];
	if (!loader) {
		throw new Error(`Missing i18n locale module: ${path}`);
	}
	return (await loader()).default;
}

async function loadNamespace(
	lang: SupportedLanguage,
	namespace: LocaleNamespace,
	parts?: readonly string[],
) {
	const splitParts = SPLIT_NAMESPACE_PARTS[namespace];
	if (!splitParts) {
		return loadJsonModule(
			`./locales/${lang}/${namespace}.json`,
			FLAT_LOCALE_MODULES,
		);
	}

	const requestedParts = parts ?? splitParts;
	const resources = await Promise.all(
		requestedParts.map((part) =>
			loadJsonModule(
				`./locales/${lang}/${namespace}/${part}.json`,
				SPLIT_LOCALE_MODULES,
			),
		),
	);
	const merged: ResourceKey = {};
	for (const resource of resources) {
		for (const [key, value] of Object.entries(resource)) {
			if (key in merged) {
				throw new Error(
					`Duplicate i18n key "${key}" in ${lang}/${namespace} split locale files`,
				);
			}
			merged[key] = value;
		}
	}
	return merged;
}

async function loadLocale(
	lang: SupportedLanguage,
	requests: readonly LocaleLoadRequest[] = ALL_NAMESPACES,
) {
	const loadedRequests = requests.map(normalizeLocaleRequest);
	const entries = await Promise.all(
		loadedRequests.map(async ({ namespace, parts }) => {
			const resources = await loadNamespace(lang, namespace, parts);
			return [namespace, resources, parts] as const;
		}),
	);
	return {
		resources: Object.fromEntries(
			entries.map(([namespace, resources]) => [namespace, resources]),
		) as Partial<Record<LocaleNamespace, ResourceKey>>,
		loadedRequests: entries.map(([namespace, _resources, parts]) => ({
			namespace,
			parts,
		})),
	};
}

async function ensureLocaleRequests(
	language: string,
	requests: readonly LocaleLoadRequest[],
) {
	const lang = normalizeLanguage(language);
	const requestedLocaleRequests = requests.map(normalizeLocaleRequest);
	const missingRequests = requestedLocaleRequests.reduce<LoadedLocaleRequest[]>(
		(acc, { namespace, parts }) => {
			const missingParts = getMissingParts(lang, namespace, parts);
			if (missingParts && missingParts.length === 0) return acc;
			acc.push({ namespace, parts: missingParts });
			return acc;
		},
		[],
	);
	if (missingRequests.length === 0) return;

	const { resources, loadedRequests } = await loadLocale(lang, missingRequests);
	for (const [namespace, data] of Object.entries(resources)) {
		i18n.addResourceBundle(lang, namespace, data, true, true);
	}
	for (const { namespace, parts } of loadedRequests) {
		rememberLoadedParts(lang, namespace, parts);
	}
}

async function ensureNamespaces(
	language: string,
	namespaces: readonly LocaleNamespace[],
) {
	await ensureLocaleRequests(language, namespaces);
}

export async function ensureI18nNamespaces(
	namespaces: readonly LocaleNamespace[],
	language: string = i18n.language,
) {
	await ensureNamespaces(normalizeLanguage(language), namespaces);
}

export async function ensureAuthenticatedShellI18nNamespaces(
	language: string = i18n.language,
) {
	await ensureLocaleRequests(
		normalizeLanguage(language),
		AUTHENTICATED_SHELL_LOCALE_REQUESTS,
	);
}

const allNamespaceLoadPromises = new Map<SupportedLanguage, Promise<void>>();

export function ensureAllI18nNamespaces(language: string = i18n.language) {
	const lang = normalizeLanguage(language);
	const existing = allNamespaceLoadPromises.get(lang);
	if (existing) return existing;

	const promise = ensureNamespaces(lang, ALL_NAMESPACES).catch((error) => {
		allNamespaceLoadPromises.delete(lang);
		throw error;
	});
	allNamespaceLoadPromises.set(lang, promise);
	return promise;
}

const lang = detectLanguage();
const initialLocale = await loadLocale(lang, INITIAL_LOCALE_REQUESTS);

await i18n.use(initReactI18next).init({
	resources: { [lang]: initialLocale.resources },
	lng: lang,
	fallbackLng: "en",
	defaultNS: "core",
	interpolation: { escapeValue: false },
	react: {
		bindI18nStore: "added",
	},
});
for (const { namespace, parts } of initialLocale.loadedRequests) {
	rememberLoadedParts(lang, namespace, parts);
}

const _changeLanguage = i18n.changeLanguage.bind(i18n);
i18n.changeLanguage = async (newLang?: string, ...args) => {
	if (newLang) {
		const targetLang = normalizeLanguage(newLang);
		try {
			localStorage.setItem("aster-language", targetLang);
		} catch {
			// ignore storage errors (private browsing, quota)
		}
		await ensureAllI18nNamespaces(targetLang);
		return _changeLanguage(targetLang, ...args);
	}
	return _changeLanguage(newLang, ...args);
};

export default i18n;
