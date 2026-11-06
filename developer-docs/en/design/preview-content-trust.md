# Preview content and integration trust boundaries

Uploaded files and administrator-configured applications have different authority. Both may contain scripts, but file access does not grant permission to execute with Drive's origin. A file-content fix must not disable configured embeds or apply a strict sandbox to all viewers.

## Ownership

| Input | Owner | Boundary |
| --- | --- | --- |
| Uploaded bytes, filenames and MIME metadata | File contributor | Reading the file does not authorize application-origin script execution |
| URL templates, allowed origins and display mode | Administrator | Preserve same-origin and explicitly allowed cross-origin applications, iframe/new-tab modes and encoded file variables |
| WOPI action/discovery and additional form fields | Administrator and enabled document application | Preserve POST launch, file-scoped tokens, named iframes, the viewer's own origin, scripts, forms, storage, popups, downloads and existing clipboard permissions |
| Blob/data display URLs | Resource layer and viewer | The original HTTP response's CSP does not prove isolation of a newly created Blob document |

`allow-same-origin` preserves the framed document's own origin; it does not make an external viewer same-origin with Drive. An administrator's same-origin application is an explicit deployment trust choice. Keep the existing distinction between basic custom embed permissions and trusted document-viewer permissions.

## WebDAV content

Current-file and immutable-revision GET/HEAD, Range and conditional responses set `X-Content-Type-Options: nosniff`. HTML, XHTML and SVG reuse the file product's `Content-Security-Policy: sandbox`, including 304. Preserve bytes, MIME, names, ETags, ranges and downloads. The policy does not apply to WOPI application pages, protocol JSON or configured embeds.

## SVG images

An SVG in an `<img>` and a standalone SVG document have different capabilities. An application-origin Blob image URL is also navigable as a document. The resource layer therefore uses a data URL containing the original SVG bytes. Images retain vector graphics, animation and dimensions; a standalone document has an opaque origin and cannot gain Drive's DOM, storage or session-reading authority. This is origin isolation, not SVG sanitization. Original downloads and server-isolated presentation retain the source bytes.

Network fetches and persisted thumbnail restoration use the same display rule; a 304 must not reintroduce an unsafe representation. Ordinary raster images keep Blob URLs. Display URLs are not file identities or authorization proofs.

## PDF toolbar

Uploader-controlled names and MIME do not define a safe navigation type. PDF.js may parse a PDF that also contains trailing HTML accepted by an HTML interpreter. The viewer creates an independent `application/pdf` URL for its loaded Blob and opens only that representation. It never navigates the uploader-typed source URL. Original bytes, download names and normal PDF/octet-stream PDF behavior are preserved. Release viewer-owned URLs on replacement/unmount and prevent stale document reuse.

## External WOPI windows

Submit the actual launch form with `target="_blank" rel="noopener"`. Opening an isolated preliminary window and then posting to a named target does not guarantee isolation of the real viewer and can leave a blank tab. Embedded launches retain their named iframe. Preserve POST tokens, TTLs, extra fields and referrer behavior; do not add `noreferrer` or tighten the viewer sandbox as part of this fix.

## Verification

Use isolated browser instances and local markers to prove uploaded content cannot gain Drive's origin. Check WebDAV bytes and GET/HEAD/Range/304 for both current files and revisions, SVG rendering and navigation, PDF mixed content, and ordinary preview/download/share flows. Install test viewers through the real admin config API to verify same/cross-origin embeds, WOPI scripts/storage, POST fields, iframe permissions and opener isolation. These local viewers and protocol fixtures do not replace acceptance against deployed Collabora, OnlyOffice, Microsoft/Google or a specific custom application.
