import Papa from "papaparse";
import type { TablePreviewDelimiterValue } from "@/lib/tablePreview";

const MAX_INPUT_CHARS = 1024 * 1024;
const MAX_OUTPUT_CHARS = 256 * 1024;
const MAX_JSON_DEPTH = 64;
const MAX_JSON_ITEMS = 10_000;
const MAX_ROWS = 500;
const MAX_COLUMNS = 50;
const MAX_CELLS = 10_000;

export function formatJsonPreview(content: string) {
	const raw = () => ({
		text: content.slice(0, MAX_OUTPUT_CHARS),
		limited: true,
	});
	if (content.length > MAX_INPUT_CHARS) return raw();

	// Estimate indentation and structural work before parsing/stringifying.
	// Ignore punctuation in strings, including escaped quotes. An overestimate
	// is acceptable: the original text remains readable without a large DOM.
	let quoted = false;
	let escaped = false;
	let depth = 0;
	let items = 0;
	let expanded = content.length;
	for (const char of content) {
		if (quoted) {
			if (escaped) escaped = false;
			else if (char === "\\") escaped = true;
			else if (char === '"') quoted = false;
			continue;
		}
		switch (char) {
			case '"':
				quoted = true;
				break;
			case "{":
			case "[":
				depth += 1;
				items += 1;
				expanded += 2 * depth + 2;
				break;
			case "}":
			case "]":
				expanded += 2 * depth + 2;
				depth = Math.max(0, depth - 1);
				break;
			case ",":
				items += 1;
				expanded += 2 * depth + 2;
				break;
			case ":":
				expanded += 1;
				break;
		}
		if (
			depth > MAX_JSON_DEPTH ||
			items > MAX_JSON_ITEMS ||
			expanded > MAX_OUTPUT_CHARS
		)
			return raw();
	}
	try {
		return {
			text: JSON.stringify(JSON.parse(content), null, 2),
			limited: false,
		};
	} catch {
		return null;
	}
}

export function parseCsvPreview(
	content: string,
	delimiter: TablePreviewDelimiterValue,
) {
	const rows: string[][] = [];
	let limited = content.length > MAX_INPUT_CHARS;
	let invalid = false;
	let cells = 0;
	let characters = 0;
	// Papa's step mode stops parsing as soon as the table is full. Row limits
	// alone do not bound empty cells rendered under a very wide header.
	Papa.parse<string[]>(content.slice(0, MAX_INPUT_CHARS), {
		...(delimiter === "auto"
			? { delimitersToGuess: [",", "\t", ";", "|"] }
			: { delimiter }),
		skipEmptyLines: true,
		step(result, parser) {
			if (result.errors.length > 0) {
				// A bounded prefix can end inside a quoted field. Retain the
				// complete preceding rows rather than report valid CSV as corrupt.
				invalid = !(
					limited &&
					result.errors.every((error) => error.code === "MissingQuotes")
				);
				parser.abort();
				return;
			}
			const width =
				rows[0]?.length ?? Math.min(result.data.length, MAX_COLUMNS);
			if (rows.length >= MAX_ROWS || cells + width > MAX_CELLS) {
				limited = true;
				parser.abort();
				return;
			}
			const row = result.data.slice(0, width);
			const size = row.reduce((total, cell) => total + cell.length, 0);
			if (characters + size > MAX_OUTPUT_CHARS) {
				limited = true;
				parser.abort();
				return;
			}
			limited ||= result.data.length > width;
			cells += width;
			characters += size;
			rows.push(row);
		},
	});
	return { rows, limited, invalid };
}
