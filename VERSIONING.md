## Versioning policy
Starting from version `0.3.3`,
the version in `Cargo.toml` is the _next_ (i.e. currently unreleased) version, with a `-dev` suffix.
The exception to this rule are the _release commits_, which change nothing but the version (removing the `-dev` suffix)
and serve as an explicit release marker in the git log.
They are also tagged with their version.
