// note the intentional distinction between stdout and stderr:
// stdout is only for things that should be considered *output* of the program,
// so all info, warning and error messages go to stderr.
//
// also note that there is no context (like `tracing` or `async-log`) for the logs,
// but that's fine since all log messages are atomic.
mod log {
	use std::fmt::Display;
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
			error(msg);
			eprintln!(/* blank line for spacing */);
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

	pub fn error(e: impl Display) {
		eprintln!("[error] {e}");
	}

	pub fn warn(w: impl Display) {
		eprintln!("[warn] {w}");
	}

	pub fn info(i: impl Display) {
		eprintln!("[info] {i}");
	}

	/// log a GET request
	pub fn get(uri: impl Display, m: impl Display) {
		// this is to stdout, since what it does with requests *does* count as the output of the program!
		println!("[GET {uri}] {m}");
	}

	/// log an unspecified request
	pub fn req(m: impl Display) {
		// same as with `get`
		println!("[!] {m}");
	}
}

mod cli {
	use std::ffi::OsString;
	use std::path::PathBuf;

	use super::log::CliMessages;

	pub struct Args {
		pub config: PathBuf,
	}

	pub const PRINT_README_FLAG: &str = "print-readme";

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
	use std::str::FromStr;

	use camino::{Utf8Path, Utf8PathBuf};
	use mime::Mime;
	use serde::Deserialize;

	use super::log;

	#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
	#[serde(untagged)]
	pub enum FileObject {
		InferMime(Utf8PathBuf),
		ExplicitMime { r#type: String, path: Utf8PathBuf },
	}

	impl FileObject {
		pub fn path(&self) -> &Utf8Path {
			match self {
				FileObject::InferMime(p) => p,
				FileObject::ExplicitMime { path, .. } => path,
			}
		}

		pub fn path_mut(&mut self) -> &mut Utf8PathBuf {
			match self {
				FileObject::InferMime(p) => p,
				FileObject::ExplicitMime { path, .. } => path,
			}
		}

		pub fn process(self) -> (Option<Mime>, Utf8PathBuf) {
			match self {
				FileObject::ExplicitMime { r#type, path } => (Mime::from_str(&r#type).ok(), path),
				FileObject::InferMime(path) => {
					let mime = path.extension().and_then(|extension| match extension {
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
			root: &Utf8Path,
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
							let res = rel.to_string();
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

		pub fn resolve_route(&self, url: impl AsRef<str>) -> Option<FileObject> {
			let mut url = url.as_ref();
			if url == "direct" {
				url = "%direct";
			}
			let s = url.strip_prefix('/').unwrap_or(url);
			self.direct
				.iter()
				.find({
					let path: &Utf8Path = s.as_ref();
					move |r| r.path() == path
				})
				.or_else(|| self.map.get(s))
				.cloned()
		}
	}

	#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
	pub struct ConfigContent {
		pub addr: String,
		#[serde(default)]
		pub failsafe_addrs: Vec<String>,
		#[serde(rename = "404")]
		pub not_found: Option<Utf8PathBuf>,
		pub get_routes: Option<GetRoutes>,
	}

	#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
	pub struct Config {
		pub file_dir: Utf8PathBuf,
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

			let root = Utf8PathBuf::from_path_buf(root)
				.map_err(|p| format!("config file is in non-UTF8 path: {p:?}"))?;

			// preprocess config
			if let Some(gr) = &mut content.get_routes {
				let (parent, to_rel, mut new_gr) = gr.clone().sanitize_direct_routes(&root);
				// todo: better diagnostics
				if !parent.is_empty() {
					log::warn(format_args!(
						"ignoring {} direct files with absolute paths that are not children of the config directory",
						parent.len()
					));
				}
				if !to_rel.is_empty() {
					log::info(format_args!(
						"converted {} direct files witha absolute paths in the config directory to relative paths",
						to_rel.len()
					));
				}
				// convert all paths to absolute
				for path in new_gr
					.direct
					.iter_mut()
					.map(|r| r.path_mut())
					.chain(new_gr.map.values_mut().map(|r| r.path_mut()))
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

		pub fn resolve_route(&self, url: impl AsRef<str>) -> Option<(Option<Mime>, Utf8PathBuf)> {
			let route = self.get_routes.as_ref()?.resolve_route(url)?;
			Some(route.process())
		}
	}
}

mod http {
	use std::net::ToSocketAddrs;
	use std::path::Path;

	use axum::body::Body;
	use axum::handler::HandlerWithoutStateExt;
	use axum::http::header::CONTENT_TYPE;
	use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode};
	use axum::response::{IntoResponse, IntoResponseParts};
	use mime::Mime;
	use tokio::net::TcpListener;

	use super::config::Config;
	use super::log;

	#[derive(Debug, Clone)]
	struct SetMime(Mime);

	impl IntoResponseParts for SetMime {
		type Error = (StatusCode, HeaderMap, String);

		fn into_response_parts(
			self,
			mut res: axum::response::ResponseParts,
		) -> Result<axum::response::ResponseParts, Self::Error> {
			let value = HeaderValue::from_str(self.0.as_ref()).map_err(|e| {
				(
					StatusCode::INTERNAL_SERVER_ERROR,
					HeaderMap::from_iter([(
						CONTENT_TYPE,
						HeaderValue::from_static(mime::TEXT_PLAIN_UTF_8.as_ref()),
					)]),
					format!("invalid MIME type for header: {e}"),
				)
			})?;
			res.headers_mut().insert(CONTENT_TYPE, value);
			Ok(res)
		}
	}

	#[derive(Debug, Clone)]
	enum Response {
		PureCode(StatusCode),
		MimeBody(StatusCode, Option<SetMime>, Vec<u8>),
	}

	impl IntoResponse for Response {
		fn into_response(self) -> axum::response::Response {
			match self {
				Self::PureCode(c) => c.into_response(),
				Self::MimeBody(c, None, b) => (c, b).into_response(),
				Self::MimeBody(c, Some(m), b) => (c, m, b).into_response(),
			}
		}
	}

	async fn app(config: &Config, error_404: &Response, request: Request<Body>) -> Response {
		use std::io::ErrorKind;

		if request.method() != Method::GET {
			// the server can only handle get requests
			log::req(format_args!("unsupported request: {:?}", request));
			return Response::PureCode(StatusCode::METHOD_NOT_ALLOWED);
		}

		let (mime, path) = match config.resolve_route(request.uri().to_string()) {
			None => {
				log::get(request.uri(), "blocked (no configured route)");
				return error_404.clone();
			}
			Some(x) => x,
		};

		let log_path = path
			.strip_prefix(&config.file_dir)
			.unwrap_or_else(|_| &path);
		log::get(request.uri(), format_args!("open {:?}", log_path));

		match tokio::fs::read(&path).await {
			Ok(v) => Response::MimeBody(StatusCode::OK, mime.map(SetMime), v),
			Err(e) => {
				log::error(format_args!("I/O error at {path:?}: {e}"));
				match e.kind() {
					ErrorKind::NotFound => error_404.clone(),
					_ => Response::MimeBody(
						StatusCode::INTERNAL_SERVER_ERROR,
						Some(SetMime(mime::TEXT_PLAIN_UTF_8)),
						// for security reasons, the client doesn't get the specific error
						"I/O error".to_string().into_bytes(),
					),
				}
			}
		}
	}

	pub async fn serve(config: Config) {
		let Some(listener) =
			setup_listener(std::iter::once(&config.addr).chain(&config.failsafe_addrs)).await
		else {
			return;
		};

		let error_404 = load_404(config.not_found.as_ref()).await;

		let app = move |request| async move { app(&config, &error_404, request).await };

		if let Err(e) = axum::serve(listener, app.into_make_service()).await {
			log::error(format_args!("server failed: {e}"));
		}
	}

	async fn setup_listener(addrs: impl Iterator<Item = &String>) -> Option<TcpListener> {
		for s in addrs {
			match s.to_socket_addrs() {
				Err(e) => log::warn(format_args!("no socket addr found for {s:?} ({e})")),
				Ok(addrs) => {
					for addr in addrs {
						match TcpListener::bind(addr).await {
							Err(e) => {
								log::warn(format_args!(
									"failed to bind to address {s:?} = {addr} ({e})"
								));
							}
							Ok(tcp) => {
								log::info(format_args!("listening on {s:?} = {addr}"));
								return Some(tcp);
							}
						}
					}
				}
			}
		}
		None
	}

	async fn load_404(path: Option<&impl AsRef<Path>>) -> Response {
		if let Some(path) = path {
			match std::fs::read(path) {
				Ok(data) => {
					log::info("loaded 404 file");
					return Response::MimeBody(
						StatusCode::NOT_FOUND,
						Some(SetMime(mime::TEXT_HTML)),
						data,
					);
				}
				Err(e) => {
					log::error(format_args!("failed to load 404 file: {e}"));
				}
			}
		} else {
			log::info("proceeding without 404 file");
		}
		Response::PureCode(StatusCode::NOT_FOUND)
	}
}

#[tokio::main]
async fn main() {
	let args = cli::parse_env();

	let cfg = match config::Config::new(args) {
		Ok(x) => x,
		Err(e) => {
			log::error(format_args!("failed to load config: {e}"));
			return;
		}
	};

	http::serve(cfg).await
}
