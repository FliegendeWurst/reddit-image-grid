use std::{env, error::Error, fmt::Display, sync::LazyLock};

pub static USE_SERVER_FETCH: LazyLock<bool> = LazyLock::new(|| {
	env::var("REDDIT_IMAGE_GRID_USE_SERVER_FETCH")
		.map(|x| x != "" && x != "0")
		.unwrap_or(false)
});
pub static BASE_URL: LazyLock<String> = LazyLock::new(|| {
	let val = env::var("REDDIT_IMAGE_GRID_BASE_URL");
	if val.is_err() {
		tracing::error!("REDDIT_IMAGE_GRID_BASE_URL not set");
		std::process::exit(1);
	}
	val.unwrap()
});
pub static PORT: LazyLock<u16> = LazyLock::new(|| {
	let val = env::var("REDDIT_IMAGE_GRID_PORT").map(|x| x.parse::<u16>());
	if val.is_err() {
		tracing::error!("REDDIT_IMAGE_GRID_PORT not set");
		std::process::exit(1);
	}
	let val = val.unwrap();
	if val.is_err() {
		tracing::error!("REDDIT_IMAGE_GRID_PORT not valid");
		std::process::exit(1);
	}
	val.unwrap()
});
pub static DATABASE_PATH: LazyLock<Option<String>> = LazyLock::new(|| env::var("REDDIT_IMAGE_GRID_DATABASE").ok());

pub fn force_lazy_vars() {
	tracing::info!("base URL: {}", LazyLock::force(&BASE_URL));
	tracing::info!("port: {}", LazyLock::force(&PORT));
	tracing::info!("server-side JSON fetch: {:?}", LazyLock::force(&USE_SERVER_FETCH));
	tracing::info!("database: {:?}", LazyLock::force(&DATABASE_PATH));
}

pub mod database;
pub mod reddit;
pub mod template;

#[derive(Debug)]
pub struct StringError(pub &'static str);

impl Error for StringError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		None
	}

	fn description(&self) -> &str {
		self.0
	}

	fn cause(&self) -> Option<&dyn Error> {
		None
	}
}

impl Display for StringError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}
