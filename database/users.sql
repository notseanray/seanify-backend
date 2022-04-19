CREATE TYPE Song AS (
	hash NUMERIC,
	date_added NUMERIC,
	custom_name TEXT
);

CREATE TYPE Playlist AS (
	name TEXT,
	description TEXT,
	image TEXT,
	pub BOOL,
	last_update NUMERIC,
	playlist_songs Song[],
	server_side BOOL
);

CREATE TYPE UserData AS (
	pub BOOL,
	playlists Playlist[],
	friend_status BOOL,
	display_name TEXT,
	share_status BOOL,
	now_playing TEXT,
	public_status TEXT,
	show_recent BOOL,
	recent_plays TEXT[],
	followers TEXT[],
	following TEXT[]
);
