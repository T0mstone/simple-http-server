# 0.3.0

## Breaking Changes
- Implementation: Switched frameworks from `rouille` to `axum`, which may change some behaviors.
- Config: Removed global `index` key. Use `get_routes.""` instead.
- Config: Replaced the old `%direct` escaping with the new `unspecial`.
- CLI: Renamed `--dump-readme` to `--print-readme`.

## Other
- Added MIME type detection for JavaScript, JPEG, JPEG XL, SVG, PDF, and WASM.
