use std::{cell::RefCell, error::Error};

use rusqlite::{Connection, Transaction, params};

use crate::{DATABASE_PATH, StringError, reddit::RedditDataPostData};

#[macro_export]
macro_rules! extract_row {
		($($t:ty)*) => {
				|_row| {
						let mut _i = 0usize;
						Ok(($(_row.get::<_, $t>({ _i += 1; _i - 1 })?),*))
				}
		};
}

thread_local! {
	pub static DATABASE: RefCell<Option<DB>> = RefCell::new(None);
}

pub struct DB {
	db: Connection,
}

impl DB {
	pub fn new() -> Result<Self, Box<dyn Error>> {
		if let Some(db_path) = &*DATABASE_PATH {
			let db = Connection::open(db_path);
			match db {
				Ok(db) => {
					db.execute(
						"CREATE TABLE IF NOT EXISTS stars(
							group_name TEXT NOT NULL,
							reddit_id TEXT NOT NULL,
							reddit_data TEXT NOT NULL
						) STRICT",
						[],
					)?;
					Ok(DB { db })
				},
				Err(e) => {
					tracing::warn!("failed to open database: {:?}", e);
					Err(Box::new(e))
				},
			}
		} else {
			Err(Box::new(StringError("database not configured")))
		}
	}

	pub fn transaction(&mut self) -> Result<Transaction, Box<dyn Error>> {
		Ok(self.db.transaction()?)
	}
}

pub trait CommonQueries {
	fn get_posts_in_group(&self, group: &str) -> Result<Vec<RedditDataPostData>, Box<dyn Error>>;
	fn put_post_in_group(&self, group: &str, post: RedditDataPostData) -> Result<(), Box<dyn Error>>;
}

impl<'conn> CommonQueries for Transaction<'conn> {
	fn get_posts_in_group(&self, group: &str) -> Result<Vec<RedditDataPostData>, Box<dyn Error>> {
		let mut query = self.prepare("SELECT reddit_data FROM stars WHERE group_name = ?")?;
		let rows = query.query_map(params![group], extract_row!(String))?;
		let mut posts = vec![];
		for row in rows {
			let row = row?;
			let data = serde_json::from_str(&row);
			posts.push(data?);
		}
		Ok(posts)
	}

	fn put_post_in_group(&self, group: &str, post: RedditDataPostData) -> Result<(), Box<dyn Error>> {
		let mut query = self.prepare("INSERT INTO stars (group_name, reddit_id, reddit_data) VALUES (?, ?, ?)")?;
		query.execute(params![group, post.id.clone(), serde_json::to_string(&post)?])?;
		Ok(())
	}
}

#[macro_export]
macro_rules! with_db {
	($code:expr) => {
		reddit_image_grid::database::DATABASE.with(|db| {
			let mut db = db.borrow_mut();
			if db.is_none() {
				*db = Some(DB::new()?);
			}
			let db: &mut DB = db.as_mut().unwrap();
			let result: std::result::Result<_, Box<dyn std::error::Error>> = $code(db);
			result
		})
	};
}
