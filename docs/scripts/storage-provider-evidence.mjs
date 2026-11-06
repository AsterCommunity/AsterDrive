#!/usr/bin/env node
// Offline contract/freshness checks are deterministic; --links produces a separate live report.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { dirname, resolve, relative } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

export const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
export const registryPath = 'docs/storage-provider-evidence.json'
export const topics = ['objects', 'uploads', 'signing', 'cors', 'range_etag', 'errors', 'permissions', 'domains']
const pagePath = (locale) => `developer-docs/${locale}/reference/storage-provider-evidence.md`
const read = (base, path) => readFileSync(resolve(base, path), 'utf8')
const requireThat = (ok, message) => { if (!ok) throw new Error(message) }
const same = (a, b) => JSON.stringify([...a].sort()) === JSON.stringify([...b].sort())
const nonempty = (s) => typeof s === 'string' && s.trim().length > 0
const datePattern = /^\d{4}-\d{2}-\d{2}$/

export function fields(source) {
  return [...new Set([...source.matchAll(/^\s*pub\s+(\w+)\s*:\s*([^\n]+)/gm)]
    .map((m) => `${m[1]}: ${m[2].split('=>')[0].trim().replace(/,$/, '').trim()}`))].sort()
}

export const schemas = (source) => [...source.matchAll(/pub struct (\w+ConfigV\d+)\b/g)].map((m) => m[1]).sort()

export function validate(data, { base = root, now = new Date() } = {}) {
  requireThat(data.schema_version === 1, 'Unsupported evidence schema')
  requireThat(Number.isInteger(data.refresh_days) && data.refresh_days > 0 && data.refresh_days <= 90, 'refresh_days must be 1..90')
  const safePath = (path) => {
    requireThat(nonempty(path) && !relative(base, resolve(base, path)).startsWith('..') && !path.startsWith('/'), `Unsafe path: ${path}`)
    return read(base, path)
  }
  const manifest = JSON.parse(safePath('docs/generated/storage-connectors.json'))
  const ids = data.providers.map((p) => p.connector_id)
  requireThat(new Set(ids).size === ids.length && same(ids, manifest.connectors.map((c) => c.connector_id)), 'Provider inventory differs from built-in connector manifest')
  const locked = [...safePath('Cargo.lock').matchAll(/\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"/g)]
  const hosts = data.official_hosts
  requireThat(Array.isArray(hosts) && hosts.length > 0, 'Missing official host policy')
  for (const p of data.providers) {
    const label = p.connector_id
    requireThat(/^[a-z][a-z0-9_]*$/.test(p.id) && label === `asterdrive.storage.${p.id}`, `Invalid provider ID: ${label}`)
    requireThat(nonempty(p.owner) && nonempty(p.api_version), `${label}: missing owner/API version`)
    requireThat(p.cloud_acceptance.status === 'not_recorded' || p.cloud_acceptance.status === 'recorded', `${label}: invalid cloud evidence status`)
    requireThat(nonempty(p.cloud_acceptance.note), `${label}: missing acceptance boundary`)
    if (p.cloud_acceptance.status === 'recorded') safePath(p.cloud_acceptance.record)
    requireThat(p.dependencies.length > 0, `${label}: missing dependency pins`)
    for (const dep of p.dependencies) {
      requireThat(locked.some((m) => m[1] === dep.name && m[2] === dep.version), `${label}: SDK/transport version drift: ${dep.name}@${dep.version}`)
    }
    requireThat(p.field_contracts.length > 0, `${label}: missing field contract`)
    for (const c of p.field_contracts) {
      const source = safePath(c.path)
      requireThat(same(fields(source), c.fields), `${label}: field drift in ${c.path}`)
      requireThat(same(schemas(source), c.schemas), `${label}: config schema version drift in ${c.path}`)
    }
    requireThat(p.guards.length > 0, `${label}: missing wire guards`)
    for (const g of p.guards) {
      const source = safePath(g.path)
      requireThat(g.contains.length > 0 && g.contains.every((s) => nonempty(s) && source.includes(s)), `${label}: protocol/limit drift in ${g.path}`)
    }
    for (const path of p.code) {
      requireThat(safePath(path).includes(`Provider evidence: ${pagePath('zh-CN')}#${p.id}`), `${label}: missing source evidence reference in ${path}`)
    }
    for (const path of p.tests) safePath(path)
    requireThat(p.code.length >= 2 && p.tests.length > 0, `${label}: missing code/tests`)
    const sourceIds = p.sources.map((s) => s.id)
    requireThat(new Set(sourceIds).size === sourceIds.length, `${label}: duplicate source ID`)
    requireThat(new Set(p.contracts.map((c) => c.topic)).size === topics.length && same(p.contracts.map((c) => c.topic), topics), `${label}: missing/duplicate capability topic`)
    const used = new Set()
    for (const c of p.contracts) {
      requireThat(nonempty(c.zh) && nonempty(c.en) && c.sources.length > 0, `${label}/${c.topic}: missing conclusion/source`)
      for (const id of c.sources) { requireThat(sourceIds.includes(id), `${label}: unknown source ${id}`); used.add(id) }
    }
    requireThat(used.size === sourceIds.length, `${label}: unreferenced source`)
    for (const s of p.sources) {
      const u = new URL(s.url)
      requireThat(u.protocol === 'https:' && !u.username && !u.password && hosts.includes(u.hostname), `${label}: nonofficial source ${s.url}`)
      // GitHub is allowed only for official organizations, and is supplemental, not sole authority.
      if (u.hostname === 'github.com') requireThat(data.official_github_orgs.includes(u.pathname.split('/')[1]), `${label}: nonofficial GitHub source`)
      requireThat(nonempty(s.version) && nonempty(s.title), `${label}: missing source version/title`)
      requireThat(Array.isArray(s.title_contains) && s.title_contains.length > 0 && s.title_contains.every(nonempty), `${label}: missing expected page title`)
      requireThat(['verified', 'unverified'].includes(s.status), `${label}: invalid source status`)
      requireThat(nonempty(s.note), `${label}: missing review/access boundary note`)
      requireThat(datePattern.test(s.attempted_on), `${label}: missing access attempt date`)
      requireThat(s.status === 'unverified' ? s.reviewed_on === null && nonempty(s.note) : datePattern.test(s.reviewed_on), `${label}: invalid content review evidence`)
      for (const d of [s.attempted_on, s.reviewed_on].filter(Boolean)) {
        const parsed = new Date(`${d}T00:00:00Z`)
        requireThat(!Number.isNaN(+parsed) && parsed.toISOString().slice(0, 10) === d && parsed <= now, `${label}: invalid/future evidence date ${d}`)
        requireThat((now - parsed) / 86400000 <= data.refresh_days, `${label}/${s.id}: stale evidence (${d}); recheck provider docs`)
      }
      if (s.reviewed_on) requireThat(s.reviewed_on <= s.attempted_on, `${label}: review date exceeds access attempt`)
    }
    if (!['local', 'remote'].includes(p.id)) {
      for (const c of p.contracts) requireThat(c.sources.some((id) => {
        const u = new URL(p.sources.find((s) => s.id === id).url)
        return u.hostname !== 'github.com' || data.official_github_doc_repositories.includes(u.pathname.split('/').slice(1, 3).join('/'))
      }), `${label}/${c.topic}: SDK source cannot be sole authority`)
    }
    for (const locale of ['', 'en/']) {
      const slug = manifest.connectors.find((c) => c.connector_id === label).documentation.tutorial_slug
      requireThat(safePath(`docs/src/content/docs/${locale}admin/storage-backends/${slug}.md`).includes(`/reference/storage-provider-evidence/#${p.id}`), `${label}: missing bilingual tutorial reference`)
    }
  }
  return data.providers.flatMap((p) => p.sources.filter((s) => s.status === 'unverified').map((s) => `${p.id}/${s.id}`))
}

export function render(data, locale) {
  const zh = locale === 'zh-CN'
  const lines = [zh ? '# 外部 provider 协议事实源与证据索引' : '# External provider protocol evidence', '',
    '<!-- Generated by docs/scripts/storage-provider-evidence.mjs --write. Edit docs/storage-provider-evidence.json. -->', '',
    zh ? '[维护、复核与升级规则](../contributing/provider-evidence.md)。下表是协议依据和受影响位置，不代表云端已验收。访问失败的条目标记为 unverified；HTTP 可达也不等于内容已复核。' : '[Ownership, refresh and upgrade policy](../contributing/provider-evidence.md). These are protocol references and affected paths, not cloud acceptance. Unreachable sources stay unverified; HTTP reachability does not establish content review.', '',
    `Schema: ${data.schema_version}; ${zh ? '复核周期' : 'refresh interval'}: ${data.refresh_days} days.`, '']
  const labels = zh ? ['对象读写', '分片/续传', '签名/预签名', 'CORS', 'Range/ETag', '错误', '权限', '源站/上传/CDN/自定义域'] : ['Object I/O', 'Multipart/resumable', 'Signing/presigned', 'CORS', 'Range/ETag', 'Errors', 'Permissions', 'Origin/upload/CDN/custom domain']
  const codeLink = (path) => `[${path}](https://github.com/AsterCommunity/AsterDrive/blob/master/${path})`
  const cell = (s) => s.replaceAll('|', '\\|').replaceAll('\n', ' ')
  for (const p of data.providers) {
    lines.push(`<a id="${p.id}"></a>`, '', `## ${p.id}`, '', `Connector: \`${p.connector_id}\`; owner: ${p.owner}; API: ${p.api_version}.`, '',
      `SDK/transport: ${p.dependencies.map((d) => `\`${d.name}@${d.version}\``).join(', ')}.`, '',
      `Code: ${p.code.map(codeLink).join(', ')}.`, '', `Fixtures: ${p.tests.map(codeLink).join(', ')}.`, '',
      `Cloud acceptance: **${p.cloud_acceptance.status}** — ${p.cloud_acceptance.note}${p.cloud_acceptance.record ? ` (${codeLink(p.cloud_acceptance.record)})` : ''}`, '',
      zh ? '| 能力 | 结论与边界 | 官方依据 |' : '| Capability | Conclusion and boundary | Official sources |', '| --- | --- | --- |')
    for (const c of p.contracts) lines.push(`| <a id="${p.id}-${c.topic}"></a>${labels[topics.indexOf(c.topic)]} | ${cell(c[zh ? 'zh' : 'en'])} | ${c.sources.map((id) => { const s = p.sources.find((s) => s.id === id); return `[${id}](${s.url})` }).join(', ')} |`)
    lines.push('', zh ? '| 依据 | 版本 | 访问尝试 | 内容复核 | 状态与说明 |' : '| Source | Version | Access attempt | Content review | Status and note |', '| --- | --- | --- | --- | --- |')
    for (const s of p.sources) lines.push(`| [${cell(s.title)}](${s.url}) | ${cell(s.version)} | ${s.attempted_on} | ${s.reviewed_on ?? '—'} | ${s.status}: ${cell(s.note)} |`)
    lines.push('')
  }
  return `${lines.join('\n').trimEnd()}\n`
}

export async function checkLinks(data, { request = fetch, checkedAt = new Date().toISOString() } = {}) {
  const urls = [...new Set(data.providers.flatMap((p) => p.sources.map((s) => s.url)))]
  const results = new Array(urls.length)
  let next = 0
  await Promise.all(Array.from({ length: Math.min(6, urls.length) }, async () => {
    while (next < urls.length) {
      const index = next++
      const url = urls[index]
      try {
        let target = new URL(url)
        let response
        const timeout = AbortSignal.timeout(15000)
        for (let redirects = 0; ; redirects++) {
          requireThat(target.protocol === 'https:' && data.official_hosts.includes(target.hostname) && !target.username && !target.password, 'Redirect leaves official HTTPS hosts')
          if (target.hostname === 'github.com') requireThat(data.official_github_orgs.includes(target.pathname.split('/')[1]), 'Redirect leaves official GitHub organizations')
          response = await request(target.href, { redirect: 'manual', signal: timeout, headers: { 'User-Agent': 'AsterDrive-provider-evidence-link-check/1.0' } })
          if (![301, 302, 303, 307, 308].includes(response.status)) break
          await response.body?.cancel()
          requireThat(redirects < 5, 'Too many redirects')
          const location = response.headers.get('location')
          requireThat(location, 'Redirect without location')
          target = new URL(location, target)
        }
        const status = response.status
        const source = data.providers.flatMap((p) => p.sources).find((s) => s.url === url)
        let reason
        let title
        if (status >= 200 && status < 300 && source.title_contains) {
          // Titles catch soft 404s, login pages and redirects to unrelated tutorials.
          // Read a bounded prefix; large official SDK files need not be downloaded in full.
          const reader = response.body?.getReader()
          const decoder = new TextDecoder()
          let html = ''
          let bytes = 0
          try {
            while (reader && bytes < 2 * 1024 * 1024) {
              const chunk = await reader.read()
              if (chunk.done) break
              bytes += chunk.value.byteLength
              html += decoder.decode(chunk.value, { stream: true })
              if (html.includes('</title>') && (source.content_contains ?? []).every((s) => html.includes(s))) break
            }
          } finally { await reader?.cancel() }
          title = html.match(/<title\b[^>]*>([\s\S]*?)<\/title>/i)?.[1]?.replace(/\s+/g, ' ').trim()
          if (!title || !source.title_contains.some((s) => title.toLowerCase().includes(s.toLowerCase()))) reason = 'Expected document title not found (possible soft 404/login/wrong page)'
          else if (!(source.content_contains ?? []).every((s) => html.includes(s))) reason = 'Expected protocol/limit text missing; review provider documentation drift'
        } else await response.body?.cancel()
        results[index] = { url, final_url: target.href, http_status: status, title, status: status >= 200 && status < 300 && !reason ? 'reachable' : 'unverified', ...(reason ? { reason } : {}) }
      } catch (error) {
        results[index] = { url, status: 'unverified', reason: error.message }
      }
    }
  }))
  return { checked_at: checkedAt, boundary: 'Reachability only. Does not update content review dates or prove protocol/cloud acceptance.', results }
}

async function main() {
  const args = process.argv.slice(2)
  const allowed = new Set(['--write', '--links', '--report'])
  for (let i = 0; i < args.length; i++) {
    requireThat(allowed.has(args[i]), `Unknown option: ${args[i]}`)
    if (args[i] === '--report') requireThat(nonempty(args[++i]) && !args[i].startsWith('--'), '--report requires a file')
  }
  const data = JSON.parse(read(root, registryPath))
  const unresolved = validate(data)
  for (const locale of ['zh-CN', 'en']) {
    const path = resolve(root, pagePath(locale))
    const expected = render(data, locale)
    if (args.includes('--write')) { mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, expected) }
    else requireThat(readFileSync(path, 'utf8') === expected, `${pagePath(locale)} drift; run node docs/scripts/storage-provider-evidence.mjs --write`)
  }
  console.log(`Provider contracts: ${data.providers.length}; content review unverified: ${unresolved.length}${unresolved.length ? ` (${unresolved.join(', ')})` : ''}`)
  if (args.includes('--links')) {
    const report = await checkLinks(data)
    const output = `${JSON.stringify(report, null, 2)}\n`
    const reportIndex = args.indexOf('--report')
    if (reportIndex >= 0) writeFileSync(args[reportIndex + 1], output)
    else console.log(output)
    const failed = report.results.filter((r) => r.status !== 'reachable')
    console.log(`Links reachable: ${report.results.length - failed.length}/${report.results.length}; unverified: ${failed.length}`)
    if (failed.length) process.exitCode = 1
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1 })
}
