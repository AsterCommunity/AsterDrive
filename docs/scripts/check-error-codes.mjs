#!/usr/bin/env bun
// 检查 docs/reference/errors.md（中英）里提到的错误码和 src/api/api_error_code.rs 是否同步。
//
// - 文档提到了代码里不存在的错误码 → 报错（文档过时）
// - 代码里有但文档从未提及的错误码 → 警告（文档只列用户会遇到的子集，允许，但要可见）
//
// 用法：
//   bun docs/scripts/check-error-codes.mjs          # 文档过时则 exit 1
//   bun docs/scripts/check-error-codes.mjs --strict # 代码有新增未记录也 exit 1

import { readFileSync } from 'node:fs'
import { resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const strict = process.argv.includes('--strict')

// 长得像错误码但不是：字段路径、旧契约字段、配置文件名
const NOT_A_CODE = new Set([
  'success',
  'error.code',
  'error.subcode',
  'error.internal_code',
  'error.retryable',
  'error.diagnostic',
  'error.diagnostic.message',
  'config.toml',
  'desktop.ini'
])

function extractSourceCodes() {
  const source = readFileSync(resolve(repoRoot, 'src/api/api_error_code.rs'), 'utf-8')
  const codes = new Set()
  for (const match of source.matchAll(/=>\s*"([a-z][a-z0-9_]*(?:\.[a-z0-9_]+)+)"/g)) {
    codes.add(match[1])
  }
  if (codes.size === 0) {
    console.error('未能从 api_error_code.rs 提取到任何错误码，提取规则可能已失效')
    process.exit(2)
  }
  return codes
}

function extractDocCodes(path) {
  const source = readFileSync(resolve(repoRoot, path), 'utf-8')
  const codes = new Set()
  for (const match of source.matchAll(/`([a-z][a-z0-9_]*(?:\.[a-z0-9_]+)+)`/g)) {
    const token = match[1]
    if (!NOT_A_CODE.has(token)) {
      codes.add(token)
    }
  }
  return codes
}

const sourceCodes = extractSourceCodes()
const docPages = ['docs/reference/errors.md', 'docs/en/reference/errors.md']
const documentedCodes = new Set()
let staleCount = 0

for (const page of docPages) {
  const pageCodes = extractDocCodes(page)
  for (const code of pageCodes) {
    documentedCodes.add(code)
    if (!sourceCodes.has(code)) {
      console.error(`✗ ${page} 提到了代码里不存在的错误码: ${code}`)
      staleCount++
    }
  }
}

const undocumented = [...sourceCodes].filter((code) => !documentedCodes.has(code)).sort()
if (undocumented.length > 0) {
  console.warn(`\n代码里有 ${undocumented.length} 个错误码文档未提及（errors.md 只列用户会遇到的子集，确认是否遗漏）：`)
  for (const code of undocumented) {
    console.warn(`  - ${code}`)
  }
}

if (staleCount > 0) {
  console.error(`\n${staleCount} 个过时错误码，请更新 errors.md`)
  process.exit(1)
}
if (strict && undocumented.length > 0) {
  console.error('\n--strict 模式下，代码新增错误码必须补充到 errors.md')
  process.exit(1)
}

console.log(`✓ 错误码文档同步检查通过（代码 ${sourceCodes.size} 个，文档覆盖 ${documentedCodes.size} 个）`)
