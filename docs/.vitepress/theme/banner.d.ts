// vite define 注入的构建期常量（见 .vitepress/config.ts 的 vite.define）。
// 旧版本 / next 版本构建时带横幅文案，根站点构建为 null。
declare const __ASTER_VERSION_BANNER__: {
	text: string
	url: string
	linkText: string
} | null
