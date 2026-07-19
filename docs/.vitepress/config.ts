import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vitepress'
import { withMermaid } from 'vitepress-plugin-mermaid'

const __dirname = dirname(fileURLToPath(import.meta.url))
const SITE_URL = 'https://drive.astercosm.com/'
const ZH_SITE_DESCRIPTION =
  'AsterDrive 官方文档中心，覆盖快速开始、日常使用、管理员配置、Docker/systemd 部署、备份恢复、WebDAV、WOPI 和远程节点。'
const EN_SITE_DESCRIPTION =
  'Official AsterDrive documentation covering quick start, daily usage, administrator configuration, Docker/systemd deployment, backup and restore, WebDAV, WOPI, and follower nodes.'

type LocaleKey = 'root' | 'en'

const LOCALES: Record<
  LocaleKey,
  {
    label: string
    lang: string
    prefix: string
    siteDescription: string
    ogLocale: string
  }
> = {
  root: {
    label: '简体中文',
    lang: 'zh-CN',
    prefix: '',
    siteDescription: ZH_SITE_DESCRIPTION,
    ogLocale: 'zh_CN'
  },
  en: {
    label: 'English',
    lang: 'en-US',
    prefix: '/en',
    siteDescription: EN_SITE_DESCRIPTION,
    ogLocale: 'en_US'
  }
}

function getVersion(): string {
  try {
    const cargoPath = resolve(__dirname, '../../Cargo.toml')
    const content = readFileSync(cargoPath, 'utf-8')
    const match = content.match(/^version\s*=\s*"([^"]+)"/m)
    return match ? match[1] : 'unknown'
  } catch {
    return 'unknown'
  }
}

const VERSION = getVersion()
const PAGE_DESCRIPTION_LIMIT = 160

// 参考页从 guide/ 迁到 reference/ 后，旧路径保留自动跳转占位页。
// 这些占位页不进 sitemap，也不让搜索引擎索引。
const LEGACY_REDIRECT_PAGES = new Set(
  ['architecture', 'faq', 'glossary', 'errors', 'docs-contributing', 'about'].flatMap((page) => [
    `guide/${page}.md`,
    `en/guide/${page}.md`
  ])
)
const LEGACY_REDIRECT_URLS = ['architecture', 'faq', 'glossary', 'errors', 'docs-contributing', 'about'].flatMap(
  (page) => [`guide/${page}`, `en/guide/${page}`]
)
const MIN_USEFUL_DESCRIPTION_LENGTH = 24
const descriptionCache = new Map<string, string>()

function toCanonicalPath(page: string): string {
  const normalizedPage = page.replace(/\\/g, '/').replace(/\.md$/, '')

  if (normalizedPage === 'index') {
    return '/'
  }

  if (normalizedPage.endsWith('/index')) {
    return `/${normalizedPage.slice(0, -'/index'.length)}/`
  }

  return `/${normalizedPage}.html`
}

function getLocaleForPage(page: string): LocaleKey {
  return page.replace(/\\/g, '/').startsWith('en/') ? 'en' : 'root'
}

function getBasePage(page: string): string {
  const normalizedPage = page.replace(/\\/g, '/')
  return normalizedPage.startsWith('en/') ? normalizedPage.slice('en/'.length) : normalizedPage
}

function getLocalizedPage(page: string, locale: LocaleKey): string {
  const basePage = getBasePage(page)
  return locale === 'en' ? `en/${basePage}` : basePage
}

function stripFrontmatter(source: string): string {
  const normalizedSource = source.replace(/^\uFEFF/, '')
  const match = normalizedSource.match(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/)
  return match ? normalizedSource.slice(match[0].length) : normalizedSource
}

function normalizeInlineMarkdown(text: string): string {
  return text
    .replace(/!\[([^\]]*)\]\(([^)]+)\)/g, '$1')
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '$1')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/[*_]/g, '')
    .replace(/<[^>]+>/g, '')
    .replace(/\s+/g, ' ')
    .replace(/\s+([，。！？；：,.!?;:])/g, '$1')
    .trim()
}

function truncateDescription(text: string): string {
  if (text.length <= PAGE_DESCRIPTION_LIMIT) {
    return text
  }

  const sliced = text.slice(0, PAGE_DESCRIPTION_LIMIT).replace(/[\s，。！？；：,.!?;:]+$/u, '')
  return `${sliced}…`
}

function extractDescriptionFromMarkdown(source: string): string {
  const lines = stripFrontmatter(source).split(/\r?\n/)
  let shortFallback = ''

  for (let index = 0; index < lines.length; ) {
    const line = lines[index].trim()

    if (!line) {
      index++
      continue
    }

    if (line.startsWith('#')) {
      index++
      continue
    }

    if (/^:::\s*/.test(line)) {
      const customBlockLines: string[] = []
      index++
      while (index < lines.length && !/^\s*:::\s*$/.test(lines[index].trim())) {
        customBlockLines.push(lines[index])
        index++
      }
      if (index < lines.length) {
        index++
      }

      const customBlockDescription = extractDescriptionFromMarkdown(customBlockLines.join('\n'))
      if (customBlockDescription.length >= MIN_USEFUL_DESCRIPTION_LENGTH) {
        return customBlockDescription
      }
      if (customBlockDescription && !shortFallback) {
        shortFallback = customBlockDescription
      }

      continue
    }

    if (/^```/.test(line) || /^~~~/.test(line)) {
      const fence = line.startsWith('```') ? '```' : '~~~'
      index++
      while (index < lines.length && !lines[index].trim().startsWith(fence)) {
        index++
      }
      if (index < lines.length) {
        index++
      }
      continue
    }

    if (/^[>*+\-|]\s/.test(line) || /^\|/.test(line)) {
      index++
      while (index < lines.length && lines[index].trim()) {
        index++
      }
      continue
    }

    const paragraphLines = [line]
    index++

    while (index < lines.length) {
      const nextLine = lines[index].trim()
      if (!nextLine) {
        break
      }
      if (
        nextLine.startsWith('#') ||
        /^:::\s*/.test(nextLine) ||
        /^```/.test(nextLine) ||
        /^~~~/.test(nextLine) ||
        /^[>*+\-|]\s/.test(nextLine) ||
        /^\|/.test(nextLine)
      ) {
        break
      }
      paragraphLines.push(nextLine)
      index++
    }

    const paragraph = normalizeInlineMarkdown(paragraphLines.join(' '))
    if (!paragraph) {
      continue
    }

    if (paragraph.length >= MIN_USEFUL_DESCRIPTION_LENGTH) {
      return truncateDescription(paragraph)
    }

    if (!shortFallback) {
      shortFallback = paragraph
    }
  }

  return shortFallback ? truncateDescription(shortFallback) : ''
}

function getPageDescription(sourceDir: string, relativePath: string): string {
  const absolutePath = resolve(sourceDir, relativePath)
  const cached = descriptionCache.get(absolutePath)
  if (cached !== undefined) {
    return cached
  }

  try {
    const description = extractDescriptionFromMarkdown(readFileSync(absolutePath, 'utf-8'))
    descriptionCache.set(absolutePath, description)
    return description
  } catch {
    descriptionCache.set(absolutePath, '')
    return ''
  }
}

function buildZhNav() {
  return [
    { text: '首页', link: '/' },
    {
      text: '开始',
      items: [
        { text: '快速开始', link: '/guide/getting-started' },
        { text: '选择部署方式', link: '/guide/installation' },
        { text: '首次启动检查', link: '/deployment/runtime-behavior' }
      ]
    },
    {
      text: '使用',
      items: [
        { text: '使用指南总览', link: '/guide/' },
        { text: '用户手册', link: '/guide/user-guide' },
        { text: '分享与公开访问', link: '/guide/sharing' },
        { text: '上传与大文件', link: '/guide/upload-modes' },
        { text: 'WebDAV', link: '/config/webdav' }
      ]
    },
    {
      text: '管理',
      items: [
        { text: '管理后台', link: '/guide/admin-console' },
        { text: '配置总览', link: '/config/' },
        { text: '存储策略', link: '/config/storage' },
        { text: '存储策略后端', link: '/storage/' },
        { text: '远程节点', link: '/guide/remote-nodes' }
      ]
    },
    {
      text: '运维',
      items: [
        { text: '部署概览', link: '/deployment/' },
        { text: 'Docker', link: '/deployment/docker' },
        { text: '反向代理', link: '/deployment/reverse-proxy' },
        { text: '备份恢复', link: '/deployment/backup' },
        { text: '故障排查', link: '/deployment/troubleshooting' }
      ]
    },
    {
      text: `v${VERSION}`,
      items: [
        { text: '更新日志', link: 'https://github.com/AsterCommunity/AsterDrive/blob/master/CHANGELOG.md' },
        { text: '发布页面', link: 'https://github.com/AsterCommunity/AsterDrive/releases' },
        { text: 'GitHub', link: 'https://github.com/AsterCommunity/AsterDrive' }
      ]
    }
  ]
}

function buildEnNav() {
  return [
    { text: 'Home', link: '/en/' },
    {
      text: 'Start',
      items: [
        { text: 'Quick Start', link: '/en/guide/getting-started' },
        { text: 'Choose Deployment', link: '/en/guide/installation' },
        { text: 'First-Start Checklist', link: '/en/deployment/runtime-behavior' }
      ]
    },
    {
      text: 'Use',
      items: [
        { text: 'Guide Overview', link: '/en/guide/' },
        { text: 'User Manual', link: '/en/guide/user-guide' },
        { text: 'Sharing and Public Access', link: '/en/guide/sharing' },
        { text: 'Uploads and Large Files', link: '/en/guide/upload-modes' },
        { text: 'WebDAV', link: '/en/config/webdav' }
      ]
    },
    {
      text: 'Admin',
      items: [
        { text: 'Admin Console', link: '/en/guide/admin-console' },
        { text: 'Configuration Overview', link: '/en/config/' },
        { text: 'Storage Policies', link: '/en/config/storage' },
        { text: 'Storage Backends', link: '/en/storage/' },
        { text: 'Follower Nodes', link: '/en/guide/remote-nodes' }
      ]
    },
    {
      text: 'Operations',
      items: [
        { text: 'Deployment Overview', link: '/en/deployment/' },
        { text: 'Docker', link: '/en/deployment/docker' },
        { text: 'Reverse Proxy', link: '/en/deployment/reverse-proxy' },
        { text: 'Backup and Restore', link: '/en/deployment/backup' },
        { text: 'Troubleshooting', link: '/en/deployment/troubleshooting' }
      ]
    },
    {
      text: `v${VERSION}`,
      items: [
        { text: 'Changelog', link: 'https://github.com/AsterCommunity/AsterDrive/blob/master/CHANGELOG.md' },
        { text: 'Releases', link: 'https://github.com/AsterCommunity/AsterDrive/releases' },
        { text: 'GitHub', link: 'https://github.com/AsterCommunity/AsterDrive' }
      ]
    }
  ]
}

function buildZhSidebar() {
  return [
    {
      text: '开始',
      collapsed: false,
      items: [
        { text: '使用指南', link: '/guide/' },
        { text: '快速开始', link: '/guide/getting-started' },
        { text: '部署方式选择', link: '/guide/installation' },
        { text: '用户手册', link: '/guide/user-guide' },
        { text: '常用流程', link: '/guide/core-workflows' }
      ]
    },
    {
      text: '功能地图',
      collapsed: false,
      items: [
        { text: '功能索引', link: '/features/' },
        { text: '身份与访问', link: '/features/auth-access' },
        { text: '文件与工作空间', link: '/features/files-workspaces' },
        { text: '上传与存储', link: '/features/upload-storage' },
        { text: '预览与处理', link: '/features/preview-processing' },
        { text: '系统与运维', link: '/features/runtime-operations' }
      ]
    },
    {
      text: '管理操作',
      collapsed: true,
      items: [
        { text: '管理后台', link: '/guide/admin-console' },
        { text: '远程节点接入', link: '/guide/remote-nodes' },
        { text: '自定义前端', link: '/guide/custom-frontend' }
      ]
    },
    {
      text: '配置',
      collapsed: true,
      items: [
        {
          text: '启动配置',
          collapsed: false,
          items: [
            { text: '服务器', link: '/config/server' },
            { text: '数据库', link: '/config/database' },
            { text: 'WebDAV 静态配置', link: '/config/webdav' },
            { text: '访问限流', link: '/config/rate-limit' },
            { text: '缓存', link: '/config/cache' },
            { text: '配置同步', link: '/config/config-sync' },
            { text: '日志', link: '/config/logging' }
          ]
        },
        {
          text: '运行时配置',
          collapsed: false,
          items: [
            { text: '配置总览', link: '/config/' },
            { text: '系统设置', link: '/config/runtime' },
            { text: '登录与会话', link: '/config/auth' },
            { text: '外部认证', link: '/config/external-auth' },
            { text: '邮件', link: '/config/mail' },
            { text: '存储策略', link: '/config/storage' },
            { text: '离线下载', link: '/config/offline-download' }
          ]
        }
      ]
    },
    {
      text: '存储后端',
      collapsed: true,
      items: [
        { text: '后端总览', link: '/storage/' },
        { text: '本地磁盘', link: '/storage/local' },
        { text: 'S3 / MinIO / R2', link: '/storage/s3-minio-r2' },
        { text: 'Azure Blob Storage', link: '/storage/azure-blob' },
        { text: '腾讯云 COS', link: '/storage/tencent-cos' },
        { text: 'OneDrive', link: '/storage/onedrive' },
        { text: 'SFTP', link: '/storage/sftp' },
        { text: '远程节点存储策略', link: '/storage/remote-follower' }
      ]
    },
    {
      text: '部署运维',
      collapsed: true,
      items: [
        { text: '部署概览', link: '/deployment/' },
        { text: 'Docker 部署', link: '/deployment/docker' },
        { text: 'Docker 从节点', link: '/deployment/docker-follower' },
        { text: 'systemd', link: '/deployment/systemd' },
        { text: '反向代理', link: '/deployment/reverse-proxy' },
        { text: '从节点网络拓扑', link: '/deployment/follower-network-topologies' },
        { text: '首次启动检查', link: '/deployment/runtime-behavior' },
        { text: '生产上线检查', link: '/deployment/production-checklist' },
        { text: '监控与 Grafana', link: '/deployment/monitoring' },
        { text: '容量规划参考', link: '/deployment/capacity-planning' },
        { text: '运维 CLI', link: '/deployment/ops-cli' },
        { text: '备份与恢复', link: '/deployment/backup' },
        { text: '升级与版本迁移', link: '/deployment/upgrade' },
        { text: '故障排查', link: '/deployment/troubleshooting' },
        { text: '前端资源缓存', link: '/deployment/frontend-assets' },
        { text: '性能基准与压测', link: '/deployment/performance-benchmarking' }
      ]
    },
    {
      text: '参考与项目',
      collapsed: true,
      items: [
        { text: '参考总览', link: '/reference/' },
        { text: '架构概览', link: '/reference/architecture' },
        { text: '常见问题速查', link: '/reference/faq' },
        { text: '术语表', link: '/reference/glossary' },
        { text: '错误码处理', link: '/reference/errors' },
        { text: '文档贡献说明', link: '/reference/docs-contributing' },
        { text: '关于 AsterDrive', link: '/reference/about' }
      ]
    }
  ]
}

function buildEnSidebar() {
  return [
    {
      text: 'Start',
      collapsed: false,
      items: [
        { text: 'Guide Overview', link: '/en/guide/' },
        { text: 'Quick Start', link: '/en/guide/getting-started' },
        { text: 'Choose Deployment', link: '/en/guide/installation' },
        { text: 'User Manual', link: '/en/guide/user-guide' },
        { text: 'Common Workflows', link: '/en/guide/core-workflows' }
      ]
    },
    {
      text: 'Feature Map',
      collapsed: false,
      items: [
        { text: 'Feature Index', link: '/en/features/' },
        { text: 'Identity and Access', link: '/en/features/auth-access' },
        { text: 'Files and Workspaces', link: '/en/features/files-workspaces' },
        { text: 'Uploads and Storage', link: '/en/features/upload-storage' },
        { text: 'Preview and Processing', link: '/en/features/preview-processing' },
        { text: 'System and Operations', link: '/en/features/runtime-operations' }
      ]
    },
    {
      text: 'Admin Workflows',
      collapsed: true,
      items: [
        { text: 'Admin Console', link: '/en/guide/admin-console' },
        { text: 'Follower Node Enrollment', link: '/en/guide/remote-nodes' },
        { text: 'Custom Frontend', link: '/en/guide/custom-frontend' }
      ]
    },
    {
      text: 'Configuration',
      collapsed: true,
      items: [
        {
          text: 'Startup Configuration',
          collapsed: false,
          items: [
            { text: 'Server', link: '/en/config/server' },
            { text: 'Database', link: '/en/config/database' },
            { text: 'WebDAV Static Config', link: '/en/config/webdav' },
            { text: 'Rate Limiting', link: '/en/config/rate-limit' },
            { text: 'Cache', link: '/en/config/cache' },
            { text: 'Configuration Sync', link: '/en/config/config-sync' },
            { text: 'Logging', link: '/en/config/logging' }
          ]
        },
        {
          text: 'Runtime Configuration',
          collapsed: false,
          items: [
            { text: 'Configuration Overview', link: '/en/config/' },
            { text: 'System Settings', link: '/en/config/runtime' },
            { text: 'Login and Sessions', link: '/en/config/auth' },
            { text: 'External Authentication', link: '/en/config/external-auth' },
            { text: 'Mail', link: '/en/config/mail' },
            { text: 'Storage Policies', link: '/en/config/storage' },
            { text: 'Offline Download', link: '/en/config/offline-download' }
          ]
        }
      ]
    },
    {
      text: 'Storage Backends',
      collapsed: true,
      items: [
        { text: 'Backend Overview', link: '/en/storage/' },
        { text: 'Local Disk', link: '/en/storage/local' },
        { text: 'S3 / MinIO / R2', link: '/en/storage/s3-minio-r2' },
        { text: 'Azure Blob Storage', link: '/en/storage/azure-blob' },
        { text: 'Tencent COS', link: '/en/storage/tencent-cos' },
        { text: 'OneDrive', link: '/en/storage/onedrive' },
        { text: 'SFTP', link: '/en/storage/sftp' },
        { text: 'Follower Node Storage Policy', link: '/en/storage/remote-follower' }
      ]
    },
    {
      text: 'Deployment and Operations',
      collapsed: true,
      items: [
        { text: 'Deployment Overview', link: '/en/deployment/' },
        { text: 'Docker Deployment', link: '/en/deployment/docker' },
        { text: 'Docker Follower', link: '/en/deployment/docker-follower' },
        { text: 'systemd', link: '/en/deployment/systemd' },
        { text: 'Reverse Proxy', link: '/en/deployment/reverse-proxy' },
        { text: 'Follower Network Topologies', link: '/en/deployment/follower-network-topologies' },
        { text: 'First-Start Checklist', link: '/en/deployment/runtime-behavior' },
        { text: 'Production Launch Checklist', link: '/en/deployment/production-checklist' },
        { text: 'Monitoring and Grafana', link: '/en/deployment/monitoring' },
        { text: 'Capacity Planning', link: '/en/deployment/capacity-planning' },
        { text: 'Operations CLI', link: '/en/deployment/ops-cli' },
        { text: 'Backup and Restore', link: '/en/deployment/backup' },
        { text: 'Upgrade and Version Migration', link: '/en/deployment/upgrade' },
        { text: 'Troubleshooting', link: '/en/deployment/troubleshooting' },
        { text: 'Frontend Asset Cache', link: '/en/deployment/frontend-assets' },
        { text: 'Performance Baselines and Load Testing', link: '/en/deployment/performance-benchmarking' }
      ]
    },
    {
      text: 'Reference and Project',
      collapsed: true,
      items: [
        { text: 'Reference Overview', link: '/en/reference/' },
        { text: 'Architecture Overview', link: '/en/reference/architecture' },
        { text: 'FAQ', link: '/en/reference/faq' },
        { text: 'Glossary', link: '/en/reference/glossary' },
        { text: 'Error Codes', link: '/en/reference/errors' },
        { text: 'Docs Contribution Guide', link: '/en/reference/docs-contributing' },
        { text: 'About AsterDrive', link: '/en/reference/about' }
      ]
    }
  ]
}

// minisearch 默认按空格/标点分词，中文整句会变成一个 token，导致中文搜索几乎失效。
// 这里把 CJK 连续段切成单字 + 二字组，拉丁词保持原样。
const CJK_SEGMENT = /[㐀-鿿豈-﫿]+/u
const CJK_OR_LATIN_RUN = /[㐀-鿿豈-﫿]+|[^㐀-鿿豈-﫿]+/gu

function tokenizeForSearch(text: string): string[] {
  const tokens: string[] = []
  for (const word of text.split(/[\s\p{P}\p{S}]+/u)) {
    if (!word) {
      continue
    }
    // 中英混合片段（如“从节点follower”）按 CJK 连续段 / 非 CJK 连续段拆开
    for (const segment of word.match(CJK_OR_LATIN_RUN) ?? []) {
      if (!CJK_SEGMENT.test(segment)) {
        tokens.push(segment.toLowerCase())
        continue
      }
      const chars = [...segment]
      for (let index = 0; index < chars.length; index++) {
        tokens.push(chars[index])
        if (index + 1 < chars.length) {
          tokens.push(chars[index] + chars[index + 1])
        }
      }
    }
  }
  return tokens
}

type SidebarItem = {
  text: string
  link?: string
  collapsed?: boolean
  items?: SidebarItem[]
}

type SidebarGroup = {
  text: string
  collapsed?: boolean
  items?: SidebarItem[]
}

function assertUniqueSidebarLinks<T extends SidebarGroup[]>(sidebar: T, locale: string): T {
  const seen = new Map<string, string>()

  function visit(items: SidebarItem[] | undefined, section: string) {
    for (const item of items ?? []) {
      if (!item.link || item.link.startsWith('http')) {
        visit(item.items, `${section} / ${item.text}`)
        continue
      }

      const previous = seen.get(item.link)
      if (previous) {
        throw new Error(
          `Duplicate sidebar link in ${locale}: ${item.link} appears in both "${previous}" and "${section}"`
        )
      }

      seen.set(item.link, section)
      visit(item.items, `${section} / ${item.text}`)
    }
  }

  for (const group of sidebar) {
    visit(group.items, group.text)
  }

  return sidebar
}

export default withMermaid(defineConfig({
  lang: 'zh-CN',
  title: 'AsterDrive',
  description: ZH_SITE_DESCRIPTION,
  lastUpdated: true,
  sitemap: {
    hostname: SITE_URL,
    transformItems(items) {
      return items.filter((item) => {
        const path = item.url.replace(/^\//, '').replace(/\.html$/, '').replace(/\/$/, '')
        return !LEGACY_REDIRECT_URLS.includes(path)
      })
    }
  },

  locales: {
    root: {
      label: LOCALES.root.label,
      lang: LOCALES.root.lang,
      description: LOCALES.root.siteDescription,
      themeConfig: {
        nav: buildZhNav(),
        sidebar: assertUniqueSidebarLinks(buildZhSidebar(), LOCALES.root.lang),
        footer: {
          message: '基于 MIT 许可证发布',
          copyright: 'Copyright © 2026 AptS:1547'
        },
        editLink: {
          pattern: 'https://github.com/AsterCommunity/AsterDrive/edit/master/docs/:path',
          text: '编辑本页'
        },
        docFooter: { prev: '上一页', next: '下一页' },
        outline: { label: '页面导航' },
        lastUpdated: {
          text: '本页编辑于',
          formatOptions: { dateStyle: 'short', timeStyle: 'medium' }
        },
        returnToTopLabel: '回到顶部',
        sidebarMenuLabel: '菜单',
        darkModeSwitchLabel: '主题',
        lightModeSwitchTitle: '切换到浅色模式',
        darkModeSwitchTitle: '切换到深色模式'
      }
    },
    en: {
      label: LOCALES.en.label,
      lang: LOCALES.en.lang,
      link: '/en/',
      description: LOCALES.en.siteDescription,
      themeConfig: {
        nav: buildEnNav(),
        sidebar: assertUniqueSidebarLinks(buildEnSidebar(), LOCALES.en.lang),
        footer: {
          message: 'Released under the MIT License',
          copyright: 'Copyright © 2026 AptS:1547'
        },
        editLink: {
          pattern: 'https://github.com/AsterCommunity/AsterDrive/edit/master/docs/:path',
          text: 'Edit this page'
        },
        docFooter: { prev: 'Previous page', next: 'Next page' },
        outline: { label: 'On this page' },
        lastUpdated: {
          text: 'Last updated',
          formatOptions: { dateStyle: 'medium', timeStyle: 'medium' }
        },
        returnToTopLabel: 'Return to top',
        sidebarMenuLabel: 'Menu',
        darkModeSwitchLabel: 'Theme',
        lightModeSwitchTitle: 'Switch to light mode',
        darkModeSwitchTitle: 'Switch to dark mode'
      }
    }
  },

  head: [
    ['meta', { name: 'theme-color', content: '#0F172A' }],
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/favicon.svg' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:site_name', content: 'AsterDrive' }],
    ['meta', { name: 'twitter:card', content: 'summary' }]
  ],

  transformHead(context) {
    if (context.page === '404.md' || LEGACY_REDIRECT_PAGES.has(context.page)) {
      return [['meta', { name: 'robots', content: 'noindex, nofollow' }]]
    }

    const locale = getLocaleForPage(context.page)
    const canonicalUrl = new URL(toCanonicalPath(context.page), SITE_URL).href
    const rootUrl = new URL(toCanonicalPath(getLocalizedPage(context.page, 'root')), SITE_URL).href
    const enUrl = new URL(toCanonicalPath(getLocalizedPage(context.page, 'en')), SITE_URL).href
    const title = context.title || 'AsterDrive'
    const description = context.description || LOCALES[locale].siteDescription

    return [
      ['link', { rel: 'canonical', href: canonicalUrl }],
      ['link', { rel: 'alternate', hreflang: LOCALES.root.lang, href: rootUrl }],
      ['link', { rel: 'alternate', hreflang: LOCALES.en.lang, href: enUrl }],
      ['link', { rel: 'alternate', hreflang: 'x-default', href: rootUrl }],
      ['meta', { property: 'og:title', content: title }],
      ['meta', { property: 'og:description', content: description }],
      ['meta', { property: 'og:url', content: canonicalUrl }],
      ['meta', { property: 'og:locale', content: LOCALES[locale].ogLocale }],
      ['meta', { property: 'og:locale:alternate', content: LOCALES[locale === 'en' ? 'root' : 'en'].ogLocale }],
      ['meta', { name: 'twitter:title', content: title }],
      ['meta', { name: 'twitter:description', content: description }]
    ]
  },

  transformPageData(pageData, { siteConfig }) {
    if (pageData.description) {
      return undefined
    }

    const inferredDescription = getPageDescription(siteConfig.srcDir, pageData.filePath)
    if (!inferredDescription) {
      return undefined
    }

    return {
      description: inferredDescription
    }
  },

  themeConfig: {
    logo: {
      light: '/asterdrive/asterdrive-dark.svg',
      dark: '/asterdrive/asterdrive-light.svg',
      alt: 'AsterDrive'
    },
    siteTitle: false,

    socialLinks: [
      { icon: 'github', link: 'https://github.com/AsterCommunity/AsterDrive' }
    ],

    search: {
      provider: 'local',
      options: {
        miniSearch: {
          options: {
            tokenize: tokenizeForSearch
          }
        },
        locales: {
          root: {
            translations: {
              button: { buttonText: '搜索文档', buttonAriaLabel: '搜索文档' },
              modal: {
                noResultsText: '无法找到相关结果',
                resetButtonTitle: '清除查询条件',
                footer: { selectText: '选择', navigateText: '切换' }
              }
            }
          },
          en: {
            translations: {
              button: { buttonText: 'Search docs', buttonAriaLabel: 'Search docs' },
              modal: {
                noResultsText: 'No results found',
                resetButtonTitle: 'Reset search',
                footer: { selectText: 'select', navigateText: 'navigate' }
              }
            }
          }
        }
      }
    }
  },

  markdown: {
    theme: { light: 'vitesse-light', dark: 'vitesse-dark' }
  },

  mermaid: {
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
}))
