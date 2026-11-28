import fs from "node:fs";

// Codecov aligns PR diff paths against lcov `SF:` paths. The frontend lcov is
// generated inside frontend-panel/, so its `src/...` paths collide with the
// repo-root Rust `src/` and never match the diff's `frontend-panel/src/...`
// paths — the patch gate then reports fabricated missing lines for frontend
// files (see issue #75). This script prefixes every `SF:` path with the panel
// directory so the uploaded report matches the diff. Only `SF:` lines carry
// paths in this lcov (the `FN:` lines are `<line>,<name>` records scoped to
// their SF block and must stay untouched). Run it on the lcov in place before
// the codecov upload; the coverage artifact shares the rewritten content.
function usage() {
	console.error(
		"Usage: bun scripts/rewrite-frontend-lcov-paths.mjs <lcov-path> <panel-dir>",
	);
	process.exit(1);
}

const [, , lcovPath, panelDir] = process.argv;
if (!lcovPath || !panelDir) usage();

const panelPrefix = `${panelDir.replaceAll("\\", "/").replace(/\/+$/, "")}/`;
const content = fs.readFileSync(lcovPath, "utf8");
let rewrittenCount = 0;
const rewritten = content.split(/\r?\n/).map((line) => {
	if (!line.startsWith("SF:")) return line;
	const file = line.slice(3).replaceAll("\\", "/").replace(/^\.\//, "");
	if (file.startsWith("/") || file.startsWith(panelPrefix)) return line;
	rewrittenCount += 1;
	return `SF:${panelPrefix}${file}`;
});

fs.writeFileSync(lcovPath, rewritten.join("\n"));
console.log(`Rewrote ${rewrittenCount} SF paths with prefix ${panelPrefix}`);
