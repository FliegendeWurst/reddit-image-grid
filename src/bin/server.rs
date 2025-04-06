use std::any::Any;
use std::error::Error;
use std::net::SocketAddr;
use std::thread;
use std::time::SystemTime;

use axum::Router;
use axum::extract::{Path, Query, RawQuery, Request};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::get;
use axum_client_ip::{ClientIp, ClientIpSource};
use itertools::Itertools;
use reddit_image_grid::reddit::{self, Time};
use reddit_image_grid::{BASE_URL, PORT, template};
use serde::Deserialize;
use tower_http::catch_panic::CatchPanicLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

type Result<T> = std::result::Result<T, AppError>;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	thread::spawn(|| reddit::work());
	tracing_subscriber::registry()
		.with(
			tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
				// axum logs rejections from built-in extractors with the `axum::rejection`
				// target, at `TRACE` level. `axum::rejection=trace` enables showing those events
				format!(
					"{}=debug,reddit_image_grid=debug,axum::rejection=trace",
					env!("CARGO_CRATE_NAME")
				)
				.into()
			}),
		)
		.with(tracing_subscriber::fmt::layer())
		.init();
	real_main().await;
}

async fn real_main() {
	let app = Router::new()
		.route("/", get(root))
		.route("/favicon.png", get(favicon))
		.route("/hls.min.js", get(hls_js))
		.route("/r/{sub}", get(root_with_sub_redirect))
		.route("/r/{sub}/", get(root_with_sub))
		.route("/r/{sub}/{sort}", get(root_with_sub_sort_redirect))
		.route("/r/{sub}/{sort}/", get(root_with_sub_sort))
		.layer(middleware::from_fn(log_time))
		.layer(ClientIpSource::ConnectInfo.into_extension())
		.layer(ClientIpSource::RightmostXForwardedFor.into_extension())
		.layer(CatchPanicLayer::custom(handle_panic));

	let bind_addr = format!("0.0.0.0:{}", *PORT);
	let listener = tokio::net::TcpListener::bind(bind_addr)
		.await
		.expect("failed to bind to port");
	axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
		.await
		.expect("failed to start axum");
}

async fn log_time(ClientIp(ip): ClientIp, req: Request, next: Next) -> Response {
	let start = SystemTime::now();

	let method = req.method().to_string();
	let path = req.uri().path().to_owned();
	let q = req
		.uri()
		.query()
		.map(|x| serde_urlencoded::from_str::<Vec<(String, String)>>(x).ok())
		.flatten()
		.map(|x| format!("?{}", x.into_iter().map(|y| format!("{}={}", y.0, y.1)).join("&")))
		.unwrap_or_default();

	let res = next.run(req).await;

	let end = SystemTime::now();
	tracing::debug!(
		"{} {}{q} from {}: {} ms elapsed",
		method,
		path,
		ip,
		end.duration_since(start).unwrap().as_millis()
	);

	res
}

async fn favicon() -> impl IntoResponse {
	(
		StatusCode::OK,
		[(header::CONTENT_TYPE, "image/png")],
		include_bytes!("../../favicon.png"),
	)
}

async fn hls_js() -> impl IntoResponse {
	(
		StatusCode::OK,
		[(header::CONTENT_TYPE, "application/javascript")],
		include_bytes!("../../hls.min.js"),
	)
}

#[axum::debug_handler]
async fn root() -> Result<Html<String>> {
	Ok(Html(template::get(None, None, None, false).await?))
}

async fn root_with_sub(Path(sub): Path<String>, Query(query): Query<Q>) -> Result<Html<String>> {
	Ok(Html(
		template::get(Some(sub), None, None, query.autoplay.unwrap_or(false)).await?,
	))
}

async fn root_with_sub_redirect(sub: Path<String>) -> Redirect {
	Redirect::permanent(&format!("{}/r/{}/", *BASE_URL, sub.0))
}

async fn root_with_sub_sort(Path(sub_sort): Path<(String, String)>, Query(query): Query<Q>) -> Result<Html<String>> {
	let time = query.t.map(|x| x.parse::<Time>());
	let time = if let Some(time) = time { Some(time?) } else { None };
	Ok(Html(
		template::get(
			Some(sub_sort.0),
			Some(sub_sort.1.parse()?),
			time,
			query.autoplay.unwrap_or(false),
		)
		.await?,
	))
}

async fn root_with_sub_sort_redirect(sub_sort: Path<(String, String)>, RawQuery(query): RawQuery) -> Redirect {
	Redirect::permanent(&format!(
		"{}/r/{}/{}/{}",
		*BASE_URL,
		sub_sort.0.0,
		sub_sort.0.1,
		query.unwrap_or_default()
	))
}

#[derive(Deserialize)]
struct Q {
	t: Option<String>,
	autoplay: Option<bool>,
}

pub struct AppError {
	inner: Box<dyn Error>,
}

macro_rules! impl_from {
	($type:ty) => {
		impl From<$type> for AppError {
			fn from(value: $type) -> Self {
				Self {
					inner: Box::new(value),
				}
			}
		}
	};
}
impl_from!(std::io::Error);

impl From<Box<dyn Error>> for AppError {
	fn from(value: Box<dyn Error>) -> Self {
		Self { inner: value }
	}
}

impl From<&'static str> for AppError {
	fn from(value: &'static str) -> Self {
		Self::from(std::io::Error::other(value))
	}
}

impl IntoResponse for AppError {
	fn into_response(self) -> Response {
		let msg = format!("{:?}", self.inner);
		(StatusCode::INTERNAL_SERVER_ERROR, msg).into_response()
	}
}

fn handle_panic(err: Box<dyn Any + Send + 'static>) -> Response {
	let mut msg = String::new();
	if let Some(s) = err.downcast_ref::<String>() {
		msg += s;
	} else if let Some(s) = err.downcast_ref::<&str>() {
		msg += s;
	} else {
		msg += "Unknown panic message";
	};
	(StatusCode::INTERNAL_SERVER_ERROR, msg).into_response()
}
