use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::OnceLock;

use mime::Mime;
use rouille::{Response, ResponseBody};
use serde_derive::Deserialize;
use thiserror::Error;

// todo: better logging system

////// ARGS //////

pub struct Args {
	pub config: PathBuf,
}

static INVOK: OnceLock<String> = OnceLock::new();

impl Args {
	pub fn get() -> Self {
		let mut args = std::env::args_os();
		if let Some(invok) = args.next() {
			let _ = INVOK.set(invok.to_string_lossy().to_string());
		}

		let config;

		let Some(arg) = args.next() else {
			Self::missing_config()
		};
		match arg
			.to_string_lossy()
			.strip_prefix('-')
			.map(|s| s.strip_prefix('-').ok_or(s))
		{
			None => {
				// free arg => config file
				config = arg.into();
			}
			Some(Err(s)) => {
				// single `-` => option
				match s {
					"h" => Self::print_help(),
					opt => Self::invalid(opt, false),
				}
			}
			Some(Ok(s)) => {
				// double `-` => flag
				match s {
					"" => {
						// flags-end marker, so the config file may start with a `-`
						let Some(arg) = args.next() else {
							Self::missing_config()
						};
						config = arg.into();
					}
					"help" => Self::print_help(),
					"dump-readme" => Self::dump_readme(),
					flag => Self::invalid(flag, true),
				}
			}
		}
		if args.count() > 0 {
			eprintln!("error: too many arguments\n");
			Self::print_help_body(false)
		}

		Self { config }
	}

	fn print_help_body(success: bool) -> ! {
		let invok = INVOK.get().map(|s| &s[..]).unwrap_or("<this>");
		let helpstr = format!(
			"USAGE: {invok} [--] <path to config file>
For more info, refer to the readme (run `{invok} --dump-readme`)"
		);
		if success {
			println!("{helpstr}");
		} else {
			eprintln!("{helpstr}");
		}
		std::process::exit(!success as i32)
	}

	fn print_help() -> ! {
		println!(concat!("simple-http-server v", env!("CARGO_PKG_VERSION")));
		Self::print_help_body(true)
	}

	fn dump_readme() -> ! {
		println!("{}", include_str!("../README.md"));
		std::process::exit(0)
	}

	fn missing_config() -> ! {
		eprintln!("error: missing config argument\n");
		Self::print_help_body(false)
	}

	fn invalid(s: &str, double: bool) -> ! {
		eprintln!(
			"error: `-{}{s}` is invalid\n",
			if double { "-" } else { "" }
		);
		Self::print_help_body(false)
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

	pub fn path_mut(&mut self) -> &mut PathBuf {
		match self {
			RouteFile::InferMIME(p) => p,
			RouteFile::ExplicitMIME { path, .. } => path,
		}
	}

	pub fn process(self) -> (Option<Mime>, PathBuf) {
		match self {
			RouteFile::ExplicitMIME { r#type, path } => (Mime::from_str(&r#type).ok(), path),
			RouteFile::InferMIME(path) => {
				let mime = path
					.extension()
					.and_then(|e| e.to_str())
					.and_then(|extension| match extension {
						"txt" => Mime::from_str("text/plain").ok(),
						"html" => Mime::from_str("text/html").ok(),
						"css" => Mime::from_str("text/css").ok(),
						"png" => Mime::from_str("image/png").ok(),
						"mp4" | "m4v" => Mime::from_str("video/mp4").ok(),
						// not an official mime type but the suggested one by matroska.org
						"mkv" => Mime::from_str("video/x-matroska").ok(),
						_ => None,
					});

				(mime, path)
			}
		}
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct GetRoutes {
	#[serde(default)]
	#[serde(rename = "direct")]
	pub short: Vec<RouteFile>,
	#[serde(default)]
	#[serde(flatten)]
	pub long: HashMap<String, RouteFile>,
}

impl GetRoutes {
	pub fn remove_parent_files(mut self, root: &Path) -> (Vec<RouteFile>, Vec<String>, Self) {
		debug_assert!(root.is_absolute());
		let made_to_rel = self
			.short
			.iter_mut()
			.filter_map(|r| {
				r.path()
					.strip_prefix(root)
					.ok()
					.map(|rel| rel.to_path_buf())
					.map(|rel| {
						let res = rel.display().to_string();
						*r.path_mut() = rel;
						res
					})
			})
			.collect();
		let mut abs = vec![];
		let mut rel = vec![];
		for r in self.short {
			if r.path().is_absolute() {
				abs.push(r);
			} else {
				rel.push(r);
			}
		}
		// paths in `abs` are not children of the directory that the config file is in
		// and thus wouldn't be reachable from the short routing list
		(abs, made_to_rel, Self {
			short: rel,
			long: self.long,
		})
	}

	pub fn resolve_route<S: AsRef<str>>(
		&self,
		url: S,
		index: Option<&PathBuf>,
	) -> Option<RouteFile> {
		let mut url = url.as_ref();
		if url == "direct" {
			url = "%direct";
		}
		match url.strip_prefix("/").unwrap_or(url) {
			"" => index.map(|p| RouteFile::ExplicitMIME {
				r#type: "text/html".to_string(),
				path: p.clone(),
			}),
			s => self
				.short
				.iter()
				.find({
					let path: &Path = s.as_ref();
					move |r| r.path() == path
				})
				.or_else(|| self.long.get(s))
				.cloned(),
		}
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct ConfigContent {
	pub addr: String,
	#[serde(default)]
	pub failsafe_addrs: Vec<String>,
	pub index: Option<PathBuf>,
	#[serde(rename = "404")]
	pub not_found: Option<PathBuf>,
	pub get_routes: Option<GetRoutes>,
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
		let mut content: ConfigContent = toml::from_str(&s)?;
		let mut root = args
			.config
			.parent()
			.expect("config file path has no parent directory")
			.to_path_buf();

		if root.is_relative() {
			root = std::env::current_dir()?.join(root);
		}

		// preprocess config
		if let Some(gr) = &mut content.get_routes {
			let (parent, to_rel, mut new_gr) = gr.clone().remove_parent_files(&root);
			// todo: better diagnostics
			if !parent.is_empty() {
				eprintln!("ignoring {} direct files with absolute paths that are not children of the config directory", parent.len());
			}
			if !to_rel.is_empty() {
				println!("[info] converted {} direct files witha absolute paths in the config directory to relative paths", to_rel.len());
			}
			// convert all paths to absolute
			for path in new_gr
				.short
				.iter_mut()
				.map(|r| r.path_mut())
				.chain(new_gr.long.values_mut().map(|r| r.path_mut()))
				.chain(content.index.as_mut())
				.chain(content.not_found.as_mut())
			{
				if path.is_relative() {
					*path = root.join(&*path);
				}
			}
			*gr = new_gr;
		}

		Ok(Self {
			file_dir: root,
			content,
		})
	}

	pub fn resolve_route<S: AsRef<str>>(&self, url: S) -> Option<(Option<Mime>, PathBuf)> {
		let route = self
			.get_routes
			.as_ref()?
			.resolve_route(url, self.index.as_ref())?;
		Some(route.process())
	}
}

////// HTTP //////

const OK: u16 = 200;
const METHOD_NOT_ALLOWED: u16 = 405;
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
	cached_404: Option<Vec<u8>>,
}

impl HttpServer {
	pub fn new(config: Config) -> Self {
		Self {
			config,
			cached_404: None,
		}
	}

	pub fn empty_response(code: u16) -> Response {
		Response {
			status_code: code,
			headers: vec![],
			data: ResponseBody::empty(),
			upgrade: None,
		}
	}

	pub fn error_404(&self) -> Response {
		self.cached_404
			.clone()
			.map_or_else(Response::empty_404, |d| {
				Response::from_data("text/html", d).with_status_code(404)
			})
	}

	pub fn io_error_response(&self, e: std::io::Error) -> Response {
		use std::io::ErrorKind;

		match e.kind() {
			ErrorKind::NotFound => self.error_404(),
			_ => Response::text(format!("error opening file: {}", e))
				.with_status_code(INTERNAL_SERVER_ERROR),
		}
	}

	pub fn run(mut self) {
		let mut addrs = vec![self.config.addr.clone()];
		addrs.append(&mut self.config.failsafe_addrs.clone());

		if let Some(path) = &self.config.not_found {
			match std::fs::read(path) {
				Ok(data) => self.cached_404 = Some(data),
				Err(e) => {
					eprintln!("[error] failed to load 404 file: {}", e);
				}
			}
		}

		if self.cached_404.is_some() {
			println!("[info] loaded 404 file");
		} else {
			println!("[info] proceeding without 404 file");
		}

		rouille::start_server(SocketAddrs(addrs), move |request| {
			if request.method() != "GET" {
				// the server can only handle get requests
				eprintln!("[error] blocked non-get request: {:?}", request);
				return Self::empty_response(METHOD_NOT_ALLOWED);
			}

			let (mime, path) = match self.config.resolve_route(request.url()) {
				None => {
					eprintln!(
						"[error] blocked request without configured route: GET {}",
						request.url()
					);
					return self.error_404();
				}
				Some(x) => x,
			};

			let log_path = path.strip_prefix(&self.config.file_dir)
				.unwrap_or_else(|_| &path);
			println!("[GET {}] open {:?}", request.url(), log_path);

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
				Err(e) => self.io_error_response(e),
			}
		})
	}
}

fn main() {
	let args = Args::get();

	let cfg = match Config::new(args) {
		Ok(x) => x,
		Err(e) => {
			eprintln!("Error while retrieving config: {}", e);
			return;
		}
	};

	HttpServer::new(cfg).run()
}
