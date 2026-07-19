import { defineConfig } from 'astro/config'
import starlight from '@astrojs/starlight'
import sitemap from '@astrojs/sitemap'
import rehypeMermaid from '@beoe/rehype-mermaid'
import starlightAnnouncement from 'starlight-announcement'
import starlightCopyButton from 'starlight-copy-button'
import starlightLinksValidator from 'starlight-links-validator'
import starlightLlmsTxt from 'starlight-llms-txt'
import starlightScrollToTop from 'starlight-scroll-to-top'

const SITE_URL = 'https://drive.astercosm.com'
const ZH_SITE_DESCRIPTION =
  'AsterDrive 官方文档中心，覆盖快速开始、日常使用、管理员配置、Docker/systemd 部署、备份恢复、WebDAV、WOPI 和远程节点。'

type SidebarItem = {
  label: string
  translations?: Record<string, string>
  link?: string
  collapsed?: boolean
  items?: SidebarItem[]
}

function assertUniqueSidebarLinks<T extends SidebarItem[]>(sidebar: T): T {
  const seen = new Map<string, string>()

  function visit(items: SidebarItem[] | undefined, section: string) {
    for (const item of items ?? []) {
      if (!item.link || item.link.startsWith('http')) {
        visit(item.items, `${section} / ${item.label}`)
        continue
      }

      const previous = seen.get(item.link)
      if (previous) {
        throw new Error(
          `Duplicate sidebar link: ${item.link} appears in both "${previous}" and "${section} / ${item.label}"`
        )
      }

      seen.set(item.link, section)
      visit(item.items, `${section} / ${item.label}`)
    }
  }

  for (const group of sidebar) {
    visit(group.items, group.label)
  }

  return sidebar
}

const sidebar = assertUniqueSidebarLinks([
  {
    label: '开始',
    translations: { en: 'Start' },
    collapsed: false,
    items: [
      { label: '使用指南', translations: { en: 'Guide Overview' }, link: '/guide/' },
      { label: '快速开始', translations: { en: 'Quick Start' }, link: '/guide/getting-started/' },
      { label: '部署方式选择', translations: { en: 'Choose Deployment' }, link: '/guide/installation/' },
      { label: '用户手册', translations: { en: 'User Manual' }, link: '/guide/user-guide/' },
      { label: '常用流程', translations: { en: 'Common Workflows' }, link: '/guide/core-workflows/' }
    ]
  },
  {
    label: '功能地图',
    translations: { en: 'Feature Map' },
    collapsed: false,
    items: [
      { label: '功能索引', translations: { en: 'Feature Index' }, link: '/features/' },
      { label: '身份与访问', translations: { en: 'Identity and Access' }, link: '/features/auth-access/' },
      { label: '文件与工作空间', translations: { en: 'Files and Workspaces' }, link: '/features/files-workspaces/' },
      { label: '上传与存储', translations: { en: 'Uploads and Storage' }, link: '/features/upload-storage/' },
      { label: '预览与处理', translations: { en: 'Preview and Processing' }, link: '/features/preview-processing/' },
      { label: '系统与运维', translations: { en: 'System and Operations' }, link: '/features/runtime-operations/' }
    ]
  },
  {
    label: '管理操作',
    translations: { en: 'Admin Workflows' },
    collapsed: true,
    items: [
      { label: '管理后台', translations: { en: 'Admin Console' }, link: '/guide/admin-console/' },
      { label: '远程节点接入', translations: { en: 'Follower Node Enrollment' }, link: '/guide/remote-nodes/' },
      { label: '自定义前端', translations: { en: 'Custom Frontend' }, link: '/guide/custom-frontend/' }
    ]
  },
  {
    label: '配置',
    translations: { en: 'Configuration' },
    collapsed: true,
    items: [
      {
        label: '启动配置',
        translations: { en: 'Startup Configuration' },
        collapsed: false,
        items: [
          { label: '服务器', translations: { en: 'Server' }, link: '/config/server/' },
          { label: '数据库', translations: { en: 'Database' }, link: '/config/database/' },
          { label: 'WebDAV 静态配置', translations: { en: 'WebDAV Static Config' }, link: '/config/webdav/' },
          { label: '访问限流', translations: { en: 'Rate Limiting' }, link: '/config/rate-limit/' },
          { label: '缓存', translations: { en: 'Cache' }, link: '/config/cache/' },
          { label: '配置同步', translations: { en: 'Configuration Sync' }, link: '/config/config-sync/' },
          { label: '日志', translations: { en: 'Logging' }, link: '/config/logging/' }
        ]
      },
      {
        label: '运行时配置',
        translations: { en: 'Runtime Configuration' },
        collapsed: false,
        items: [
          { label: '配置总览', translations: { en: 'Configuration Overview' }, link: '/config/' },
          { label: '系统设置', translations: { en: 'System Settings' }, link: '/config/runtime/' },
          { label: '登录与会话', translations: { en: 'Login and Sessions' }, link: '/config/auth/' },
          { label: '外部认证', translations: { en: 'External Authentication' }, link: '/config/external-auth/' },
          { label: '邮件', translations: { en: 'Mail' }, link: '/config/mail/' },
          { label: '存储策略', translations: { en: 'Storage Policies' }, link: '/config/storage/' },
          { label: '离线下载', translations: { en: 'Offline Download' }, link: '/config/offline-download/' }
        ]
      }
    ]
  },
  {
    label: '存储后端',
    translations: { en: 'Storage Backends' },
    collapsed: true,
    items: [
      { label: '后端总览', translations: { en: 'Backend Overview' }, link: '/storage/' },
      { label: '本地磁盘', translations: { en: 'Local Disk' }, link: '/storage/local/' },
      { label: 'S3 / MinIO / R2', translations: { en: 'S3 / MinIO / R2' }, link: '/storage/s3-minio-r2/' },
      { label: 'Azure Blob Storage', translations: { en: 'Azure Blob Storage' }, link: '/storage/azure-blob/' },
      { label: '腾讯云 COS', translations: { en: 'Tencent COS' }, link: '/storage/tencent-cos/' },
      { label: 'OneDrive', translations: { en: 'OneDrive' }, link: '/storage/onedrive/' },
      { label: 'SFTP', translations: { en: 'SFTP' }, link: '/storage/sftp/' },
      { label: '远程节点存储策略', translations: { en: 'Follower Node Storage Policy' }, link: '/storage/remote-follower/' }
    ]
  },
  {
    label: '部署运维',
    translations: { en: 'Deployment and Operations' },
    collapsed: true,
    items: [
      { label: '部署概览', translations: { en: 'Deployment Overview' }, link: '/deployment/' },
      { label: 'Docker 部署', translations: { en: 'Docker Deployment' }, link: '/deployment/docker/' },
      { label: 'Docker 从节点', translations: { en: 'Docker Follower' }, link: '/deployment/docker-follower/' },
      { label: 'systemd', translations: { en: 'systemd' }, link: '/deployment/systemd/' },
      { label: '反向代理', translations: { en: 'Reverse Proxy' }, link: '/deployment/reverse-proxy/' },
      {
        label: '从节点网络拓扑',
        translations: { en: 'Follower Network Topologies' },
        link: '/deployment/follower-network-topologies/'
      },
      { label: '首次启动检查', translations: { en: 'First-Start Checklist' }, link: '/deployment/runtime-behavior/' },
      {
        label: '生产上线检查',
        translations: { en: 'Production Launch Checklist' },
        link: '/deployment/production-checklist/'
      },
      { label: '监控与 Grafana', translations: { en: 'Monitoring and Grafana' }, link: '/deployment/monitoring/' },
      { label: '容量规划参考', translations: { en: 'Capacity Planning' }, link: '/deployment/capacity-planning/' },
      { label: '运维 CLI', translations: { en: 'Operations CLI' }, link: '/deployment/ops-cli/' },
      { label: '备份与恢复', translations: { en: 'Backup and Restore' }, link: '/deployment/backup/' },
      { label: '升级与版本迁移', translations: { en: 'Upgrade and Version Migration' }, link: '/deployment/upgrade/' },
      { label: '故障排查', translations: { en: 'Troubleshooting' }, link: '/deployment/troubleshooting/' },
      { label: '前端资源缓存', translations: { en: 'Frontend Asset Cache' }, link: '/deployment/frontend-assets/' },
      {
        label: '性能基准与压测',
        translations: { en: 'Performance Baselines and Load Testing' },
        link: '/deployment/performance-benchmarking/'
      }
    ]
  },
  {
    label: '参考与项目',
    translations: { en: 'Reference and Project' },
    collapsed: true,
    items: [
      { label: '参考总览', translations: { en: 'Reference Overview' }, link: '/reference/' },
      { label: '架构概览', translations: { en: 'Architecture Overview' }, link: '/reference/architecture/' },
      { label: '常见问题速查', translations: { en: 'FAQ' }, link: '/reference/faq/' },
      { label: '术语表', translations: { en: 'Glossary' }, link: '/reference/glossary/' },
      { label: '错误码处理', translations: { en: 'Error Codes' }, link: '/reference/errors/' },
      { label: '文档贡献说明', translations: { en: 'Docs Contribution Guide' }, link: '/reference/docs-contributing/' },
      { label: '关于 AsterDrive', translations: { en: 'About AsterDrive' }, link: '/reference/about/' }
    ]
  }
])

export default defineConfig({
  site: SITE_URL,
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
      title: 'AsterDrive',
      description: ZH_SITE_DESCRIPTION,
      plugins: [
        starlightAnnouncement({
          displayMode: 'first',
          iconSize: 18,
          announcements: [
            {
              id: 'security-advisory-ghsa-7797-6gjx-hwgh',
              content: {
                'zh-CN':
                  '安全更新：v0.4.0-beta.3 已修复 WebDAV 请求可导致服务进程终止的问题，旧版本实例请尽快升级。',
                en: 'Security update: v0.4.0-beta.3 fixes a WebDAV request issue that can terminate the server process. Upgrade older instances promptly.'
              },
              link: {
                text: {
                  'zh-CN': '查看安全公告',
                  en: 'View advisory'
                },
                href: 'https://github.com/AsterCommunity/AsterDrive/security/advisories/GHSA-7797-6gjx-hwgh'
              },
              variant: 'caution',
              dismissible: true,
              showOn: ['/**']
            }
          ]
        }),
        {
          name: 'asterdrive-announcement-zh-cn',
          hooks: {
            'config:setup'() {},
            'i18n:setup'({ injectTranslations }) {
              injectTranslations({
                'zh-CN': {
                  'starlightAnnouncement.dismiss': '关闭',
                  'starlightAnnouncement.learnMore': '了解更多'
                }
              })
            }
          }
        },
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
      logo: {
        light: './src/assets/asterdrive-dark.svg',
        dark: './src/assets/asterdrive-light.svg',
        replacesTitle: true
      },
      social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/AsterCommunity/AsterDrive' }],
      defaultLocale: 'root',
      locales: {
        root: { label: '简体中文', lang: 'zh-CN' },
        en: { label: 'English', lang: 'en' }
      },
      editLink: {
        baseUrl: 'https://github.com/AsterCommunity/AsterDrive/edit/master/docs/'
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
        PageFrame: './src/components/PageFrame.astro'
      },
      head: [
        { tag: 'meta', attrs: { name: 'theme-color', content: '#0F172A' } },
        { tag: 'link', attrs: { rel: 'icon', type: 'image/svg+xml', href: '/favicon.svg' } },
        { tag: 'meta', attrs: { name: 'twitter:card', content: 'summary' } }
      ],
      sidebar
    }),
    sitemap()
  ]
})
