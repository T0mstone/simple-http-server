// todo: better logging system (also in this module)
mod log {
	use std::process::exit;

	use super::cli::PRINT_README_FLAG;

	pub fn print_readme() -> ! {
		println!("{}", include_str!("../README.md"));
		exit(0)
	}

	pub struct CliMessages(pub Option<String>);

	impl CliMessages {
		pub fn print_usage(&self, success: bool) -> ! {
			let this = self.0.as_deref().unwrap_or("<this>");

			let output = format!(
				"USAGE:
	{this} [--] <path to config file>
		Run the server normally
	{this} -h|--help
		Show this message and exit
	{this} --{PRINT_README_FLAG}
		Write out this software's documentation
		in the form of a README.md file (to stdout)"
			);
			if success {
				println!("{output}");
			} else {
				eprintln!("{output}");
			}
			std::process::exit(!success as i32)
		}

		#[inline]
		pub fn print_help(&self) -> ! {
			println!(concat!("simple-http-server v", env!("CARGO_PKG_VERSION")));
			self.print_usage(true)
		}

		#[inline(always)]
		pub fn err(&self, msg: impl std::fmt::Display) -> ! {
			eprintln!("error: {msg}\n");
			self.print_usage(false)
		}

		#[inline]
		pub fn err_missing_config(&self) -> ! {
			self.err("missing config argument")
		}

		#[inline]
		pub fn err_invalid(&self, s: &str, double: bool) -> ! {
			self.err(format_args!(
				"`-{}{s}` is invalid",
				if double { "-" } else { "" }
			))
		}
	}
}

mod cli {
	use std::ffi::OsString;
	use std::path::PathBuf;

	use super::log::CliMessages;

	pub struct Args {
		pub config: PathBuf,
	}

	pub const PRINT_README_FLAG: &str = "dump-readme";

	pub fn parse_env() -> Args {
		let mut args = std::env::args_os();
		let msg = CliMessages(args.next().map(|s| s.to_string_lossy().to_string()));

		let Some(config) = args
			.next()
			.and_then(|arg| process_options(&msg, arg, args.next()))
		else {
			msg.err_missing_config()
		};
		if args.count() > 0 {
			msg.err("too many arguments")
		}

		Args {
			config: config.into(),
		}
	}

	fn process_options(
		msg: &CliMessages,
		arg: OsString,
		next: Option<OsString>,
	) -> Option<OsString> {
		match arg
			.to_string_lossy()
			.strip_prefix('-')
			.map(|s| s.strip_prefix('-').ok_or(s))
		{
			None => {
				// free arg
				Some(arg)
			}
			Some(Err(s)) => {
				// single `-` => option
				match s {
					"h" => msg.print_help(),
					opt => msg.err_invalid(opt, false),
				}
			}
			Some(Ok(s)) => {
				// double `-` => flag
				match s {
					// empty means just `--`.
					// This marks the end of any arg parsing, so the config file may start with a `-`
					"" => next,
					"help" => msg.print_help(),
					PRINT_README_FLAG => super::log::print_readme(),
					flag => msg.err_invalid(flag, true),
				}
			}
		}
	}
}

mod config {
	use std::collections::HashMap;
	use std::ops::{Deref, DerefMut};
	use std::path::{Path, PathBuf};
	use std::str::FromStr;

	use mime::Mime;
	use serde::Deserialize;

	#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
	#[serde(untagged)]
	pub enum FileObject {
		InferMime(PathBuf),
		ExplicitMime { r#type: String, path: PathBuf },
	}

	impl FileObject {
		pub fn path(&self) -> &Path {
			match self {
				FileObject::InferMime(p) => p,
				FileObject::ExplicitMime { path, .. } => path,
			}
		}

		pub fn path_mut(&mut self) -> &mut PathBuf {
			match self {
				FileObject::InferMime(p) => p,
				FileObject::ExplicitMime { path, .. } => path,
			}
		}

		pub fn process(self) -> (Option<Mime>, PathBuf) {
			match self {
				FileObject::ExplicitMime { r#type, path } => (Mime::from_str(&r#type).ok(), path),
				FileObject::InferMime(path) => {
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
		pub direct: Vec<FileObject>,
		#[serde(default)]
		#[serde(flatten)]
		pub map: HashMap<String, FileObject>,
	}

	impl GetRoutes {
		pub fn sanitize_direct_routes(
			mut self,
			root: &Path,
		) -> (Vec<FileObject>, Vec<String>, Self) {
			debug_assert!(root.is_absolute());
			let made_to_rel = self
				.direct
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
			for r in self.direct {
				if r.path().is_absolute() {
					abs.push(r);
				} else {
					rel.push(r);
				}
			}
			(abs, made_to_rel, Self {
				direct: rel,
				map: self.map,
			})
		}

		pub fn resolve_route(
			&self,
			url: impl AsRef<str>,
			index: Option<&PathBuf>,
		) -> Option<FileObject> {
			let mut url = url.as_ref();
			if url == "direct" {
				url = "%direct";
			}
			match url.strip_prefix('/').unwrap_or(url) {
				"" => index.map(|p| FileObject::ExplicitMime {
					r#type: "text/html".to_string(),
					path: p.clone(),
				}),
				s => self
					.direct
					.iter()
					.find({
						let path: &Path = s.as_ref();
						move |r| r.path() == path
					})
					.or_else(|| self.map.get(s))
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

	impl Config {
		pub fn new(args: crate::cli::Args) -> Result<Self, String> {
			let err_open_file = |e| format!("failed to open file ({e})");

			let s = std::fs::read_to_string(&args.config).map_err(err_open_file)?;
			let mut content: ConfigContent =
				toml::from_str(&s).map_err(|e| format!("malformed config file ({e})"))?;
			let mut root = args
				.config
				.parent()
				.expect("config file path has no parent directory")
				.to_path_buf();

			if root.is_relative() {
				root = std::env::current_dir().map_err(err_open_file)?.join(root);
			}

			// preprocess config
			if let Some(gr) = &mut content.get_routes {
				let (parent, to_rel, mut new_gr) = gr.clone().sanitize_direct_routes(&root);
				// todo: better diagnostics
				if !parent.is_empty() {
					eprintln!(
						"ignoring {} direct files with absolute paths that are not children of the config directory",
						parent.len()
					);
				}
				if !to_rel.is_empty() {
					println!(
						"[info] converted {} direct files witha absolute paths in the config directory to relative paths",
						to_rel.len()
					);
				}
				// convert all paths to absolute
				for path in new_gr
					.direct
					.iter_mut()
					.map(|r| r.path_mut())
					.chain(new_gr.map.values_mut().map(|r| r.path_mut()))
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

		pub fn resolve_route(&self, url: impl AsRef<str>) -> Option<(Option<Mime>, PathBuf)> {
			let route = self
				.get_routes
				.as_ref()?
				.resolve_route(url, self.index.as_ref())?;
			Some(route.process())
		}
	}
}

mod http {
	use std::net::ToSocketAddrs;
	use std::path::Path;

	use axum::body::Body;
	use axum::handler::HandlerWithoutStateExt;
	use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode};
	use tokio::net::TcpListener;

	use super::config::Config;

	pub async fn serve(config: Config) {
		let Some(listener) =
			setup_listener(std::iter::once(&config.addr).chain(&config.failsafe_addrs)).await
		else {
			return;
		};

		let (hm404, e404) = load_404(config.not_found.as_deref()).await;
		let error_404 = (StatusCode::NOT_FOUND, hm404, e404);

		let app = |request: Request<Body>| async move {
			use std::io::ErrorKind;

			if request.method() != Method::GET {
				// the server can only handle get requests
				eprintln!("[error] blocked non-get request: {:?}", request);
				return (StatusCode::METHOD_NOT_ALLOWED, HeaderMap::new(), Vec::new());
			}

			let (mime, path) = match config.resolve_route(request.uri().to_string()) {
				None => {
					eprintln!(
						"[error] blocked request without configured route: GET {}",
						request.uri()
					);
					return error_404.clone();
				}
				Some(x) => x,
			};

			let log_path = path
				.strip_prefix(&config.file_dir)
				.unwrap_or_else(|_| &path);
			println!("[GET {}] open {:?}", request.uri(), log_path);

			match tokio::fs::read(path).await {
				Ok(v) => {
					let hdr = match mime {
						None => HeaderMap::new(),
						Some(t) => {
							let Ok(mime) = HeaderValue::from_str(t.as_ref()) else {
								eprintln!("[error] invalid mime type for header: {t}");
								return (
									StatusCode::INTERNAL_SERVER_ERROR,
									HeaderMap::new(),
									Vec::new(),
								);
							};

							mime_header(mime)
						}
					};
					(StatusCode::OK, hdr, v)
				}
				Err(e) => match e.kind() {
					ErrorKind::NotFound => error_404.clone(),
					_ => (
						StatusCode::INTERNAL_SERVER_ERROR,
						mime_header(HeaderValue::from_static(mime::TEXT_PLAIN_UTF_8.as_ref())),
						format!("error opening file: {e}").into_bytes(),
					),
				},
			}
		};

		if let Err(e) = axum::serve(listener, app.into_make_service()).await {
			eprintln!("[error] server failed: {e}");
		}
	}

	async fn setup_listener(addrs: impl Iterator<Item = &String>) -> Option<TcpListener> {
		for s in addrs {
			match s.to_socket_addrs() {
				Err(e) => eprintln!("warning: no socket addr found for {s:?} ({e})"),
				Ok(addrs) => {
					for addr in addrs {
						match TcpListener::bind(addr).await {
							Err(e) => {
								eprintln!("warning: failed to bind to address {s:?} = {addr} ({e})")
							}
							Ok(tcp) => {
								println!("[info] listening on {addr}");
								return Some(tcp);
							}
						}
					}
				}
			}
		}
		None
	}

	async fn load_404(path: Option<&Path>) -> (HeaderMap, Vec<u8>) {
		if let Some(path) = path {
			match std::fs::read(path) {
				Ok(data) => {
					println!("[info] loaded 404 file");
					return (
						mime_header(HeaderValue::from_static(mime::TEXT_HTML.as_ref())),
						data,
					);
				}
				Err(e) => {
					eprintln!("[error] failed to load 404 file: {e}");
				}
			}
		} else {
			println!("[info] proceeding without 404 file");
		}
		Default::default()
	}

	fn mime_header(mime: HeaderValue) -> HeaderMap {
		HeaderMap::from_iter([(axum::http::header::CONTENT_TYPE, mime)])
	}
}

#[tokio::main]
async fn main() {
	let args = cli::parse_env();

	let cfg = match config::Config::new(args) {
		Ok(x) => x,
		Err(e) => {
			eprintln!("Error while retrieving config: {}", e);
			return;
		}
	};

	http::serve(cfg).await
}
