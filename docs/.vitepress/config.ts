import { readFileSync } from 'node:fs'
import { resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vitepress'

const __dirname = dirname(fileURLToPath(import.meta.url))

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

export default defineConfig({
  title: 'AsterDrive',
  description: 'AsterDrive 部署、使用与维护手册，按当前版本的实际页面、后台入口和默认行为编写',
  lastUpdated: true,

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
              { text: '分享与公开访问', link: '/guide/sharing' },
              { text: '文件编辑', link: '/guide/editing' },
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
    ['meta', { name: 'og:type', content: 'website' }],
    ['meta', { name: 'og:locale', content: 'zh_CN' }],
    ['meta', { name: 'og:title', content: 'AsterDrive | 自托管云存储' }],
    ['meta', { name: 'og:site_name', content: 'AsterDrive' }]
  ],

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
          { text: '分享与公开访问', link: '/guide/sharing' },
          { text: '文件编辑', link: '/guide/editing' },
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
        text: '部署运维',
        collapsed: true,
        items: [
          { text: '部署概览', link: '/deployment/' },
          { text: 'Docker 部署', link: '/deployment/docker' },
          { text: 'Docker 从节点', link: '/deployment/docker-follower' },
          { text: 'systemd', link: '/deployment/systemd' },
          { text: '反向代理', link: '/deployment/reverse-proxy' },
          { text: '首次启动检查', link: '/deployment/runtime-behavior' },
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
          { text: '错误码处理', link: '/guide/errors' },
          { text: '自定义前端', link: '/guide/custom-frontend' },
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
