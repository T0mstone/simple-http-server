# simple-http-server
This is a simple, configurable http server.

All the configuration is done in a config file, 
which is passed to the binary as the first argument.

## Config file format
The file format is [TOML](https://toml.io/).\
All relative file paths are interpreted as relative to the config file.

#### Global keys
- 'addr' (required): the address (including port) to bind to 
    (this is resolved using the hosts file so you can put e.g. 'localhost')
- 'failsafe_addrs' (optional): the addresses to try one after the other if binding to 'addr' fails (1)
- '404' (optional): the path to the html file that will be displayed with an error 404 response

(1): Trying stops once a working one is found and that one is then used
#### Sections
- 'get_routes' (optional): specify which GET request paths lead to which files (the values are FileObjects)
  - note: if you want to route the root page, you need to specify an empty key, i.e. `"" = "root.html"`
  - the special (optional) 'direct' key has to be a list of FileObjects.
    This is a shorthand for directly using the GET request path to read the file.
    For that reason, absolute paths outside of the config file's directory are disallowed here.
    Examples:
    - `direct = ["a/b"]` is equivalent to `"a/b" = "a/b"`
    - `direct = [{ type = "t", path = "a/b" }]` is equivalent to `"a/b" = { type = "t", path = "a/b" }`
  - the special (optional) 'unspecial' subtable is used to configure routes to URLs
    that would otherwise be parsed as special keys, i.e. 'direct' and 'unspecial'.

#### Other
- A FileObject is either a path (relative or absolute) or a map of the form `{ type = <mime type>, path = <path> }`
- Currently supported inferred Media Types are
    - `text/plain` from `.txt`
    - `text/html` from `.html`
    - `text/css` from `.css`
    - `text/javascript` from `.js`
    - `image/png` from `.png`
    - `image/jpeg` from `.jpg` or `.jpeg`
    - `image/jxl` from `.jxl`
    - `image/svg` from `.svg`
    - `video/mp4` from `.mp4`
    - `video/x-matroska` from `.mkv`
    - `application/pdf` from `.pdf`
    - `application/wasm` from `.wasm`
