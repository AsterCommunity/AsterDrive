import { defineConfig } from 'astro/config'
import starlight from '@astrojs/starlight'
import sitemap from '@astrojs/sitemap'
import rehypeMermaid from '@beoe/rehype-mermaid'
import starlightCopyButton from 'starlight-copy-button'
import starlightLinksValidator from 'starlight-links-validator'
import starlightLlmsTxt from 'starlight-llms-txt'
import starlightOpenAPI, { createOpenAPISidebarGroup } from 'starlight-openapi'
import starlightScrollToTop from 'starlight-scroll-to-top'

const SITE_URL = 'https://drive.astercosm.com'
const DEVELOPER_BASE = '/developer'

type SidebarItem = {
	label: string
	translations?: Record<string, string>
	link?: string
	collapsed?: boolean
	items?: SidebarItem[]
}

const openAPISidebarGroup = createOpenAPISidebarGroup() as SidebarItem

function assertUniqueSidebarLinks<T extends SidebarItem[]>(items: T): T {
	const seen = new Map<string, string>()

	function visit(entries: SidebarItem[] | undefined, section: string) {
		for (const entry of entries ?? []) {
			if (entry.link && !entry.link.startsWith('http')) {
				const previous = seen.get(entry.link)
				if (previous) {
					throw new Error(`Duplicate developer sidebar link: ${entry.link} appears in ${previous} and ${section}`)
				}
				seen.set(entry.link, section)
			}
			visit(entry.items, `${section} / ${entry.label}`)
		}
	}

	visit(items, 'Developer Docs')
	return items
}

const sidebar = assertUniqueSidebarLinks([
	{
		label: '开始',
		translations: { en: 'Start' },
		collapsed: false,
		items: [
			{ label: '开发者文档', translations: { en: 'Developer Docs' }, link: '/' },
			{ label: '用户文档', translations: { en: 'User Documentation' }, link: SITE_URL }
		]
	},
	{
		label: '架构与边界',
		translations: { en: 'Architecture and Boundaries' },
		collapsed: false,
		items: [
			{ label: '架构概览', translations: { en: 'Architecture Overview' }, link: '/architecture/' },
			{ label: '关键模块设计', translations: { en: 'Core Module Design' }, link: '/architecture/module-designs/' },
			{
				label: '后端服务所有权',
				translations: { en: 'Backend Service Ownership' },
				link: '/architecture/backend-service-ownership/'
			}
		]
	},
	{
		label: '领域设计与契约',
		translations: { en: 'Domain Design and Contracts' },
		collapsed: true,
		items: [
			{ label: '设计文档索引', translations: { en: 'Design Index' }, link: '/design/' },
			{ label: '外部认证模块', translations: { en: 'External Authentication' }, link: '/design/external-auth/' },
			{
				label: '远端存储目标归属',
				translations: { en: 'Remote Storage Target Ownership' },
				link: '/design/remote-storage-target-policy-ownership/'
			},
			{
				label: 'Descriptor 规范化',
				translations: { en: 'Descriptor Normalization' },
				link: '/design/storage-descriptor-normalization-contract/'
			},
			{
				label: '对象命名与 OneDrive',
				translations: { en: 'Object Naming and OneDrive' },
				link: '/design/storage-object-naming-and-onedrive-direct-download/'
			},
			{
				label: '上传完成契约',
				translations: { en: 'Upload Finalization Contracts' },
				link: '/design/upload-finalization-contracts/'
			}
		]
	},
	{
		label: 'API Reference',
		collapsed: true,
		items: [
			{ label: 'API 概览', translations: { en: 'API Overview' }, link: '/api/' },
			openAPISidebarGroup,
			{
				label: '身份与公开访问',
				translations: { en: 'Identity and Public Access' },
				collapsed: true,
				items: [
					{ label: '认证', translations: { en: 'Authentication' }, link: '/api/auth/' },
					{ label: '公共接口', translations: { en: 'Public API' }, link: '/api/public/' },
					{ label: '分享', translations: { en: 'Sharing' }, link: '/api/shares/' },
					{ label: '团队与团队空间', translations: { en: 'Teams and Team Spaces' }, link: '/api/teams/' }
				]
			},
			{
				label: '文件工作流',
				translations: { en: 'File Workflows' },
				collapsed: true,
				items: [
					{ label: '文件', translations: { en: 'Files' }, link: '/api/files/' },
					{ label: '文件夹', translations: { en: 'Folders' }, link: '/api/folders/' },
					{ label: '批量操作', translations: { en: 'Batch Operations' }, link: '/api/batch/' },
					{ label: '回收站', translations: { en: 'Trash' }, link: '/api/trash/' },
					{ label: '属性', translations: { en: 'Properties' }, link: '/api/properties/' }
				]
			},
			{
				label: '发现与后台任务',
				translations: { en: 'Discovery and Tasks' },
				collapsed: true,
				items: [
					{ label: '搜索', translations: { en: 'Search' }, link: '/api/search/' },
					{ label: '标签', translations: { en: 'Tags' }, link: '/api/tags/' },
					{ label: '后台任务', translations: { en: 'Background Tasks' }, link: '/api/tasks/' }
				]
			},
			{
				label: '协议与运维',
				translations: { en: 'Protocols and Operations' },
				collapsed: true,
				items: [
					{ label: '管理 API', translations: { en: 'Admin API' }, link: '/api/admin/' },
					{ label: '健康检查', translations: { en: 'Health Checks' }, link: '/api/health/' },
					{ label: 'WebDAV', link: '/api/webdav/' },
					{ label: 'WOPI', link: '/api/wopi/' },
					{
						label: '内部存储协议',
						translations: { en: 'Internal Storage Protocol' },
						link: '/api/internal-storage/'
					}
				]
			}
		]
	},
	{
		label: '测试与诊断',
		translations: { en: 'Testing and Diagnostics' },
		collapsed: true,
		items: [
			{ label: '测试与数据库后端', translations: { en: 'Testing and Database Backends' }, link: '/testing/' },
			{
				label: 'WebDAV 合规测试',
				translations: { en: 'WebDAV Compliance Testing' },
				link: '/testing/webdav-compliance-testing/'
			},
			{
				label: 'Jemalloc 堆画像',
				translations: { en: 'Jemalloc Heap Profiling' },
				link: '/testing/jemalloc-profiling/'
			}
		]
	},
	{
		label: '草稿与历史记录',
		translations: { en: 'Draft and Historical Records' },
		collapsed: true,
		items: [
			{ label: '记录索引', translations: { en: 'Record Index' }, link: '/records/' },
			{
				label: '静态配置密钥（草稿）',
				translations: { en: 'Static Config Secrets (Draft)' },
				link: '/records/static-config-secret-handling/'
			},
			{
				label: '服务模块化（历史）',
				translations: { en: 'Service Modularization (Historical)' },
				link: '/records/service-modularization-refactor-plan/'
			}
		]
	}
])

const movedRoutes = {
	'/module-designs': '/architecture/module-designs',
	'/backend-service-ownership': '/architecture/backend-service-ownership',
	'/external-auth': '/design/external-auth',
	'/remote-storage-target-policy-ownership': '/design/remote-storage-target-policy-ownership',
	'/storage-descriptor-normalization-contract': '/design/storage-descriptor-normalization-contract',
	'/storage-object-naming-and-onedrive-direct-download': '/design/storage-object-naming-and-onedrive-direct-download',
	'/upload-finalization-contracts': '/design/upload-finalization-contracts',
	'/webdav-compliance-testing': '/testing/webdav-compliance-testing',
	'/jemalloc-profiling': '/testing/jemalloc-profiling',
	'/static-config-secret-handling': '/records/static-config-secret-handling',
	'/service-modularization-refactor-plan': '/records/service-modularization-refactor-plan'
}

const redirects = Object.fromEntries(
	Object.entries(movedRoutes).flatMap(([from, to]) => [
		[from, `${DEVELOPER_BASE}${to}`],
		[`/en${from}`, `${DEVELOPER_BASE}/en${to}`]
	])
)

export default defineConfig({
	site: SITE_URL,
	base: DEVELOPER_BASE,
	srcDir: './src-developer',
	publicDir: './src-developer/public',
	outDir: './dist-developer',
	redirects,
	build: { format: 'directory' },
	trailingSlash: 'always',
	markdown: {
		rehypePlugins: [
			[
				rehypeMermaid,
				{
					strategy: 'inline',
					darkScheme: 'class',
					mermaidConfig: {
						theme: 'default',
						themeVariables: {
							fontFamily: 'Inter, ui-sans-serif, system-ui, sans-serif',
							fontSize: '14px',
							primaryColor: '#F8FAFC',
							primaryTextColor: '#0F172A',
							primaryBorderColor: '#CBD5E1',
							lineColor: '#64748B',
							secondaryColor: '#ECFEFF',
							tertiaryColor: '#F1F5F9'
						},
						flowchart: {
							htmlLabels: true,
							nodeSpacing: 28,
							rankSpacing: 34,
							padding: 10
						}
					}
				}
			]
		]
	},
	integrations: [
		starlight({
			title: 'AsterDrive Developer Docs',
			description: 'AsterDrive 源码架构、API、存储契约与测试文档。',
			logo: {
				light: './src/assets/asterdrive-dark.svg',
				dark: './src/assets/asterdrive-light.svg',
				replacesTitle: true
			},
			defaultLocale: 'root',
			locales: {
				root: { label: '简体中文', lang: 'zh-CN' },
				en: { label: 'English', lang: 'en' }
			},
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/AsterCommunity/AsterDrive' }],
			editLink: {
				baseUrl: 'https://github.com/AsterCommunity/AsterDrive/edit/master/developer-docs/'
			},
			lastUpdated: true,
			routeMiddleware: './src/routeMiddleware.ts',
			customCss: ['./src/styles/custom.css'],
			expressiveCode: {
				themes: ['vitesse-dark', 'vitesse-light']
			},
			components: {
				Head: './src/components/Head.astro',
				Header: './src/components/Header.astro',
				MobileMenuFooter: './src/components/MobileMenuFooter.astro',
				PageFrame: './src/components/PageFrame.astro'
			},
			head: [
				{ tag: 'meta', attrs: { name: 'theme-color', content: '#0F172A' } },
				{ tag: 'link', attrs: { rel: 'icon', type: 'image/svg+xml', href: `${DEVELOPER_BASE}/favicon.svg` } },
				{ tag: 'meta', attrs: { name: 'twitter:card', content: 'summary' } }
			],
			plugins: [
				starlightOpenAPI([
					{
						base: 'openapi',
						schema: '../frontend-panel/generated/openapi.json',
						sidebar: {
							label: 'OpenAPI Reference',
							collapsed: true,
							group: openAPISidebarGroup,
							operations: {
								badges: true,
								labels: 'operationId',
								sort: 'document'
							},
							tags: { sort: 'document' }
						},
						snippets: {
							operation: {
								clients: {
									javascript: ['fetch'],
									rust: ['reqwest'],
									shell: ['curl']
								},
								default: { target: 'shell', client: 'curl' }
							}
						}
					}
				]),
				starlightCopyButton({
					label: '复制本页 / Copy page',
					successLabel: '已复制 / Copied',
					errorLabel: '复制失败 / Copy failed',
					stateDuration: 1800,
					iconOnly: true
				}),
				starlightLinksValidator({ errorOnRelativeLinks: false }),
				starlightLlmsTxt(),
				starlightScrollToTop({
					position: 'right',
					tooltipText: {
						'zh-CN': '返回顶部',
						en: 'Scroll to top'
					},
					smoothScroll: true,
					threshold: 300,
					borderRadius: '50',
					showProgressRing: true,
					progressRingColor: 'var(--sl-color-accent)',
					showOnHomepage: true
				})
			],
			sidebar
		}),
		sitemap()
	]
})
