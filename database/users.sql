CREATE TYPE Song AS (
	song_hash NUMERIC,
	date_added NUMERIC,
	custom_name TEXT
);

CREATE TYPE Playlist AS (
	name TEXT,
	description TEXT,
	image TEXT,
	public_playlist BOOL,
	last_update NUMERIC,
	playlist_songs Song[]
);

CREATE TYPE UserData AS (
	public_profile BOOL,
	display_name TEXT,
	share_status BOOL,
	now_playing TEXT,
	public_status TEXT,
	recent_plays TEXT[],
	followers TEXT[],
	following TEXT[]
);
