import { readFileSync } from 'node:fs'
import { resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vitepress'

const __dirname = dirname(fileURLToPath(import.meta.url))
const SITE_URL = 'https://asterdrive.docs.esap.cc/'
const SITE_DESCRIPTION =
  'AsterDrive 官方文档中心，覆盖快速开始、日常使用、管理员配置、Docker/systemd 部署、备份恢复、WebDAV、WOPI 和远程节点。'

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
    .replace(/"([^"]+)"/g, '“$1”')
    .replace(/"/g, '”')
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

export default defineConfig({
  lang: 'zh-CN',
  title: 'AsterDrive',
  description: SITE_DESCRIPTION,
  lastUpdated: true,
  sitemap: {
    hostname: SITE_URL
  },

  locales: {
    root: {
      label: '简体中文',
      lang: 'zh-CN',
      themeConfig: {
        nav: [
          { text: '首页', link: '/' },
          {
            text: '开始',
            items: [
              { text: '快速开始', link: '/guide/getting-started' },
              { text: '选择部署方式', link: '/deployment/' },
              { text: '首次启动检查', link: '/deployment/runtime-behavior' }
            ]
          },
          {
            text: '使用',
            items: [
              { text: '使用指南总览', link: '/guide/' },
              { text: '用户手册', link: '/guide/user-guide' },
              { text: '常用流程', link: '/guide/core-workflows' },
              { text: '团队与权限', link: '/guide/teams-and-permissions' },
              { text: '分享与公开访问', link: '/guide/sharing' },
              { text: '文件编辑', link: '/guide/editing' },
              { text: '在线预览与 WOPI', link: '/guide/preview-and-wopi' },
              { text: '上传与大文件', link: '/guide/upload-modes' },
              { text: 'WebDAV', link: '/config/webdav' }
            ]
          },
          {
            text: '管理',
            items: [
              { text: '管理后台', link: '/guide/admin-console' },
              { text: '配置总览', link: '/config/' },
              { text: '系统设置', link: '/config/runtime' },
              { text: '存储策略', link: '/config/storage' },
              { text: '存储策略后端', link: '/storage/' },
              { text: '邮件', link: '/config/mail' },
              { text: '远程节点', link: '/guide/remote-nodes' }
            ]
          },
          {
            text: '运维',
            items: [
              { text: '部署概览', link: '/deployment/' },
              { text: 'Docker', link: '/deployment/docker' },
              { text: 'systemd', link: '/deployment/systemd' },
              { text: '反向代理', link: '/deployment/reverse-proxy' },
              { text: '监控与 Grafana', link: '/deployment/monitoring' },
              { text: '上线检查', link: '/deployment/production-checklist' },
              { text: '升级', link: '/deployment/upgrade' },
              { text: '备份恢复', link: '/deployment/backup' },
              { text: '故障排查', link: '/deployment/troubleshooting' },
              { text: '运维 CLI', link: '/deployment/ops-cli' }
            ]
          },
          {
            text: `v${VERSION}`,
            items: [
              { text: '更新日志', link: 'https://github.com/AptS-1547/AsterDrive/blob/master/CHANGELOG.md' },
              { text: '发布页面', link: 'https://github.com/AptS-1547/AsterDrive/releases' },
              { text: 'GitHub', link: 'https://github.com/AptS-1547/AsterDrive' }
            ]
          }
        ],
        footer: {
          message: '基于 MIT 许可证发布',
          copyright: 'Copyright © 2026 AptS:1547'
        },
        editLink: {
          pattern: 'https://github.com/AptS-1547/AsterDrive/edit/master/docs/:path',
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
    }
  },

  head: [
    ['meta', { name: 'theme-color', content: '#0F172A' }],
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/favicon.svg' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:locale', content: 'zh_CN' }],
    ['meta', { property: 'og:site_name', content: 'AsterDrive' }],
    ['meta', { name: 'twitter:card', content: 'summary' }]
  ],

  transformHead(context) {
    if (context.page === '404.md') {
      return [['meta', { name: 'robots', content: 'noindex, nofollow' }]]
    }

    const canonicalUrl = new URL(toCanonicalPath(context.page), SITE_URL).href
    const title = context.title || 'AsterDrive'
    const description = context.description || SITE_DESCRIPTION

    return [
      ['link', { rel: 'canonical', href: canonicalUrl }],
      ['meta', { property: 'og:title', content: title }],
      ['meta', { property: 'og:description', content: description }],
      ['meta', { property: 'og:url', content: canonicalUrl }],
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

    sidebar: [
      {
        text: '开始',
        collapsed: false,
        items: [
          { text: '使用指南', link: '/guide/' },
          { text: '快速开始', link: '/guide/getting-started' },
          { text: '部署方式选择', link: '/guide/installation' }
        ]
      },
      {
        text: '日常使用',
        collapsed: false,
        items: [
          { text: '用户手册', link: '/guide/user-guide' },
          { text: '常用流程', link: '/guide/core-workflows' },
          { text: '团队与权限', link: '/guide/teams-and-permissions' },
          { text: '分享与公开访问', link: '/guide/sharing' },
          { text: '文件编辑', link: '/guide/editing' },
          { text: '在线预览与 WOPI', link: '/guide/preview-and-wopi' },
          { text: '上传与大文件', link: '/guide/upload-modes' }
        ]
      },
      {
        text: '管理配置',
        collapsed: true,
        items: [
          { text: '管理后台', link: '/guide/admin-console' },
          { text: '配置总览', link: '/config/' },
          { text: '服务器', link: '/config/server' },
          { text: '数据库', link: '/config/database' },
          { text: '登录与会话', link: '/config/auth' },
          { text: '系统设置', link: '/config/runtime' },
          { text: '邮件', link: '/config/mail' },
          { text: '存储策略', link: '/config/storage' },
          { text: '远程节点', link: '/guide/remote-nodes' },
          { text: 'WebDAV', link: '/config/webdav' },
          { text: '访问限流', link: '/config/rate-limit' },
          { text: '缓存', link: '/config/cache' },
          { text: '日志', link: '/config/logging' }
        ]
      },
      {
        text: '存储策略后端',
        collapsed: true,
        items: [
          { text: '后端总览', link: '/storage/' },
          { text: 'S3 / MinIO / R2', link: '/storage/s3-minio-r2' },
          { text: '远程节点', link: '/storage/remote-follower' }
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
          { text: '首次启动检查', link: '/deployment/runtime-behavior' },
          { text: '监控与 Grafana', link: '/deployment/monitoring' },
          { text: '生产上线检查', link: '/deployment/production-checklist' },
          { text: '运维 CLI', link: '/deployment/ops-cli' },
          { text: '升级与版本迁移', link: '/deployment/upgrade' },
          { text: '备份与恢复', link: '/deployment/backup' },
          { text: '故障排查', link: '/deployment/troubleshooting' },
          { text: '前端资源缓存', link: '/deployment/frontend-assets' },
          { text: '性能基准与压测', link: '/deployment/performance-benchmarking' }
        ]
      },
      {
        text: '参考与项目',
        collapsed: true,
        items: [
          { text: '常见问题速查', link: '/guide/faq' },
          { text: '术语表', link: '/guide/glossary' },
          { text: '错误码处理', link: '/guide/errors' },
          { text: '自定义前端', link: '/guide/custom-frontend' },
          { text: '文档贡献说明', link: '/guide/docs-contributing' },
          { text: '关于 AsterDrive', link: '/guide/about' }
        ]
      }
    ],

    socialLinks: [
      { icon: 'github', link: 'https://github.com/AptS-1547/AsterDrive' }
    ],

    search: {
      provider: 'local',
      options: {
        locales: {
          zh: {
            translations: {
              button: { buttonText: '搜索文档', buttonAriaLabel: '搜索文档' },
              modal: {
                noResultsText: '无法找到相关结果',
                resetButtonTitle: '清除查询条件',
                footer: { selectText: '选择', navigateText: '切换' }
              }
            }
          }
        }
      }
    }
  },

  markdown: {
    theme: { light: 'vitesse-light', dark: 'vitesse-dark' }
  }
})
