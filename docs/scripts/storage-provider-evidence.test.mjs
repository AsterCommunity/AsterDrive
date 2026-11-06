import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { checkLinks, root, registryPath, validate } from './storage-provider-evidence.mjs'

const registry = JSON.parse(readFileSync(resolve(root, registryPath), 'utf8'))
const now = new Date()
const dateAfter = (days) => new Date(+now + days * 86400000).toISOString().slice(0, 10)
function rejects(change, message) {
  const data = structuredClone(registry)
  change(data, data.providers[0])
  assert.throws(() => validate(data, { now }), message)
}

test('current provider inventory and actual source contracts validate', () => {
  assert.ok(Array.isArray(validate(registry, { now })))
})
test('new or removed connector cannot lose its evidence', () => {
  rejects((data) => data.providers.pop(), /inventory/)
  rejects((data) => data.providers.push(data.providers[0]), /inventory/)
})
test('SDK versions, schema fields and protocol limits require evidence updates', () => {
  rejects((_, p) => { p.dependencies[0].version = '999.0.0' }, /version drift/)
  rejects((_, p) => { p.field_contracts[0].fields.pop() }, /field drift/)
  rejects((_, p) => { p.field_contracts[0].schemas = ['UnexpectedConfigV999'] }, /schema version drift/)
  rejects((_, p) => { p.guards[0].contains.push('const CORS_MAX_RULES: usize = 999;') }, /protocol\/limit drift/)
})
test('missing capability sources and nonofficial sources are rejected', () => {
  rejects((_, p) => { p.contracts.pop() }, /capability topic/)
  rejects((_, p) => { p.contracts[0].sources = ['missing'] }, /unknown source/)
  rejects((_, p) => { p.sources[0].url = 'https://blog.example.com/oss' }, /nonofficial/)
  rejects((_, p) => { p.sources[0].url = 'https://github.com/random/oss' }, /nonofficial GitHub/)
  rejects((_, p) => { p.sources[0].url = 'https://github.com/qiniu/go-sdk'; p.contracts[0].sources = [p.sources[0].id] }, /sole authority|unreferenced/)
})
test('freshness cannot be reset by a successful link probe; unknown and future review dates fail', () => {
  rejects((_, p) => { p.sources[0].attempted_on = dateAfter(-91) }, /stale/)
  rejects((_, p) => { Object.assign(p.sources[0], { status: 'verified', reviewed_on: dateAfter(-91) }) }, /stale/)
  rejects((_, p) => { p.sources[0].attempted_on = dateAfter(1) }, /future/)
  rejects((_, p) => { p.sources[0].attempted_on = '2026-02-30' }, /invalid/)
  rejects((_, p) => { Object.assign(p.sources[0], { status: 'verified', reviewed_on: null }) }, /content review/)
})
test('claiming cloud acceptance requires a real repository record', () => {
  rejects((_, p) => { p.cloud_acceptance = { status: 'recorded', note: 'Cloud passed', record: 'missing-record.md' } }, /ENOENT/)
  rejects((_, p) => { p.guards[0].path = '../outside' }, /Unsafe path/)
})

const linkData = (urls) => ({ official_hosts: ['docs.aws.amazon.com'], official_github_orgs: [], providers: [{ sources: urls.map((url) => ({ url })) }] })
test('live report distinguishes 2xx, denied, missing and offline without updating reviews', async () => {
  const data = linkData(['https://docs.aws.amazon.com/ok', 'https://docs.aws.amazon.com/denied', 'https://docs.aws.amazon.com/missing', 'https://docs.aws.amazon.com/offline'])
  const original = JSON.stringify(data)
  const report = await checkLinks(data, { request: async (url) => {
    if (url.endsWith('offline')) throw new Error('Network unavailable')
    return new Response('', { status: url.endsWith('denied') ? 403 : url.endsWith('missing') ? 404 : 200 })
  } })
  assert.deepEqual(report.results.map((r) => r.status), ['reachable', 'unverified', 'unverified', 'unverified'])
  assert.equal(JSON.stringify(data), original)
})
test('redirects are bounded and cannot probe nonofficial targets', async () => {
  let calls = 0
  const report = await checkLinks(linkData(['https://docs.aws.amazon.com/redirect']), { request: async () => {
    calls++
    return new Response(null, { status: 302, headers: { location: 'https://attacker.test/private' } })
  } })
  assert.equal(calls, 1)
  assert.equal(report.results[0].status, 'unverified')
  assert.match(report.results[0].reason, /official/)
  calls = 0
  const loop = await checkLinks(linkData(['https://docs.aws.amazon.com/loop']), { request: async () => {
    calls++
    return new Response(null, { status: 302, headers: { location: '/loop' } })
  } })
  assert.equal(calls, 6)
  assert.match(loop.results[0].reason, /redirects/)
})

test('HTTP 200 soft errors, wrong tutorials and changed limit text remain unverified', async () => {
  const data = linkData(['https://docs.aws.amazon.com/spec'])
  Object.assign(data.providers[0].sources[0], { title_contains: ['PutBucketCors'], content_contains: ['20 rules'] })
  for (const html of ['<title>404 Page not found</title>', '<title>Python tutorial</title>', '<title>PutBucketCors</title><p>10 rules</p>']) {
    const report = await checkLinks(data, { request: async () => new Response(html) })
    assert.equal(report.results[0].status, 'unverified')
  }
  const report = await checkLinks(data, { request: async () => new Response('<title>PutBucketCors</title><p>20 rules</p>') })
  assert.equal(report.results[0].status, 'reachable')
})
