const HEX_COLOR_RE = /^#[0-9a-f]{6}$/i;

export const TAG_COLOR_PALETTE = [
	"#2563eb",
	"#0891b2",
	"#059669",
	"#65a30d",
	"#ca8a04",
	"#ea580c",
	"#dc2626",
	"#e11d48",
	"#c026d3",
	"#7c3aed",
	"#4f46e5",
	"#0d9488",
];

export function safeTagColor(color: string | null | undefined) {
	return color && HEX_COLOR_RE.test(color) ? color : "#64748b";
}

export function tagColorFromName(name: string | null | undefined) {
	if (!name) return TAG_COLOR_PALETTE[0];

	const normalized = name.trim().toLowerCase();
	if (!normalized) return TAG_COLOR_PALETTE[0];

	let hash = 2166136261;
	for (const char of normalized) {
		hash ^= char.codePointAt(0) ?? 0;
		hash = Math.imul(hash, 16777619);
	}

	return TAG_COLOR_PALETTE[Math.abs(hash) % TAG_COLOR_PALETTE.length];
}
