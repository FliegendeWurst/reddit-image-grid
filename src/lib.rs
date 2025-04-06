use std::{env, error::Error, fmt::Display, sync::LazyLock};

pub static BASE_URL: LazyLock<String> =
	LazyLock::new(|| env::var("REDDIT_IMAGE_GRID_BASE_URL").expect("REDDIT_IMAGE_GRID_BASE_URL not set"));
pub static PORT: LazyLock<u16> = LazyLock::new(|| {
	env::var("REDDIT_IMAGE_GRID_PORT")
		.map(|x| x.parse::<u16>())
		.expect("REDDIT_IMAGE_GRID_PORT not set")
		.expect("REDDIT_IMAGE_GRID_PORT invalid")
});

pub mod reddit;
pub mod template;

#[derive(Debug)]
struct StringError(&'static str);

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
