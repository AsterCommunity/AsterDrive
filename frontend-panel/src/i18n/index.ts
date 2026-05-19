import i18n, { type ResourceKey } from "i18next";
import { initReactI18next } from "react-i18next";
import { runWhenIdle } from "@/lib/idleTask";

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

const ALL_NAMESPACES = [
	"core",
	"files",
	"auth",
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
const INITIAL_NAMESPACES = [
	"core",
	"files",
	"auth",
	"validation",
	"errors",
	"offline",
	"share",
	"tasks",
] as const;
const DEFERRED_NAMESPACES = ["admin", "webdav", "settings", "search"] as const;

type LocaleNamespace = (typeof ALL_NAMESPACES)[number];

type LocaleModule = { default: ResourceKey };

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

function isLocaleNamespace(namespace: string): namespace is LocaleNamespace {
	return (ALL_NAMESPACES as readonly string[]).includes(namespace);
}

function getLanguageSwitchNamespaces(): LocaleNamespace[] {
	const usedNamespaces = i18n.reportNamespaces?.getUsedNamespaces?.() ?? [];
	return [
		...new Set([
			...INITIAL_NAMESPACES,
			...usedNamespaces.filter(isLocaleNamespace),
		]),
	];
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
) {
	const splitParts = SPLIT_NAMESPACE_PARTS[namespace];
	if (!splitParts) {
		return loadJsonModule(
			`./locales/${lang}/${namespace}.json`,
			FLAT_LOCALE_MODULES,
		);
	}

	const resources = await Promise.all(
		splitParts.map((part) =>
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
	namespaces: readonly LocaleNamespace[] = ALL_NAMESPACES,
) {
	const entries = await Promise.all(
		namespaces.map(async (namespace) => {
			const resources = await loadNamespace(lang, namespace);
			return [namespace, resources] as const;
		}),
	);
	return Object.fromEntries(entries) as Partial<
		Record<LocaleNamespace, ResourceKey>
	>;
}

async function ensureNamespaces(
	language: string,
	namespaces: readonly LocaleNamespace[],
) {
	const lang = normalizeLanguage(language);
	const missing = namespaces.filter(
		(namespace) => !i18n.hasResourceBundle(lang, namespace),
	);
	if (missing.length === 0) return;
	const resources = await loadLocale(lang, missing);
	for (const [namespace, data] of Object.entries(resources)) {
		i18n.addResourceBundle(lang, namespace, data);
	}
}

export async function ensureI18nNamespaces(
	namespaces: readonly LocaleNamespace[],
	language: string = i18n.language,
) {
	await ensureNamespaces(normalizeLanguage(language), namespaces);
}

const pendingDeferredWarmups = new Set<SupportedLanguage>();

function getAlternateLanguage(lang: SupportedLanguage): SupportedLanguage {
	return lang === "zh" ? "en" : "zh";
}

function scheduleDeferredWarmup(lang: SupportedLanguage) {
	if (
		pendingDeferredWarmups.has(lang) ||
		DEFERRED_NAMESPACES.every((namespace) =>
			i18n.hasResourceBundle(lang, namespace),
		)
	) {
		return;
	}

	pendingDeferredWarmups.add(lang);
	runWhenIdle(() => {
		void ensureNamespaces(lang, DEFERRED_NAMESPACES).finally(() => {
			pendingDeferredWarmups.delete(lang);
		});
	});
}

const lang = detectLanguage();
const resources = await loadLocale(lang, INITIAL_NAMESPACES);

i18n.use(initReactI18next).init({
	resources: { [lang]: resources },
	lng: lang,
	fallbackLng: "en",
	defaultNS: "core",
	interpolation: { escapeValue: false },
	react: {
		bindI18nStore: "added",
	},
});

void ensureNamespaces(lang, DEFERRED_NAMESPACES);
scheduleDeferredWarmup(getAlternateLanguage(lang));

// 切换语言时按需加载目标语言包
const _changeLanguage = i18n.changeLanguage.bind(i18n);
i18n.changeLanguage = async (newLang?: string, ...args) => {
	if (newLang) {
		const targetLang = normalizeLanguage(newLang);
		try {
			localStorage.setItem("aster-language", targetLang);
		} catch {
			// ignore storage errors (private browsing, quota)
		}
		await ensureNamespaces(targetLang, getLanguageSwitchNamespaces());
		void ensureNamespaces(targetLang, DEFERRED_NAMESPACES);
		scheduleDeferredWarmup(getAlternateLanguage(targetLang));
		return _changeLanguage(targetLang, ...args);
	}
	return _changeLanguage(newLang, ...args);
};

export default i18n;
