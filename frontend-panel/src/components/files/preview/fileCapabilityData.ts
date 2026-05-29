import { PREVIEW_APP_ICON_URLS } from "@/components/common/previewAppIconUrls";
import type { IconName } from "@/components/ui/icon";
import {
	BUILTIN_TABLE_PREVIEW_APP_KEY,
	DEFAULT_TABLE_PREVIEW_DELIMITER,
} from "@/lib/tablePreview";
import type { FileTypeInfo, OpenWithOption } from "./types";

export const TEXT_EXTENSIONS = new Set([
	// Plain text & markup
	"txt",
	"md",
	"markdown",
	"log",
	"env",
	"ini",
	"conf",
	"cfg",
	"toml",
	"yaml",
	"yml",
	"json",
	"xml",
	"csv",
	"tsv",
	"rst",
	"tex",
	"bib",
	"adoc",
	// Web
	"html",
	"htm",
	"css",
	"scss",
	"less",
	"svg",
	"vue",
	"svelte",
	"astro",
	// JavaScript / TypeScript
	"js",
	"jsx",
	"ts",
	"tsx",
	"mjs",
	"cjs",
	"coffee",
	// Systems languages
	"c",
	"h",
	"cpp",
	"hpp",
	"cs",
	"rs",
	"go",
	"dart",
	"zig",
	"nim",
	"d",
	"asm",
	// JVM
	"java",
	"kt",
	"kts",
	"scala",
	"sbt",
	"groovy",
	"clj",
	"cljs",
	// Scripting
	"py",
	"rb",
	"php",
	"pl",
	"pm",
	"lua",
	"r",
	"jl",
	"vim",
	"el",
	// Shell
	"sh",
	"bash",
	"zsh",
	"fish",
	"ps1",
	"psm1",
	"bat",
	"cmd",
	// Functional
	"hs",
	"ex",
	"exs",
	"erl",
	// Query / data
	"sql",
	"graphql",
	"gql",
	"proto",
	"prisma",
	// IaC / config
	"tf",
	"tfvars",
	"hcl",
	"properties",
	"cmake",
	"gradle",
	// Hardware
	"v",
	"sv",
	"vhd",
	"vhdl",
	// Web3
	"sol",
	// VCS
	"diff",
	"patch",
]);

export const IMAGE_EXTENSIONS = new Set([
	"avif",
	"bmp",
	"gif",
	"heic",
	"heif",
	"ico",
	"jfif",
	"jpe",
	"jpeg",
	"jpg",
	"png",
	"svg",
	"tif",
	"tiff",
	"webp",
]);

export const SPECIAL_TEXT_FILENAMES = new Map<string, string>([
	["dockerfile", "dockerfile"],
	["makefile", "plaintext"],
	[".gitignore", "plaintext"],
	[".env", "plaintext"],
	[".dockerignore", "plaintext"],
	[".editorconfig", "ini"],
	[".gitattributes", "plaintext"],
	[".gitmodules", "plaintext"],
	[".npmrc", "ini"],
	[".npmignore", "plaintext"],
	["jenkinsfile", "groovy"],
	["vagrantfile", "ruby"],
	["gemfile", "ruby"],
	["rakefile", "ruby"],
	["procfile", "plaintext"],
	[".mailmap", "plaintext"],
]);

export const LANGUAGE_BY_EXTENSION: Record<string, string> = {
	// Web
	js: "javascript",
	jsx: "javascript",
	ts: "typescript",
	tsx: "typescript",
	mjs: "javascript",
	cjs: "javascript",
	coffee: "coffeescript",
	html: "html",
	htm: "html",
	css: "css",
	scss: "scss",
	less: "less",
	svg: "xml",
	vue: "html",
	svelte: "html",
	astro: "html",
	// Data / markup
	json: "json",
	xml: "xml",
	yaml: "yaml",
	yml: "yaml",
	toml: "toml",
	md: "markdown",
	markdown: "markdown",
	rst: "restructuredtext",
	tex: "plaintext",
	bib: "plaintext",
	adoc: "plaintext",
	csv: "plaintext",
	tsv: "plaintext",
	// Systems
	c: "c",
	h: "c",
	cpp: "cpp",
	hpp: "cpp",
	cs: "csharp",
	rs: "rust",
	go: "go",
	dart: "dart",
	zig: "plaintext",
	nim: "plaintext",
	d: "plaintext",
	asm: "plaintext",
	// JVM
	java: "java",
	kt: "kotlin",
	kts: "kotlin",
	scala: "scala",
	sbt: "scala",
	groovy: "groovy",
	clj: "clojure",
	cljs: "clojure",
	// Scripting
	py: "python",
	rb: "ruby",
	php: "php",
	pl: "perl",
	pm: "perl",
	lua: "lua",
	r: "r",
	jl: "julia",
	vim: "plaintext",
	el: "plaintext",
	// Shell
	sh: "shell",
	bash: "shell",
	zsh: "shell",
	fish: "shell",
	ps1: "powershell",
	psm1: "powershell",
	bat: "bat",
	cmd: "bat",
	swift: "swift",
	// Functional
	hs: "plaintext",
	ex: "elixir",
	exs: "elixir",
	erl: "plaintext",
	// Query / schema
	sql: "sql",
	graphql: "graphql",
	gql: "graphql",
	proto: "protobuf",
	prisma: "plaintext",
	// IaC / config
	tf: "hcl",
	tfvars: "hcl",
	hcl: "hcl",
	properties: "ini",
	cmake: "plaintext",
	gradle: "java",
	// Hardware
	v: "verilog",
	sv: "systemverilog",
	vhd: "plaintext",
	vhdl: "plaintext",
	// Web3
	sol: "sol",
	// VCS
	diff: "plaintext",
	patch: "plaintext",
	// Plain text fallbacks
	log: "plaintext",
	env: "plaintext",
	ini: "ini",
	conf: "plaintext",
	cfg: "plaintext",
	txt: "plaintext",
};

export const DOCUMENT_MIME_TYPES = new Map<
	string,
	{ icon: IconName; color: string }
>([
	["application/pdf", { icon: "FileText", color: "text-red-500" }],
	["application/json", { icon: "BracketsCurly", color: "text-amber-500" }],
	["application/msword", { icon: "FileText", color: "text-blue-500" }],
	["application/vnd.ms-excel", { icon: "Table", color: "text-green-600" }],
	[
		"application/vnd.ms-powerpoint",
		{ icon: "Presentation", color: "text-orange-500" },
	],
]);

export const PREFIX_TYPE_INFO: Array<
	[
		string,
		{ category: FileTypeInfo["category"]; icon: IconName; color: string },
	]
> = [
	[
		"application/vnd.openxmlformats-officedocument.wordprocessingml",
		{ category: "document", icon: "FileText", color: "text-blue-500" },
	],
	[
		"application/vnd.openxmlformats-officedocument.spreadsheetml",
		{ category: "spreadsheet", icon: "Table", color: "text-green-600" },
	],
	[
		"application/vnd.openxmlformats-officedocument.presentationml",
		{
			category: "presentation",
			icon: "Presentation",
			color: "text-orange-500",
		},
	],
	["image/", { category: "image", icon: "FileImage", color: "text-sky-500" }],
	[
		"video/",
		{ category: "video", icon: "FileVideo", color: "text-purple-500" },
	],
	["audio/", { category: "audio", icon: "FileAudio", color: "text-pink-500" }],
	["text/", { category: "text", icon: "FileCode", color: "text-slate-500" }],
	[
		"application/zip",
		{ category: "archive", icon: "FileZip", color: "text-yellow-600" },
	],
	[
		"application/x-zip-compressed",
		{ category: "archive", icon: "FileZip", color: "text-yellow-600" },
	],
	[
		"application/x-tar",
		{ category: "archive", icon: "FileZip", color: "text-yellow-600" },
	],
	[
		"application/gzip",
		{ category: "archive", icon: "FileZip", color: "text-yellow-600" },
	],
	[
		"application/x-rar",
		{ category: "archive", icon: "FileZip", color: "text-yellow-600" },
	],
	[
		"application/x-7z",
		{ category: "archive", icon: "FileZip", color: "text-yellow-600" },
	],
	[
		"application/x-7z-compressed",
		{ category: "archive", icon: "FileZip", color: "text-yellow-600" },
	],
];

export const DEFAULT_TYPE_INFO: FileTypeInfo = {
	category: "unknown",
	icon: "File",
	color: "text-muted-foreground",
};

const GOOGLE_VIEWER_CONFIG = {
	allowed_origins: ["https://docs.google.com"],
	mode: "iframe",
	url_template:
		"https://docs.google.com/gview?embedded=true&url={{file_preview_url}}",
} as const;

const MICROSOFT_VIEWER_CONFIG = {
	allowed_origins: ["https://view.officeapps.live.com"],
	mode: "iframe",
	url_template:
		"https://view.officeapps.live.com/op/embed.aspx?src={{file_preview_url}}",
} as const;

export const BUILTIN_PREVIEW_OPTIONS: Record<string, OpenWithOption[]> = {
	image: [
		{
			key: "builtin.image",
			mode: "image",
			labelKey: "open_with_image",
			icon: PREVIEW_APP_ICON_URLS.image,
		},
	],
	video: [
		{
			key: "builtin.video",
			mode: "video",
			labelKey: "open_with_video",
			icon: PREVIEW_APP_ICON_URLS.video,
		},
	],
	audio: [
		{
			key: "builtin.audio",
			mode: "audio",
			labelKey: "open_with_audio",
			icon: PREVIEW_APP_ICON_URLS.audio,
		},
	],
	pdf: [
		{
			key: "builtin.pdf",
			mode: "pdf",
			labelKey: "open_with_pdf",
			icon: PREVIEW_APP_ICON_URLS.pdf,
		},
	],
	document: [
		{
			key: "builtin.office_microsoft",
			mode: "url_template",
			labelKey: "open_with_office_microsoft",
			icon: PREVIEW_APP_ICON_URLS.microsoftOnedrive,
			config: MICROSOFT_VIEWER_CONFIG,
		},
		{
			key: "builtin.office_google",
			mode: "url_template",
			labelKey: "open_with_office_google",
			icon: PREVIEW_APP_ICON_URLS.googleDrive,
			config: GOOGLE_VIEWER_CONFIG,
		},
	],
	spreadsheet: [
		{
			key: "builtin.office_microsoft",
			mode: "url_template",
			labelKey: "open_with_office_microsoft",
			icon: PREVIEW_APP_ICON_URLS.microsoftOnedrive,
			config: MICROSOFT_VIEWER_CONFIG,
		},
		{
			key: "builtin.office_google",
			mode: "url_template",
			labelKey: "open_with_office_google",
			icon: PREVIEW_APP_ICON_URLS.googleDrive,
			config: GOOGLE_VIEWER_CONFIG,
		},
	],
	presentation: [
		{
			key: "builtin.office_microsoft",
			mode: "url_template",
			labelKey: "open_with_office_microsoft",
			icon: PREVIEW_APP_ICON_URLS.microsoftOnedrive,
			config: MICROSOFT_VIEWER_CONFIG,
		},
		{
			key: "builtin.office_google",
			mode: "url_template",
			labelKey: "open_with_office_google",
			icon: PREVIEW_APP_ICON_URLS.googleDrive,
			config: GOOGLE_VIEWER_CONFIG,
		},
	],
	markdown: [
		{
			key: "builtin.markdown",
			mode: "markdown",
			labelKey: "open_with_markdown",
			icon: PREVIEW_APP_ICON_URLS.markdown,
		},
		{
			key: "builtin.code",
			mode: "code",
			labelKey: "open_with_code",
			icon: PREVIEW_APP_ICON_URLS.code,
		},
	],
	csv: [
		{
			key: BUILTIN_TABLE_PREVIEW_APP_KEY,
			mode: "table",
			labelKey: "open_with_table",
			icon: PREVIEW_APP_ICON_URLS.table,
			config: { delimiter: DEFAULT_TABLE_PREVIEW_DELIMITER },
		},
		{
			key: "builtin.code",
			mode: "code",
			labelKey: "open_with_code",
			icon: PREVIEW_APP_ICON_URLS.code,
		},
	],
	tsv: [
		{
			key: BUILTIN_TABLE_PREVIEW_APP_KEY,
			mode: "table",
			labelKey: "open_with_table",
			icon: PREVIEW_APP_ICON_URLS.table,
			config: { delimiter: DEFAULT_TABLE_PREVIEW_DELIMITER },
		},
		{
			key: "builtin.code",
			mode: "code",
			labelKey: "open_with_code",
			icon: PREVIEW_APP_ICON_URLS.code,
		},
	],
	json: [
		{
			key: "builtin.formatted",
			mode: "formatted",
			labelKey: "open_with_formatted",
			icon: PREVIEW_APP_ICON_URLS.json,
		},
		{
			key: "builtin.code",
			mode: "code",
			labelKey: "open_with_code",
			icon: PREVIEW_APP_ICON_URLS.code,
		},
	],
	xml: [
		{
			key: "builtin.formatted",
			mode: "formatted",
			labelKey: "open_with_formatted",
			icon: PREVIEW_APP_ICON_URLS.xml,
		},
		{
			key: "builtin.code",
			mode: "code",
			labelKey: "open_with_code",
			icon: PREVIEW_APP_ICON_URLS.code,
		},
	],
	text: [
		{
			key: "builtin.code",
			mode: "code",
			labelKey: "open_with_code",
			icon: PREVIEW_APP_ICON_URLS.code,
		},
	],
	archive: [
		{
			key: "builtin.archive",
			mode: "archive",
			labelKey: "open_with_archive",
			icon: PREVIEW_APP_ICON_URLS.archive,
		},
	],
	svg: [
		{
			key: "builtin.image",
			mode: "image",
			labelKey: "open_with_image",
			icon: PREVIEW_APP_ICON_URLS.image,
		},
		{
			key: "builtin.code",
			mode: "code",
			labelKey: "open_with_code",
			icon: PREVIEW_APP_ICON_URLS.code,
		},
	],
};
