use std::any::Any;
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::LazyLock;
use std::thread;
use std::time::SystemTime;

use axum::extract::{Path, Query, RawQuery, Request};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use axum_client_ip::{ClientIp, ClientIpSource};
use itertools::Itertools;
use petname::{Generator, Petnames};
use reddit_image_grid::database::{CommonQueries, DB};
use reddit_image_grid::reddit::{self, RedditData, RedditDataPostData, Time};
use reddit_image_grid::template::TemplateParameters;
use reddit_image_grid::{BASE_URL, PORT, StringError, UppercaseFirst, template, with_db};
use serde::Deserialize;
use tokio::sync::RwLock;
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

#[cfg(feature = "proxy")]
fn ip_extractor() -> Extension<ClientIpSource> {
	ClientIpSource::RightmostXForwardedFor.into_extension()
}

#[cfg(not(feature = "proxy"))]
fn ip_extractor() -> Extension<ClientIpSource> {
	ClientIpSource::ConnectInfo.into_extension()
}

async fn real_main() {
	reddit_image_grid::force_lazy_vars();
	let app = Router::new()
		.route("/", get(root))
		.route("/favicon.png", get(favicon))
		.route("/hls.min.js", get(hls_js))
		.route("/render", post(render_json))
		.route("/r/{sub}", get(root_with_sub_redirect))
		.route("/r/{sub}/", get(root_with_sub))
		.route("/r/{sub}/{sort}", get(root_with_sub_sort_redirect))
		.route("/r/{sub}/{sort}/", get(root_with_sub_sort))
		.route("/s/{group}/", get(star_group))
		.route("/s/{group}/add/{id}", post(star_group_submit))
		.layer(middleware::from_fn(log_time))
		.layer(ip_extractor())
		.layer(CatchPanicLayer::custom(handle_panic));

	let bind_addr = format!("0.0.0.0:{}", *PORT);
	let listener = tokio::net::TcpListener::bind(bind_addr)
		.await
		.expect("failed to bind to port");
	if let Ok(addr) = listener.local_addr() {
		tracing::info!("listening on {}", addr);
	}
	axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
		.await
		.expect("failed to start axum");
}

// TODO: populate post cache on server fetch
static POST_CACHE: LazyLock<RwLock<HashMap<String, RedditDataPostData>>> =
	LazyLock::new(|| RwLock::new(HashMap::new()));

async fn star_group(Path(group): Path<String>) -> Result<Html<String>> {
	let res = with_db!(|db: &mut DB| {
		let tx = db.transaction()?;
		let res = tx.get_posts_in_group(&group)?;
		tx.commit()?;
		Ok(res)
	})?;
	let to_render = RedditData::from_posts(res);
	{
		let mut cache = POST_CACHE.write().await;
		for x in to_render.posts() {
			cache.insert(x.id.clone(), x.clone());
		}
	}
	Ok(Html(
		template::get(TemplateParameters::render_ui_stars(group, false, to_render)).await?,
	))
}

fn gen_petname() -> String {
	let pn = Petnames::default();
	pn.generate_raw(&mut rand::thread_rng(), 3)
		.unwrap()
		.into_iter()
		.map(|x| x.uppercase_first())
		.join("")
}

async fn star_group_submit(Path((mut group, id)): Path<(String, String)>) -> Result<String> {
	if group == "new" {
		group = gen_petname();
	}
	let Some(post) = ({ POST_CACHE.read().await.get(&id).cloned() }) else {
		return Err(StringError("failed to find post in cache, try reloading").into());
	};
	with_db!(|db: &mut DB| {
		let tx = db.transaction()?;
		tx.put_post_in_group(&group, post)?;
		tx.commit()?;
		Ok(())
	})?;
	Ok(group)
}

async fn render_json(Query(q): Query<Q2>, Json(payload): Json<RedditData>) -> Result<Html<String>> {
	{
		let mut cache = POST_CACHE.write().await;
		for x in payload.posts() {
			cache.insert(x.id.clone(), x.clone());
		}
	}
	Ok(Html(
		template::get(TemplateParameters::render_grid_items(
			Some(q.sub),
			Some(q.sort.parse()?),
			Some(q.time.parse()?),
			q.autoplay,
			Some(payload),
		))
		.await?,
	))
}

async fn log_time(ClientIp(ip): ClientIp, RawQuery(query): RawQuery, req: Request, next: Next) -> Response {
	let start = SystemTime::now();

	let method = req.method().to_string();
	let path = req.uri().path().to_owned();
	let q = query.as_deref().unwrap_or_default();
	let q_mark = if !q.is_empty() { "?" } else { "" };

	let res = next.run(req).await;

	let end = SystemTime::now();
	tracing::debug!(
		"{} {}{q_mark}{q} from {}: {} ms elapsed",
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
	Ok(Html(template::get(TemplateParameters::landing_page()).await?))
}

async fn root_with_sub(Path(sub): Path<String>, Query(query): Query<Q>) -> Result<Html<String>> {
	Ok(Html(
		template::get(TemplateParameters::render_ui(
			Some(sub),
			None,
			None,
			query.autoplay.unwrap_or(false),
		))
		.await?,
	))
}

async fn root_with_sub_redirect(sub: Path<String>) -> Redirect {
	Redirect::permanent(&format!("{}/r/{}/", *BASE_URL, sub.0))
}

async fn root_with_sub_sort(Path(sub_sort): Path<(String, String)>, Query(query): Query<Q>) -> Result<Html<String>> {
	let time = query.t.map(|x| x.parse::<Time>());
	let time = if let Some(time) = time { Some(time?) } else { None };
	Ok(Html(
		template::get(TemplateParameters::render_ui(
			Some(sub_sort.0),
			Some(sub_sort.1.parse()?),
			time,
			query.autoplay.unwrap_or(false),
		))
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

#[derive(Deserialize)]
struct Q2 {
	sub: String,
	sort: String,
	time: String,
	autoplay: bool,
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
impl_from!(StringError);

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
		tracing::warn!("Internal Server Error {:?}", self.inner);
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
