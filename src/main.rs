use mime::Mime;
use rouille::{Response, ResponseBody};
use serde_derive::Deserialize;
use std::collections::HashMap;
use std::io::Write;
use std::net::{SocketAddr, ToSocketAddrs};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use structopt::StructOpt;
use thiserror::Error;

////// ARGS //////

#[derive(StructOpt)]
/// A simple configurable http server
pub struct Args {
    /// The path to the configuration file
    ///
    /// For the format, refer to the README (--dump-readme)
    #[structopt(
        required_unless = "dump-readme",
        default_value_if("dump-readme", None, "")
    )]
    pub config: PathBuf,
    /// Prints the REAMDE and exits (you don't need to provide a config)
    #[structopt(long)]
    pub dump_readme: bool,
}

impl Args {
    #[inline]
    pub fn get() -> Self {
        Self::from_args()
    }
}

////// CONFIG //////

// note: RouteFile is called FileObject in the docs
#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum RouteFile {
    InferMIME(PathBuf),
    ExplicitMIME { r#type: String, path: PathBuf },
}

impl RouteFile {
    pub fn path(&self) -> &Path {
        match self {
            RouteFile::InferMIME(p) => p,
            RouteFile::ExplicitMIME { path, .. } => path,
        }
    }

    pub fn mime_for(&self, extension: &str) -> Option<Mime> {
        match self {
            RouteFile::ExplicitMIME { r#type, .. } => Mime::from_str(r#type).ok(),
            RouteFile::InferMIME(..) => Some(match extension {
                "txt" => Mime::from_str("text/plain").unwrap(),
                "html" => Mime::from_str("text/html").unwrap(),
                "css" => Mime::from_str("text/css").unwrap(),
                "png" => Mime::from_str("image/png").unwrap(),
                _ => return None,
            }),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct ConfigContent {
    pub index: PathBuf,
    pub addr: String,
    #[serde(default)]
    pub failsafe_addrs: Vec<String>,
    #[serde(default)]
    pub host_files: Vec<RouteFile>,
    pub get_routes: Option<HashMap<String, RouteFile>>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct Config {
    pub file_dir: PathBuf,
    pub content: ConfigContent,
}

impl Deref for Config {
    type Target = ConfigContent;

    fn deref(&self) -> &Self::Target {
        &self.content
    }
}

impl DerefMut for Config {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.content
    }
}

#[derive(Debug, Error)]
pub enum LoadConfigError {
    #[error("failed to open file ({0})")]
    Open(#[from] std::io::Error),
    #[error("malformed config file ({0})")]
    Format(#[from] toml::de::Error),
    #[error("invalid socket addrs")]
    InvalidSocketAddrs,
}

impl Config {
    pub fn new(args: Args) -> Result<Self, LoadConfigError> {
        let s = std::fs::read_to_string(&args.config)?;
        let content = toml::from_str(&s)?;
        Ok(Self {
            file_dir: args
                .config
                .parent()
                .expect("config file path has no parent")
                .to_path_buf(),
            content,
        })
    }

    pub fn resolve_path<P: AsRef<Path>>(&self, path: P) -> PathBuf {
        let path = path.as_ref();
        if path.is_relative() {
            self.file_dir.join(path).to_path_buf()
        } else {
            path.to_path_buf()
        }
    }

    pub fn get_route<S: AsRef<str>>(&self, url: S) -> Option<(Option<Mime>, PathBuf)> {
        let url = url.as_ref();
        Some(match url.strip_prefix("/").unwrap_or(url) {
            "" => (
                Some(Mime::from_str("text/html").unwrap()),
                self.resolve_path(&self.index),
            ),
            s => {
                let route = self
                    .host_files
                    .iter()
                    .filter_map(|r| {
                        let path: &Path = s.as_ref();
                        if r.path().is_relative() && r.path() == path {
                            Some(r)
                        } else {
                            None
                        }
                    })
                    .next()
                    .or_else(|| self.get_routes.as_ref()?.get(s))?;
                let path = self.resolve_path(route.path());
                let mime = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .and_then(|e| route.mime_for(e));
                (mime, path)
            }
        })
    }
}

////// HTTP //////

const OK: u16 = 200;
const METHOD_NOT_ALLOWED: u16 = 403;
const INTERNAL_SERVER_ERROR: u16 = 500;

struct SocketAddrs(Vec<String>);

impl ToSocketAddrs for SocketAddrs {
    type Iter = std::vec::IntoIter<SocketAddr>;

    fn to_socket_addrs(&self) -> std::io::Result<Self::Iter> {
        Ok(self
            .0
            .iter()
            .flat_map(|s| match s.to_socket_addrs() {
                // note: a single string can become multiple socket addrs if it e.g. is mapped to multiple ips in /etc/hosts
                Ok(iter) => iter.map(Ok).collect(),
                Err(e) => vec![Err(e)],
            })
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HttpServer {
    pub config: Config,
}

impl HttpServer {
    #[inline]
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    #[inline]
    pub fn empty_response(code: u16) -> Response {
        Response {
            status_code: code,
            headers: vec![],
            data: ResponseBody::empty(),
            upgrade: None,
        }
    }

    pub fn io_error_response(e: std::io::Error) -> Response {
        use std::io::ErrorKind;

        match e.kind() {
            ErrorKind::NotFound => Response::empty_404(),
            _ => Response::text(format!("error opening file: {}", e))
                .with_status_code(INTERNAL_SERVER_ERROR),
        }
    }

    pub fn run(self) {
        let mut addrs = vec![self.config.addr.clone()];
        addrs.append(&mut self.config.failsafe_addrs.clone());

        rouille::start_server(SocketAddrs(addrs), move |request| {
            if request.method() != "GET" {
                // the server can only handle get requests
                eprintln!("[error] blocked non-get request: {:?}", request);
                return Self::empty_response(METHOD_NOT_ALLOWED);
            }

            let (mime, path) = match self.config.get_route(request.url()) {
                None => {
                    eprintln!(
                        "[error] blocked request without configured route: GET {}",
                        request.url()
                    );
                    return Response::empty_404();
                }
                Some(x) => x,
            };

            println!("[GET {}] open {:?}", request.url(), path);

            match std::fs::read(path) {
                Ok(v) => match mime {
                    None => Response {
                        status_code: OK,
                        headers: vec![],
                        data: ResponseBody::from_data(v),
                        upgrade: None,
                    },
                    Some(t) => Response::from_data(t.to_string(), v),
                },
                Err(e) => Self::io_error_response(e),
            }
        })
    }
}

const README: &str = include_str!("../README.md");

fn main() {
    let args = Args::get();

    if args.dump_readme {
        print!("{}", README);
        let _ = std::io::stdout().flush();
        return;
    }

    let cfg = match Config::new(args) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("Error while retrieving config: {}", e);
            return;
        }
    };

    HttpServer::new(cfg).run()
}
