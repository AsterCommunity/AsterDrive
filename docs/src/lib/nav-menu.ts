/** 顶部导航数据。站内链接与 Astro 的 directory + trailingSlash 路由保持一致。 */
export type NavItem = { label: string; link: string }
export type NavEntry = NavItem | { label: string; items: NavItem[] }

const zhNav: NavEntry[] = [
	{ label: '首页', link: '/' },
	{
		label: '开始',
		items: [
			{ label: '快速开始', link: '/guide/getting-started/' },
			{ label: '选择部署方式', link: '/guide/installation/' },
			{ label: '首次启动检查', link: '/deployment/runtime-behavior/' }
		]
	},
	{
		label: '使用',
		items: [
			{ label: '使用指南总览', link: '/guide/' },
			{ label: '用户手册', link: '/guide/user-guide/' },
			{ label: '分享与公开访问', link: '/guide/sharing/' },
			{ label: '上传与大文件', link: '/guide/upload-modes/' },
			{ label: 'WebDAV', link: '/config/webdav/' }
		]
	},
	{
		label: '管理',
		items: [
			{ label: '管理后台', link: '/guide/admin-console/' },
			{ label: '配置总览', link: '/config/' },
			{ label: '存储策略', link: '/config/storage/' },
			{ label: '存储策略后端', link: '/storage/' },
			{ label: '远程节点', link: '/guide/remote-nodes/' }
		]
	},
	{
		label: '运维',
		items: [
			{ label: '部署概览', link: '/deployment/' },
			{ label: 'Docker', link: '/deployment/docker/' },
			{ label: '反向代理', link: '/deployment/reverse-proxy/' },
			{ label: '备份恢复', link: '/deployment/backup/' },
			{ label: '故障排查', link: '/deployment/troubleshooting/' }
		]
	},
	{ label: '开发者', link: '/developer/' }
]

const enNav: NavEntry[] = [
	{ label: 'Home', link: '/en/' },
	{
		label: 'Start',
		items: [
			{ label: 'Quick Start', link: '/en/guide/getting-started/' },
			{ label: 'Choose Deployment', link: '/en/guide/installation/' },
			{ label: 'First-Start Checklist', link: '/en/deployment/runtime-behavior/' }
		]
	},
	{
		label: 'Use',
		items: [
			{ label: 'Guide Overview', link: '/en/guide/' },
			{ label: 'User Manual', link: '/en/guide/user-guide/' },
			{ label: 'Sharing and Public Access', link: '/en/guide/sharing/' },
			{ label: 'Uploads and Large Files', link: '/en/guide/upload-modes/' },
			{ label: 'WebDAV', link: '/en/config/webdav/' }
		]
	},
	{
		label: 'Admin',
		items: [
			{ label: 'Admin Console', link: '/en/guide/admin-console/' },
			{ label: 'Configuration Overview', link: '/en/config/' },
			{ label: 'Storage Policies', link: '/en/config/storage/' },
			{ label: 'Storage Backends', link: '/en/storage/' },
			{ label: 'Follower Nodes', link: '/en/guide/remote-nodes/' }
		]
	},
	{
		label: 'Operations',
		items: [
			{ label: 'Deployment Overview', link: '/en/deployment/' },
			{ label: 'Docker', link: '/en/deployment/docker/' },
			{ label: 'Reverse Proxy', link: '/en/deployment/reverse-proxy/' },
			{ label: 'Backup and Restore', link: '/en/deployment/backup/' },
			{ label: 'Troubleshooting', link: '/en/deployment/troubleshooting/' }
		]
	},
	{ label: 'Developer', link: '/developer/en/' }
]

export function getNav(locale: string | undefined): NavEntry[] {
	return locale === 'en' ? enNav : zhNav
}

/** 把链接规范成可比较的路径形式。 */
function normalize(path: string): string {
	return path.length > 1 ? path.replace(/\/+$/, '') : path
}

/** 当前页面是否命中该链接。 */
export function isActiveLink(currentPath: string, link: string): boolean {
	return normalize(currentPath) === normalize(link)
}
