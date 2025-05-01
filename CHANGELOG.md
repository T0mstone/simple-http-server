# 0.5.0
- Changed how routing works: It now only looks at the [_path_ part](https://docs.rs/http/1.3.1/http/uri/struct.Uri.html#method.path) of the URI.
- Updated dependencies.

# 0.4.1
- Removed one level of indentation from the `--help` output.

# 0.4.0
- Removed inferred mime-type for `.m4v`.
- Added new inferred mime-types.

# 0.3.2
- Updated dependencies to compatible versions.

# 0.3.1
- Updated dependencies.
- Increased MSRV to 1.75.0.

# 0.3.0
## Breaking Changes
- Implementation: Switched frameworks from `rouille` to `axum`, which may change some behaviors.
- Config: Removed global `index` key. Use `get_routes.""` instead.
- Config: Replaced the old `%direct` escaping with the new `unspecial`.
- CLI: Renamed `--dump-readme` to `--print-readme`.

## Other
- Added MIME type detection for JavaScript, JPEG, JPEG XL, SVG, PDF, and WASM.
