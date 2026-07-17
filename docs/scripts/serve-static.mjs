#!/usr/bin/env bun
// 极简静态文件服务器，用于本地预览组装好的版本化站点（.vitepress/dist-all）。
// 用法: bun scripts/serve-static.mjs <dir> [port]

import { readFile } from 'node:fs/promises'
import { createServer } from 'node:http'
import { extname, join, normalize } from 'node:path'

const root = normalize(process.argv[2] || '.')
const port = Number(process.argv[3] || 4173)

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.svg': 'image/svg+xml',
  '.json': 'application/json',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.woff2': 'font/woff2',
  '.xml': 'application/xml',
  '.txt': 'text/plain'
}

createServer(async (req, res) => {
  const path = decodeURIComponent(new URL(req.url ?? '/', 'http://localhost').pathname)
  let file = normalize(join(root, path))
  if (!file.startsWith(root)) {
    res.writeHead(403)
    return res.end()
  }
  if (path.endsWith('/')) {
    file = join(file, 'index.html')
  }
  let data
  try {
    data = await readFile(file)
  } catch {
    try {
      data = await readFile(`${file}.html`)
      file = `${file}.html`
    } catch {
      res.writeHead(404)
      return res.end('404')
    }
  }
  res.writeHead(200, { 'content-type': MIME[extname(file)] || 'application/octet-stream' })
  res.end(data)
}).listen(port, () => {
  console.log(`版本化站点预览: http://localhost:${port}`)
  console.log(`  /        最新 release 分支`)
  console.log(`  /next/   当前工作区（开发版）`)
  console.log(`  /vX.Y/   旧版本归档`)
})
