# shadcn Tailwind stylesheet

Vendored unchanged from npm `shadcn` 4.21.5, `dist/tailwind.css`.
Source: https://github.com/shadcn-ui/ui (MIT; see LICENSE.md).
SHA-256: `4c371f7a1ff5d219ae2f7ff28bd256b4346fd546fe46fbae22092e57db2f0fae`.

The app only imports the stylesheet from this package. Keeping the CSS locally
avoids installing the generator CLI's unrelated dependency graph, including
`braces` versions with no published fix for GHSA-vfj7-8cjw-p6xm. Component
installation remains an explicit `bunx shadcn@latest add` developer operation.
Update this stylesheet and its license together when upgrading the component styles.
