import { copyFile, mkdir, readdir, readFile, rm, writeFile } from 'node:fs/promises'
import { dirname, join, relative, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import { extractDescriptionFromMarkdown } from '../src/lib/extract-description.mjs'

const docsRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const sourceRoot = resolve(docsRoot, '..', 'developer-docs')
const generatedRoot = resolve(docsRoot, 'src-developer')
const generatedContentRoot = join(generatedRoot, 'content', 'docs')
const generatedPublicRoot = join(generatedRoot, 'public')
const siteRoot = 'https://drive.astercosm.com'
const githubRoot = 'https://github.com/AsterCommunity/AsterDrive/blob/master'
const githubEditRoot = 'https://github.com/AsterCommunity/AsterDrive/edit/master'

await rm(generatedRoot, { recursive: true, force: true })
await mkdir(generatedContentRoot, { recursive: true })
await mkdir(generatedPublicRoot, { recursive: true })
await copyFile(join(docsRoot, 'public', 'favicon.svg'), join(generatedPublicRoot, 'favicon.svg'))
await writeFile(
	join(generatedRoot, 'content.config.ts'),
	[
		"import { defineCollection } from 'astro:content'",
		"import { docsLoader } from '@astrojs/starlight/loaders'",
		"import { docsSchema } from '@astrojs/starlight/schema'",
		'',
		'export const collections = { docs: defineCollection({ loader: docsLoader(), schema: docsSchema() }) }',
		''
	].join('\n'),
	'utf8'
)

for (const language of [
	{ source: 'zh-CN', destination: '' },
	{ source: 'en', destination: 'en' }
]) {
	const sourceDir = join(sourceRoot, language.source)
	const destinationDir = join(generatedContentRoot, language.destination)
	await copyDirectory(sourceDir, destinationDir, language)
}

async function copyDirectory(sourceDir, destinationDir, language) {
	await mkdir(destinationDir, { recursive: true })
	const entries = await readdir(sourceDir, { withFileTypes: true })

	for (const entry of entries) {
		if (!entry.isFile() || !entry.name.endsWith('.md')) continue

		const sourcePath = join(sourceDir, entry.name)
		const sourceRelativePath = relative(sourceRoot, sourcePath)
		const destinationName = entry.name === 'README.md' ? 'index.md' : entry.name
		const destinationPath = join(destinationDir, destinationName)
		const content = await readFile(sourcePath, 'utf8')
		const title = extractTitle(content, sourcePath)
		const description = extractDescriptionFromMarkdown(content)
		const body = rewriteLinks(stripTitle(content), sourcePath, language)
		const frontmatter = [
			'---',
			`title: ${JSON.stringify(title)}`,
			...(description ? [`description: ${JSON.stringify(description)}`] : []),
			`editUrl: ${JSON.stringify(`${githubEditRoot}/developer-docs/${sourceRelativePath.split(sep).join('/')}`)}`,
			'---',
			''
		].join('\n')

		await writeFile(destinationPath, `${frontmatter}${body.trimStart()}\n`, 'utf8')
	}

	for (const entry of entries) {
		if (!entry.isDirectory()) continue
		await copyDirectory(join(sourceDir, entry.name), join(destinationDir, entry.name), language)
	}
}

function extractTitle(content, sourcePath) {
	const title = content.match(/^#\s+(.+)$/m)?.[1]?.trim()
	if (!title) throw new Error(`Developer doc has no H1 title: ${sourcePath}`)
	return title.replace(/[`*_]/g, '')
}

function stripTitle(content) {
	return content.replace(/^#\s+.+(?:\r?\n)+/, '')
}

function rewriteLinks(content, sourcePath, language) {
	return content.replace(/\]\(([^)]+)\)/g, (match, rawTarget) => {
		const target = rawTarget.trim()
		if (!target || ['<', '#', 'http://', 'https://', 'mailto:'].some((prefix) => target.startsWith(prefix))) {
			return match
		}

		const [targetPath, fragment] = target.split('#', 2)
		if (!targetPath.endsWith('.md')) return match

		const resolvedTarget = resolve(dirname(sourcePath), targetPath)
		const developerRelativeTarget = relative(sourceRoot, resolvedTarget)
		const isInsideDeveloperDocs =
			!developerRelativeTarget.startsWith(`..${sep}`) && developerRelativeTarget !== '..'
		let href

		if (isInsideDeveloperDocs) {
			const [targetLanguage, ...targetParts] = developerRelativeTarget.split(sep)
			const targetLocalePrefix = targetLanguage === 'en' ? 'en/' : ''
			href = `${siteRoot}/developer/${targetLocalePrefix}${pagePath(targetParts.join(sep))}`
		} else if (resolvedTarget.includes(`${sep}docs${sep}`)) {
			const docsRelativePath = resolvedTarget.split(`${sep}docs${sep}`)[1]
			href = `${siteRoot}/${pagePath(docsRelativePath)}`
		} else if (resolvedTarget.includes(`${sep}tests${sep}`)) {
			const repoRelativePath = resolvedTarget.split(`${sep}AsterDrive${sep}`)[1]
			href = `${githubRoot}/${repoRelativePath.split(sep).join('/')}`
		} else {
			return match
		}

		if (fragment) href += `#${fragment}`
		return `](${href})`
	})
}

function pagePath(relativePath) {
	const normalized = relativePath.split(sep).join('/')
	const withoutExtension = normalized.replace(/\.md$/, '')
	if (withoutExtension === 'README' || withoutExtension === 'index') return ''
	if (withoutExtension.endsWith('/README')) return `${withoutExtension.slice(0, -'/README'.length)}/`
	if (withoutExtension.endsWith('/index')) return `${withoutExtension.slice(0, -'/index'.length)}/`
	return `${withoutExtension}/`
}
