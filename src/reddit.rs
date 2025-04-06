use std::{
	collections::HashMap,
	fmt,
	str::FromStr,
	sync::{LazyLock, RwLock},
};

use serde::Deserialize;
use tokio::sync::{
	mpsc::{UnboundedSender, unbounded_channel},
	oneshot::Sender,
};
use ureq::Agent;

use crate::StringError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
	New,
	Hot,
	Top,
	Controversial,
}

impl Sort {
	pub fn id(&self) -> &'static str {
		match self {
			Sort::New => "new",
			Sort::Hot => "hot",
			Sort::Top => "top",
			Sort::Controversial => "controversial",
		}
	}
}

impl FromStr for Sort {
	type Err = &'static str;

	fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
		Ok(match s {
			"new" => Sort::New,
			"hot" => Sort::Hot,
			"top" => Sort::Top,
			"controversial" => Sort::Controversial,
			_ => return Err("invalid sort parameter"),
		})
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Time {
	Hour,
	Day,
	Week,
	Month,
	Year,
	All,
}

impl Time {
	pub fn id(&self) -> &'static str {
		use Time::*;
		match self {
			Hour => "hour",
			Day => "day",
			Week => "week",
			Month => "month",
			Year => "year",
			All => "all",
		}
	}
}

impl FromStr for Time {
	type Err = &'static str;

	fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
		use Time::*;
		Ok(match s {
			"hour" => Hour,
			"day" => Day,
			"week" => Week,
			"month" => Month,
			"year" => Year,
			"all" => All,
			_ => return Err("invalid time parameter"),
		})
	}
}

#[derive(Debug)]
pub struct Post {
	pub width: usize,
	pub height: usize,
	pub details: PostDetails,
	pub sub: String,
	pub author: String,
	pub title: String,
	pub permalink: String,
}

#[derive(Debug)]
pub enum PostDetails {
	Image { src_url: String, sizes: Vec<SizedImage> },
	Video { hls_url: String },
	VideoMp4 { mp4_urls: Vec<String> },
	VideoEmbed { html: String },
}

#[derive(Debug)]
pub struct SizedImage {
	pub width: usize,
	pub height: usize,
	pub src_url: String,
}

static CLIENT: LazyLock<Agent> = LazyLock::new(|| {
	Agent::config_builder()
		.user_agent(&format!(
			"linux:reddit-image-grid:{} (by /u/username)",
			env!("CARGO_PKG_VERSION")
		))
		.build()
		.into()
});

static WORK_QUEUE: RwLock<
	Option<UnboundedSender<(String, Sort, Time, u64, Sender<Result<Vec<Post>, anyhow::Error>>)>>,
> = RwLock::new(None);

pub fn work() {
	let (tx, mut rx) = unbounded_channel();
	WORK_QUEUE.write().unwrap().replace(tx);
	while let Some(work) = rx.blocking_recv() {
		let res = get_posts_internal(&*CLIENT, &work.0, work.1, work.2, work.3);
		let _ = work.4.send(res);
	}
}

pub fn get_posts(
	sub: String,
	sort: Sort,
	time: Time,
	limit: u64,
	tx_instance: Sender<Result<Vec<Post>, anyhow::Error>>,
) {
	if let Some(tx) = WORK_QUEUE.read().unwrap().as_ref() {
		tx.send((sub, sort, time, limit, tx_instance)).unwrap();
	}
}

fn get_posts_internal(
	client: &Agent,
	sub: &str,
	sort: Sort,
	time: Time,
	limit: u64,
) -> Result<Vec<Post>, anyhow::Error> {
	let req = client.get(format!(
		"https://www.reddit.com/r/{sub}/{sort}.json?limit={limit}&t={time}&show=all"
	));
	let mut res = req.call()?;
	let body = res.body_mut();
	let json: RedditData = body.read_json()?;

	let mut posts = vec![];

	let mut count_rm = 0;
	let mut count_sm_vid = 0;
	let mut count_sm_embed = 0;
	let mut count_mm = 0;
	let mut count_preview = 0;
	let mut count_other = 0;

	for x in json.data.children.into_iter().map(|x| x.data) {
		if x.removed_by_category.is_some() {
			count_rm += 1;
			continue;
		}
		let author = x.author;
		let sub = x.subreddit;
		let title = x.title;
		let permalink = x.permalink;
		if let Some(sm) = x.secure_media {
			if let Some(rv) = sm.reddit_video {
				let width = rv.width;
				let height = rv.height;
				count_sm_vid += 1;
				let hls_url = rv.hls_url.replace("&amp;", "&");
				posts.push(Post {
					width,
					height,
					details: PostDetails::Video { hls_url },
					author,
					sub,
					title,
					permalink,
				});
			} else if let Some(embed) = sm.oembed {
				count_sm_embed += 1;
				let width = embed.width;
				let height = embed.height;
				if sm.r#type.as_deref() == Some("redgifs.com") || sm.r#type.as_deref() == Some("v3.redgifs.com") {
					let last_slash = embed
						.thumbnail_url
						.rfind('/')
						.ok_or(Box::new(StringError("wrong redgifs url")))?;
					let id = &embed.thumbnail_url[last_slash + 1..embed.thumbnail_url.len() - "-poster.jpg".len()];
					posts.push(Post {
						width,
						height,
						details: PostDetails::VideoMp4 {
							mp4_urls: vec![
								format!("https://media.redgifs.com/{id}-mobile.m4s"),
								format!("https://media.redgifs.com/{id}-mobile.mp4"),
							],
						},
						author,
						sub,
						title,
						permalink,
					});
				} else {
					posts.push(Post {
						width,
						height,
						details: PostDetails::VideoEmbed {
							html: html_escape::decode_html_entities(&embed.html).into_owned(),
						},
						author,
						sub,
						title,
						permalink,
					});
				}
			}
		} else if let Some(mm) = x.media_metadata {
			count_mm += 1;
			for x in mm.values() {
				if let Some(url) = x.s.mp4.as_ref() {
					posts.push(Post {
						width: x.s.x,
						height: x.s.y,
						details: PostDetails::VideoMp4 {
							mp4_urls: vec![url.replace("&amp;", "&")],
						},
						author: author.clone(),
						sub: sub.clone(),
						title: title.clone(),
						permalink: permalink.clone(),
					});
				} else if let Some(u) = x.s.u.as_ref() {
					let src_url = u.replace("&amp;", "&");
					let mut sizes: Vec<_> =
						x.p.iter()
							.flat_map(|x| {
								x.u.as_ref().map(|u| SizedImage {
									width: x.x,
									height: x.y,
									src_url: u.replace("&amp;", "&"),
								})
							})
							.collect();
					sizes.push(SizedImage {
						width: x.s.x,
						height: x.s.y,
						src_url: src_url.clone(),
					});
					posts.push(Post {
						width: x.s.x,
						height: x.s.y,
						details: PostDetails::Image { src_url, sizes },
						author: author.clone(),
						sub: sub.clone(),
						title: title.clone(),
						permalink: permalink.clone(),
					});
				} else {
					// TODO: log
				}
			}
		} else if let Some(p) = x.preview {
			count_preview += 1;
			for img in p.images {
				let img_box = Box::new(img);
				let img = img_box
					.variants
					.as_ref()
					.map(|x| x.get("mp4"))
					.flatten()
					.unwrap_or(&img_box);
				let mut sizes: Vec<_> = img
					.resolutions
					.iter()
					.map(|x| SizedImage {
						width: x.width,
						height: x.height,
						src_url: x.url.replace("&amp;", "&"),
					})
					.collect();
				sizes.push(SizedImage {
					width: img.source.width,
					height: img.source.height,
					src_url: img.source.url.replace("&amp;", "&"),
				});
				let src_url = img.source.url.replace("&amp;", "&");
				if src_url.contains("format=mp4") {
					posts.push(Post {
						width: img.source.width,
						height: img.source.height,
						details: PostDetails::VideoMp4 {
							mp4_urls: vec![src_url],
						},
						author: author.clone(),
						sub: sub.clone(),
						title: title.clone(),
						permalink: permalink.clone(),
					});
				} else {
					posts.push(Post {
						width: img.source.width,
						height: img.source.height,
						details: PostDetails::Image { src_url, sizes },
						author: author.clone(),
						sub: sub.clone(),
						title: title.clone(),
						permalink: permalink.clone(),
					});
				}
			}
		} else {
			count_other += 1;
		}
	}

	tracing::debug!(
		"fetch {sub} sort={sort} t={time}: {count_rm} removed, {count_sm_vid} videos, {count_sm_embed} embeds, {count_mm} galleries, {count_preview} previews, {count_other} other"
	);

	Ok(posts)
}

#[derive(Deserialize, Debug)]
struct RedditData {
	data: RedditDataPosts,
}

#[derive(Deserialize, Debug)]
struct RedditDataPosts {
	children: Vec<RedditDataPost>,
}

#[derive(Deserialize, Debug)]
struct RedditDataPost {
	data: RedditDataPostData,
}

#[derive(Deserialize, Debug)]
struct RedditDataPostData {
	title: String,
	url: String,
	subreddit: String,
	author: String,
	permalink: String,
	// Do not use thumbnail_height/thumbnail_width, they are highly unreliable.
	/// Available for videos
	secure_media: Option<RedditDataSecureMedia>,
	/// Available for videos / non-reddit-hosted images
	preview: Option<RedditDataPreview>,
	/// Available for images / galleries
	media_metadata: Option<HashMap<String, RedditDataMediaImage>>,
	/// If not null: removed for that reason (e.g. copyright)
	removed_by_category: Option<String>,
}

#[derive(Deserialize, Debug)]
struct RedditDataSecureMedia {
	reddit_video: Option<RedditDataRedditVideo>,
	r#type: Option<String>,
	oembed: Option<RedditDataEmbed>,
}

#[derive(Deserialize, Debug)]
struct RedditDataRedditVideo {
	height: usize,
	width: usize,
	/// URL, html-escaped
	hls_url: String,
}

#[derive(Deserialize, Debug)]
struct RedditDataEmbed {
	html: String,
	thumbnail_url: String,
	width: usize,
	height: usize,
}

#[derive(Deserialize, Debug)]
struct RedditDataPreview {
	images: Vec<RedditDataImage>,
}

#[derive(Deserialize, Debug)]
struct RedditDataImage {
	source: RedditDataImage1,
	resolutions: Vec<RedditDataImage1>,
	/// Important key: mp4
	variants: Option<HashMap<String, Box<RedditDataImage>>>,
}

#[derive(Deserialize, Debug)]
struct RedditDataImage1 {
	/// URL, html-escaped
	url: String,
	width: usize,
	height: usize,
}

#[derive(Deserialize, Debug)]
struct RedditDataMediaImage {
	p: Vec<RedditDataMediaImage1>,
	s: RedditDataMediaImage1,
}

#[derive(Deserialize, Debug)]
struct RedditDataMediaImage1 {
	x: usize,
	y: usize,
	/// URL, html-escaped
	u: Option<String>,
	/// If u is None, this may contain an mp4 URL, html-escaped
	mp4: Option<String>,
}

impl fmt::Display for Sort {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", match self {
			Sort::New => "new",
			Sort::Hot => "hot",
			Sort::Top => "top",
			Sort::Controversial => "controversial",
		})
	}
}

impl fmt::Display for Time {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", match self {
			Time::Hour => "hour",
			Time::Day => "day",
			Time::Week => "week",
			Time::Month => "month",
			Time::Year => "year",
			Time::All => "all",
		})
	}
}
