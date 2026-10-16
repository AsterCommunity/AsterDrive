// Offline evidence check. This validates revisions/paths, not runtime support or rollout.
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const read = (path) => readFileSync(resolve(root, path), 'utf8')
const lockRevisions = (source) => new Set([...source.matchAll(
  /source = "git\+https:\/\/github\.com\/AsterCommunity\/AsterForge[^"\n]*#([a-f0-9]{40})"/g
)].map((match) => match[1]))
const current = lockRevisions(read('Cargo.lock'))
if (current.size !== 1) throw new Error('Expected one resolved Forge revision in Cargo.lock')
const [forgeRevision] = current
const pages = ['docs/src/content/docs/ops/monitoring.md', 'docs/src/content/docs/en/ops/monitoring.md']
let consumerRevision
for (const page of pages) {
  const source = read(page)
  const commitLinks = [...source.matchAll(
    /https:\/\/github\.com\/AsterCommunity\/(AsterDrive|AsterForge)\/commit\/([a-f0-9]{40})/g
  )]
  const revisions = (repo) => new Set(commitLinks.filter((m) => m[1] === repo).map((m) => m[2]))
  const drive = revisions('AsterDrive')
  const forge = revisions('AsterForge')
  if (drive.size !== 1 || forge.size !== 1 || !forge.has(forgeRevision)) {
    throw new Error(`${page}: missing/ambiguous evidence or stale Forge revision`)
  }
  const [revision] = drive
  if (consumerRevision && consumerRevision !== revision) throw new Error('Bilingual Drive revisions differ')
  consumerRevision = revision
  const pinnedLock = execFileSync('git', ['show', `${revision}:Cargo.lock`], { cwd: root, encoding: 'utf8' })
  const pinnedRevisions = lockRevisions(pinnedLock)
  if (pinnedRevisions.size !== 1 || !pinnedRevisions.has(forgeRevision)) {
    throw new Error(`${page}: cited consumer lock does not resolve the documented Forge revision`)
  }
  for (const link of source.matchAll(
    /https:\/\/github\.com\/AsterCommunity\/(AsterDrive|AsterForge)\/blob\/([a-f0-9]{40})\/([^\s)#]+)/g
  )) {
    const [, repo, sha, path] = link
    const expected = repo === 'AsterDrive' ? revision : forgeRevision
    if (sha !== expected) throw new Error(`${page}: source link outside the declared ${repo} baseline`)
    if (repo === 'AsterDrive') {
      execFileSync('git', ['cat-file', '-e', `${sha}:${path}`], { cwd: root, stdio: 'pipe' })
    }
    // Provider source links stay upstream; no network or sibling checkout is needed here.
  }
}
console.log(`Observability evidence OK: Drive ${consumerRevision}, Forge ${forgeRevision}; runtime/rollout not checked`)
