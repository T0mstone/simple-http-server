# simple-http-server
This is a simple, configurable http server.

All the configuration is done in a config file, 
which is passed to the binary as the first argument.

## Config file format
The file format is TOML.\
All relative file paths are interpreted as relative to the config file.

#### Global keys
- 'addr' (required): the address (including port) to bind to 
    (this is resolved using the hosts file so you can put e.g. 'localhost')
- 'failsafe_addrs' (optional): the addresses to try one after the other if binding to 'addr' fails (1)
- 'index' (optional): the path to the html file that will be returned if you request the root page (`/`)
- '404' (optional): the path to the html file that will be displayed with an error 404 response

(1): Trying stops once a working one is found and that one is then used
#### Sections
- 'get_routes' (optional): specify which GET request paths lead to which files (the values are FileObjects)
  - the special (optional) 'direct' key has to be a list of FileObjects, each of which routes to itself (2)
  - a route to `direct` can be configured using the key `%direct` instead

(2): see this table:
| entry in 'direct' | equivalent entry in 'get_routes' |
--- | ---
|`"a/b"`|`"a/b" = "a/b"`|
|`{ type = "t", path = "a/b" }`|`"a/b" = { type = "t", path = "a/b" }`|
#### Other
- A FileObject is either a path (relative or absolute) or a map of the form '{ type = <mime type>, path = <path> }'
- Currently supported inferred Media Types are
    - `text/plain` from `.txt`
    - `text/html` from `.html`
    - `text/css` from `.css`
    - `image/png` from `.png`
    - `video/mp4` from `.mp4` or `.m4v`
    - `video/x-matroska` from `.mkv`