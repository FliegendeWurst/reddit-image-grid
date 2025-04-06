use std::error::Error;

use serde::Serialize;
use tinytemplate::TinyTemplate;
use tokio::sync::oneshot;

use crate::{
	BASE_URL, USE_SERVER_FETCH,
	reddit::{self, RedditData, Sort, Time, make_request_url},
};

pub async fn get(
	sub: Option<String>,
	sort: Option<Sort>,
	time: Option<Time>,
	autoplay: bool,
	data: Option<RedditData>,
) -> Result<String, Box<dyn Error>> {
	let full_page = data.is_none();
	let limit = 25;
	let time = time.unwrap_or(Time::Day);
	let sort = sort.unwrap_or(Sort::Hot);

	let mut cards = vec![];
	let mut any_hls = false;
	if let Some(x) = &sub {
		let posts = if *USE_SERVER_FETCH {
			let (tx, rx) = oneshot::channel();
			reddit::get_posts(x.clone(), sort, time, limit, tx);
			rx.await??
		} else if let Some(json) = data {
			reddit::parse_json(json, x, sort, time)?
		} else {
			vec![]
		};
		for p in posts {
			let width;
			let height;
			// let class;
			let w = p.width as f32;
			let h = p.height as f32;
			//let scale = w / 1000.0;
			//w /= scale;
			//h /= scale;
			/*
			let s = 50.0;
			let a = w/h;
			if a >= 1.5 {
				width = w / s;
				height = width / 2.0;
				if a >= 2.0 {
					class = "fix-width";
				} else {
					class = "fix-height";
				}
			} else if a <= 2.0/3.0 {
				height = h / s;
				width = height / 2.0;
				if a <= 0.5 {
					class = "fix-height";
				} else {
					class = "fix-width";
				}
			} else {
				width = 1000.0 / s;
				height = width;
				if w >= h {
					class = "fix-width";
				} else {
					class = "fix-height";
				}
			}
			*/
			width = 20.0;
			height = h / w * width;
			match p.details {
				reddit::PostDetails::Image { src_url, sizes } => {
					//let sizes_val = sizes.iter().rev().map(|x| format!("(min-width: {}px) 100%", x.width)).join(", ");
					//let srcset = sizes.into_iter().rev().map(|x| format!("{} {}w", x.src_url, x.width)).join(", ");
					cards.push(Card {
						src: src_url,
						mp4_urls: vec![],
						is_hls: false,
						is_mp4: false,
						is_embed: false,
						//srcset,
						//sizes: sizes_val,
						width: width.round() as _,
						height: height.ceil() as _,
						class: "fix-width",
						sub: p.sub,
						user: p.author,
						title: p.title,
						permalink: p.permalink,
					});
				},
				reddit::PostDetails::Video { hls_url } => {
					any_hls = true;
					cards.push(Card {
						src: hls_url,
						mp4_urls: vec![],
						is_hls: true,
						is_mp4: false,
						is_embed: false,
						width: width.round() as _,
						height: height.ceil() as _,
						class: "fix-width",
						sub: p.sub,
						user: p.author,
						title: p.title,
						permalink: p.permalink,
					});
				},
				reddit::PostDetails::VideoMp4 { mp4_urls } => {
					cards.push(Card {
						src: String::new(),
						mp4_urls: mp4_urls,
						is_hls: false,
						is_mp4: true,
						is_embed: false,
						width: width.round() as _,
						height: height.ceil() as _,
						class: "fix-width",
						sub: p.sub,
						user: p.author,
						title: p.title,
						permalink: p.permalink,
					});
				},
				reddit::PostDetails::VideoEmbed { html } => {
					cards.push(Card {
						src: html,
						mp4_urls: vec![],
						is_hls: false,
						is_mp4: false,
						is_embed: true,
						width: width.round() as _,
						height: height.ceil() as _,
						class: "fix-width",
						sub: p.sub,
						user: p.author,
						title: p.title,
						permalink: p.permalink,
					});
				},
			}
		}
	}
	let title;
	if let Some(sub) = &sub {
		if sort != Sort::Hot && sort != Sort::New {
			title = format!("{sub} · {} · {}", sort.id(), time.id())
		} else if sort != Sort::Hot {
			title = format!("{sub} · {}", sort.id());
		} else {
			title = format!("{sub}");
		}
	} else {
		title = "Reddit Image Grid".to_owned();
	};
	let mut tt = TinyTemplate::new();
	let templ = include_str!("grid.html").replace("{\n", "\\{\n");
	tt.add_formatter("raw_html", |val, x| {
		x.push_str(val.as_str().unwrap_or_default());
		Ok(())
	});
	tt.add_template("grid", &templ)?;
	Ok(tt.render("grid", &Context {
		full_page,
		have_data: !cards.is_empty(),
		fetch_url: sub
			.as_ref()
			.map(|sub| make_request_url(sub, sort, time, limit))
			.unwrap_or_default(),
		subs_are_empty: sub.is_none(),
		title,
		one_sub: sub.as_ref().map(|x| !x.contains('+')).unwrap_or(true),
		autoplay,
		base_url: &BASE_URL,
		sort: sort.id(),
		time: time.id(),
		subs_list: sub
			.as_ref()
			.map(|x| x.split('+').map(|x| x.to_owned()).collect())
			.unwrap_or_default(),
		subs: sub.unwrap_or_default().to_owned(),
		sort_controversial: sort == Sort::Controversial,
		sort_new: sort == Sort::New,
		sort_top: sort == Sort::Top,
		sort_hot: sort == Sort::Hot,
		cards,
		time_hour: time == Time::Hour,
		time_day: time == Time::Day,
		time_week: time == Time::Week,
		time_month: time == Time::Month,
		time_year: time == Time::Year,
		time_all: time == Time::All,
		any_hls,
	})?)
}

#[derive(Serialize)]
struct Context {
	full_page: bool,
	have_data: bool,
	fetch_url: String,
	title: String,
	cards: Vec<Card>,
	sort_top: bool,
	sort_new: bool,
	sort_controversial: bool,
	sort_hot: bool,
	autoplay: bool,
	subs: String,
	subs_list: Vec<String>,
	subs_are_empty: bool,
	one_sub: bool,
	base_url: &'static str,
	sort: &'static str,
	time: &'static str,
	time_hour: bool,
	time_day: bool,
	time_week: bool,
	time_month: bool,
	time_year: bool,
	time_all: bool,
	any_hls: bool,
}

#[derive(Serialize)]
struct Card {
	src: String,
	mp4_urls: Vec<String>,
	is_hls: bool,
	is_mp4: bool,
	is_embed: bool,
	//srcset: String,
	//sizes: String,
	width: usize,
	height: usize,
	class: &'static str,
	sub: String,
	user: String,
	title: String,
	permalink: String,
}
