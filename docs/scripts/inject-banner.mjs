#!/usr/bin/env bun
// 给构建产物里的每个 HTML 注入旧版本提示条。
// 只用于老分支（主题不支持 VITE_VERSION_BANNER 的版本）的归档构建。
//
// 用法: bun docs/scripts/inject-banner.mjs <dir> <text> [url] [link-text]

import { readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const [dir, text, url, linkText] = process.argv.slice(2)
if (!dir || !text) {
  console.error('用法: inject-banner.mjs <dir> <text> [url] [link-text]')
  process.exit(2)
}

const link = url ? `<a href="${url}" style="margin-left:8px;font-weight:600;text-decoration:underline;color:inherit;">${linkText || url}</a>` : ''
// 横幅钉在视口顶部（AppHeader 之上），不随滚动移动；导航/正文下移由 --vp-layout-top-height 承担
// （老主题 v0.1+ 的 .VPNav 均消费该变量，z-index 用主题自带的 --vp-z-index-layout-top）。
// 注意必须注入到 </body> 之前：SSR HTML 里 body 前部有 teleport 锚点，Vue hydration 会按位置匹配
// body 子节点，锚点之前的额外元素节点会被当作不匹配节点移除（横幅会渲染后消失）。
const banner =
  `<style>:root{--vp-layout-top-height:36px;}</style>` +
  `<div style="position:fixed;top:0;right:0;left:0;z-index:var(--vp-z-index-layout-top,40);display:flex;align-items:center;justify-content:center;height:36px;padding:0 24px;background:linear-gradient(rgba(234,179,8,.14),rgba(234,179,8,.14)),var(--vp-c-bg,#fff);color:var(--vp-c-text-1,#1c1917);font-size:14px;line-height:1.5;border-bottom:1px solid var(--vp-c-divider,rgba(0,0,0,.1));">${text}${link}</div>`

function* walk(root) {
  for (const entry of readdirSync(root)) {
    const full = join(root, entry)
    if (statSync(full).isDirectory()) {
      yield* walk(full)
    } else if (entry.endsWith('.html')) {
      yield full
    }
  }
}

let count = 0
for (const file of walk(dir)) {
  const html = readFileSync(file, 'utf-8')
  const injected = html.replace('</body>', `${banner}</body>`)
  if (injected !== html) {
    writeFileSync(file, injected)
    count++
  }
}
console.log(`已在 ${count} 个 HTML 文件注入版本提示条`)
