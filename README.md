# simple-http-server
This is a simple, configurable http server.

All the configuration is done in a config file, 
which is passed to the binary as the first argument.

## Config file format
The file format is TOML.\
All relative paths are interpreted as relative to the config file.

#### Global keys
- 'index' (required): the path to the index html file
- 'addr' (required): the address (including port) to bind to 
    (this is resolved using the hosts file so you can put e.g. 'localhost')
- 'failsafe_addrs' (optional): the addresses to try one after the other if binding to 'addr' fails (1)
- 'host_files' (optional): a list of FileObjects with relative paths which to host at those paths

(1): Trying stops once a working one is found and that one is then used
#### Sections
- 'get_routes' (optional): specify which GET request paths lead to which files (the values are FileObjects)

#### Other
- A FileObject is either a path (relative or absolute) or a map of the form '{ type = <mime type>, path = <path> }'
- Currently supported inferred Media Types are
    - `text/plain` from `.txt`
    - `text/html` from `.html`
    - `text/css` from `.css`
    - `image/png` from `.png`