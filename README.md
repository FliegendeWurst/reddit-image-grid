reddit-image-grid
=================

Configurable full screen image/video grid for Reddit.

Try it here: https://fliegendewurst.eu/rig/

- sort by: hot / new / controversial / top
- filter by: last hour / day / week / month / year / all time
- one or more subreddits
- autoplay videos (optional)
- configurable number of columns (1-10)

## Similar tools

- [redditp](https://github.com/ubershmekel/redditp): full screen slide show

## Self-hosting

Run: `cargo install --git https://github.com/FliegendeWurst/reddit-image-grid`.

Set both `REDDIT_IMAGE_GRID_BASE_URL` (without trailing slash) and `REDDIT_IMAGE_GRID_PORT`, then run the `server` binary.

Example: 

```
REDDIT_IMAGE_GRID_BASE_URL=http://127.0.0.1:8080
REDDIT_IMAGE_GRID_PORT=8080
```

If you want to activate the "star" feature, set `REDDIT_IMAGE_GRID_DATABASE`. The server will create an SQLite database at the provided location.

## License

`hls.min.js` is derived from Apache 2.0-licensed [hls.js](https://github.com/video-dev/hls.js/).
`favicon.png` is derived from CC BY-SA 4.0-licensed [Arcticons](https://github.com/Arcticons-Team/Arcticons).

Everything else:

Copyright © 2025 FliegendeWurst

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as
published by the Free Software Foundation, either version 3 of the
License, or (at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/agpl-3.0.html>.
