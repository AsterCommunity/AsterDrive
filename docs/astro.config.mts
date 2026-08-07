import { defineConfig } from 'astro/config'
import sitemap from '@astrojs/sitemap'
import starlight from '@astrojs/starlight'
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
      { label: '开始使用', translations: { en: 'Start Here' }, link: '/start/' },
      { label: '快速开始', translations: { en: 'Quick Start' }, link: '/start/quick-trial/' },
      { label: '部署方式选择', translations: { en: 'Choose a Deployment' }, link: '/start/choose-deployment/' },
      { label: '常用流程', translations: { en: 'Common Workflows' }, link: '/start/common-workflows/' },
      {
        label: '首次启动与第一个管理员',
        translations: { en: 'First Start and the First Admin' },
        link: '/start/first-admin/'
      }
    ]
  },
  {
    label: '使用',
    translations: { en: 'Using' },
    collapsed: true,
    items: [
      { label: '使用 AsterDrive', translations: { en: 'Using AsterDrive' }, link: '/using/' },
      { label: '文件与整理', translations: { en: 'Files and Organization' }, link: '/using/files/' },
      { label: '上传与下载', translations: { en: 'Upload and Download' }, link: '/using/upload-download/' },
      { label: '工作空间与团队', translations: { en: 'Workspaces and Teams' }, link: '/using/workspaces-teams/' },
      { label: '分享与公开访问', translations: { en: 'Sharing and Public Access' }, link: '/using/sharing/' },
      { label: '回收站与版本', translations: { en: 'Recycle Bin and Versions' }, link: '/using/recycle-bin/' },
      { label: '预览与编辑', translations: { en: 'Preview and Editing' }, link: '/using/preview-editing/' },
      { label: 'WebDAV 使用', translations: { en: 'Using WebDAV' }, link: '/using/webdav/' },
      { label: '账号与安全', translations: { en: 'Account and Security' }, link: '/using/account-security/' }
    ]
  },
  {
    label: '管理',
    translations: { en: 'Administration' },
    collapsed: true,
    items: [
      { label: '管理后台', translations: { en: 'Admin Console' }, link: '/admin/' },
      { label: '用户与团队', translations: { en: 'Users and Teams' }, link: '/admin/users-teams/' },
      { label: '注册、登录与 SSO', translations: { en: 'Registration, Login and SSO' }, link: '/admin/auth-sso/' },
      { label: '邮件投递', translations: { en: 'Mail Delivery' }, link: '/admin/mail/' },
      {
        label: '存储策略与策略组',
        translations: { en: 'Storage Policies and Policy Groups' },
        link: '/admin/storage-policies/'
      },
      {
        label: '存储后端',
        translations: { en: 'Storage Backends' },
        collapsed: true,
        items: [
          { label: '后端总览', translations: { en: 'Backend Overview' }, link: '/admin/storage-backends/' },
          { label: '本地磁盘', translations: { en: 'Local Disk' }, link: '/admin/storage-backends/local/' },
          { label: 'S3 / MinIO / R2', translations: { en: 'S3 / MinIO / R2' }, link: '/admin/storage-backends/s3/' },
          { label: '阿里云 OSS', translations: { en: 'Alibaba Cloud OSS' }, link: '/admin/storage-backends/alibaba-oss/' },
          {
            label: 'Azure Blob Storage',
            translations: { en: 'Azure Blob Storage' },
            link: '/admin/storage-backends/azure-blob/'
          },
          { label: '腾讯云 COS', translations: { en: 'Tencent COS' }, link: '/admin/storage-backends/tencent-cos/' },
          { label: '华为云 OBS', translations: { en: 'Huawei Cloud OBS' }, link: '/admin/storage-backends/huawei-obs/' },
          { label: 'OneDrive', translations: { en: 'OneDrive' }, link: '/admin/storage-backends/onedrive/' },
          { label: 'SFTP', translations: { en: 'SFTP' }, link: '/admin/storage-backends/sftp/' },
          {
            label: '远程节点存储策略',
            translations: { en: 'Follower Node Storage Policy' },
            link: '/admin/storage-backends/remote-follower/'
          }
        ]
      },
      { label: '远程节点', translations: { en: 'Remote Nodes' }, link: '/admin/follower-nodes/' },
      {
        label: '预览与文件处理',
        translations: { en: 'Preview and File Processing' },
        link: '/admin/preview-processing/'
      },
      { label: '离线下载', translations: { en: 'Offline Download' }, link: '/admin/offline-download/' },
      { label: '自定义前端', translations: { en: 'Custom Frontend' }, link: '/admin/custom-frontend/' }
    ]
  },
  {
    label: '部署',
    translations: { en: 'Deployment' },
    collapsed: true,
    items: [
      { label: '部署概览', translations: { en: 'Deployment Overview' }, link: '/deploy/' },
      { label: '单实例 Docker', translations: { en: 'Single-Instance Docker' }, link: '/deploy/docker/' },
      { label: '单实例 systemd', translations: { en: 'Single-Instance systemd' }, link: '/deploy/systemd/' },
      { label: '反向代理', translations: { en: 'Reverse Proxy' }, link: '/deploy/reverse-proxy/' },
      {
        label: '多实例与负载均衡',
        translations: { en: 'Multi-Instance and Load Balancing' },
        link: '/deploy/multi-instance/'
      },
      { label: 'Kubernetes 部署', translations: { en: 'Kubernetes Deployment' }, link: '/deploy/kubernetes/' },
      {
        label: 'Follower 存储节点',
        translations: { en: 'Follower Storage Node' },
        collapsed: true,
        items: [
          {
            label: '接入与部署',
            translations: { en: 'Enrollment and Deployment' },
            link: '/deploy/follower-node/'
          },
          {
            label: '网络拓扑',
            translations: { en: 'Network Topologies' },
            link: '/deploy/follower-node/network/'
          }
        ]
      }
    ]
  },
  {
    label: '运维',
    translations: { en: 'Operations' },
    collapsed: true,
    items: [
      { label: '运维概览', translations: { en: 'Operations Overview' }, link: '/ops/' },
      { label: '首次启动检查', translations: { en: 'First-Start Checklist' }, link: '/ops/first-check/' },
      {
        label: '生产上线检查',
        translations: { en: 'Production Launch Checklist' },
        link: '/ops/launch-checklist/'
      },
      { label: '监控与 Grafana', translations: { en: 'Monitoring and Grafana' }, link: '/ops/monitoring/' },
      {
        label: '容量与压测',
        translations: { en: 'Capacity and Benchmarking' },
        collapsed: true,
        items: [
          { label: '容量规划参考', translations: { en: 'Capacity Planning' }, link: '/ops/capacity/' },
          {
            label: '性能基准与压测',
            translations: { en: 'Performance Benchmarking' },
            link: '/ops/capacity/benchmarking/'
          }
        ]
      },
      { label: '备份与恢复', translations: { en: 'Backup and Restore' }, link: '/ops/backup/' },
      { label: '升级与版本迁移', translations: { en: 'Upgrade and Version Migration' }, link: '/ops/upgrade/' },
      { label: '故障排查', translations: { en: 'Troubleshooting' }, link: '/ops/troubleshooting/' },
      { label: '运维 CLI', translations: { en: 'Operations CLI' }, link: '/ops/cli/' }
    ]
  },
  {
    label: '参考与项目',
    translations: { en: 'Reference and Project' },
    collapsed: true,
    items: [
      { label: '参考总览', translations: { en: 'Reference Overview' }, link: '/reference/' },
      {
        label: '配置字段',
        translations: { en: 'Configuration Fields' },
        collapsed: true,
        items: [
          { label: '配置总览', translations: { en: 'Configuration Overview' }, link: '/reference/config/' },
          { label: '部署模式', translations: { en: 'Deployment Profile' }, link: '/reference/config/deployment/' },
          { label: '服务器', translations: { en: 'Server' }, link: '/reference/config/server/' },
          { label: '数据库', translations: { en: 'Database' }, link: '/reference/config/database/' },
          { label: '缓存', translations: { en: 'Cache' }, link: '/reference/config/cache/' },
          { label: '配置同步', translations: { en: 'Configuration Sync' }, link: '/reference/config/config-sync/' },
          { label: '日志', translations: { en: 'Logging' }, link: '/reference/config/logging/' },
          { label: '访问限流', translations: { en: 'Rate Limiting' }, link: '/reference/config/rate-limit/' },
          {
            label: 'WebDAV 静态配置',
            translations: { en: 'WebDAV Static Config' },
            link: '/reference/config/webdav/'
          },
          { label: '登录与会话', translations: { en: 'Login and Sessions' }, link: '/reference/config/auth/' },
          {
            label: '外部认证',
            translations: { en: 'External Authentication' },
            link: '/reference/config/external-auth/'
          }
        ]
      },
      {
        label: '系统设置',
        translations: { en: 'System Settings' },
        collapsed: true,
        items: [
          {
            label: '系统设置总览',
            translations: { en: 'System Settings Overview' },
            link: '/reference/config/runtime/'
          },
          { label: '站点配置', translations: { en: 'Site Configuration' }, link: '/reference/config/runtime/site/' },
          { label: '用户管理', translations: { en: 'User Management' }, link: '/reference/config/runtime/users/' },
          {
            label: '认证与 Cookie',
            translations: { en: 'Authentication and Cookies' },
            link: '/reference/config/runtime/auth/'
          },
          { label: '邮件投递', translations: { en: 'Mail Delivery' }, link: '/reference/config/runtime/mail/' },
          { label: '网络访问', translations: { en: 'Network Access' }, link: '/reference/config/runtime/network/' },
          { label: '运行时', translations: { en: 'Runtime' }, link: '/reference/config/runtime/jobs/' },
          {
            label: '存储与保留',
            translations: { en: 'Storage and Retention' },
            link: '/reference/config/runtime/retention/'
          },
          {
            label: '文件处理',
            translations: { en: 'File Processing' },
            link: '/reference/config/runtime/file-processing/'
          },
          { label: 'WebDAV', translations: { en: 'WebDAV' }, link: '/reference/config/runtime/webdav/' },
          { label: '审计日志', translations: { en: 'Audit Logs' }, link: '/reference/config/runtime/audit/' }
        ]
      },
      {
        label: '存储能力矩阵',
        translations: { en: 'Storage Capability Matrix' },
        link: '/reference/storage-matrix/'
      },
      {
        label: 'WebDAV 协议兼容',
        translations: { en: 'WebDAV Protocol Compatibility' },
        link: '/reference/webdav-compat/'
      },
      {
        label: '运行架构',
        translations: { en: 'Runtime Architecture' },
        link: '/reference/runtime-architecture/'
      },
      { label: '常见问题速查', translations: { en: 'FAQ' }, link: '/reference/faq/' },
      { label: '术语表', translations: { en: 'Glossary' }, link: '/reference/glossary/' },
      { label: '错误码处理', translations: { en: 'Error Codes' }, link: '/reference/errors/' },
      { label: '关于 AsterDrive', translations: { en: 'About AsterDrive' }, link: '/reference/about/' }
    ]
  }
])

const movedRoutes: Record<string, string> = {
  '/deployment': '/deploy',
  '/deployment/docker': '/deploy/docker',
  '/deployment/systemd': '/deploy/systemd',
  '/deployment/load-balancing': '/deploy/multi-instance',
  '/deployment/kubernetes': '/deploy/kubernetes',
  '/deployment/docker-follower': '/deploy/follower-node',
  '/deployment/follower-network-topologies': '/deploy/follower-node/network',
  '/deployment/reverse-proxy': '/deploy/reverse-proxy',
  '/deployment/runtime-behavior': '/ops/first-check',
  '/deployment/production-checklist': '/ops/launch-checklist',
  '/deployment/monitoring': '/ops/monitoring',
  '/deployment/capacity-planning': '/ops/capacity',
  '/deployment/performance-benchmarking': '/ops/capacity/benchmarking',
  '/deployment/backup': '/ops/backup',
  '/deployment/upgrade': '/ops/upgrade',
  '/deployment/troubleshooting': '/ops/troubleshooting',
  '/deployment/ops-cli': '/ops/cli',
  '/deployment/frontend-assets': '/ops/upgrade',
  '/guide': '/start',
  '/guide/getting-started': '/start/quick-trial',
  '/guide/installation': '/start/choose-deployment',
  '/guide/core-workflows': '/start/common-workflows',
  '/guide/user-guide': '/using',
  '/guide/editing': '/using/preview-editing',
  '/guide/sharing': '/using/sharing',
  '/guide/teams-and-permissions': '/using/workspaces-teams',
  '/guide/upload-modes': '/using/upload-download',
  '/guide/webdav': '/using/webdav',
  '/guide/admin-console': '/admin',
  '/guide/remote-nodes': '/admin/follower-nodes',
  '/guide/custom-frontend': '/admin/custom-frontend',
  '/guide/preview-and-wopi': '/admin/preview-processing',
  '/config/storage': '/admin/storage-policies',
  '/config/mail': '/admin/mail',
  '/config/offline-download': '/admin/offline-download',
  '/storage': '/admin/storage-backends',
  '/storage/local': '/admin/storage-backends/local',
  '/storage/s3-minio-r2': '/admin/storage-backends/s3',
  '/storage/azure-blob': '/admin/storage-backends/azure-blob',
  '/storage/tencent-cos': '/admin/storage-backends/tencent-cos',
  '/storage/onedrive': '/admin/storage-backends/onedrive',
  '/storage/sftp': '/admin/storage-backends/sftp',
  '/storage/remote-follower': '/admin/storage-backends/remote-follower',
  '/config': '/reference/config',
  '/config/deployment': '/reference/config/deployment',
  '/config/server': '/reference/config/server',
  '/config/database': '/reference/config/database',
  '/config/cache': '/reference/config/cache',
  '/config/config-sync': '/reference/config/config-sync',
  '/config/logging': '/reference/config/logging',
  '/config/rate-limit': '/reference/config/rate-limit',
  '/config/webdav': '/reference/config/webdav',
  '/config/auth': '/reference/config/auth',
  '/config/external-auth': '/reference/config/external-auth',
  '/config/runtime': '/reference/config/runtime',
  '/features': '/using',
  '/features/auth-access': '/admin/auth-sso',
  '/features/files-workspaces': '/using',
  '/features/upload-storage': '/admin/storage-backends',
  '/features/preview-processing': '/admin/preview-processing',
  '/features/runtime-operations': '/ops',
  '/reference/architecture': '/reference/runtime-architecture'
}

const redirects = {
  ...Object.fromEntries(
    Object.entries(movedRoutes).flatMap(([from, to]) => [
      [from, to],
      [`/en${from}`, `/en${to}`]
    ])
  ),
  // 跨站重定向不能走 movedRoutes 的 /en 自动展开，手动声明
  '/reference/docs-contributing': '/developer/contributing/documentation',
  '/en/reference/docs-contributing': '/developer/en/contributing/documentation'
}

export default defineConfig({
  site: SITE_URL,
  build: { format: 'directory' },
  trailingSlash: 'always',
  redirects,
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
          displayMode: 'stack',
          iconSize: 18,
          announcements: [
            {
              id: 'security-advisory-cve-2026-8461',
              content: {
                'zh-CN':
                  '安全更新：Docker 镜像从 v0.4.0-rc.1 起已修复 FFmpeg MagicYUV 解码器漏洞 CVE-2026-8461（高危），使用旧镜像的实例请立即升级。',
                en: 'Security update: Docker images from v0.4.0-rc.1 fix the high-severity FFmpeg MagicYUV decoder vulnerability (CVE-2026-8461). Upgrade instances using older images immediately.'
              },
              link: {
                text: {
                  'zh-CN': '查看 CVE 公告',
                  en: 'View CVE advisory'
                },
                href: 'https://nvd.nist.gov/vuln/detail/CVE-2026-8461'
              },
              variant: 'caution',
              dismissible: true,
              showOn: ['/**']
            },
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
