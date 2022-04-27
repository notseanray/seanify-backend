CREATE TABLE IF NOT EXISTS PlaylistData (
	username NUMERIC NOT NULL,
	playlist_name TEXT NOT NULL,
	song_hash NUMERIC NOT NULL,
	song_name TEXT NOT NULL,
	date_added NUMERIC NOT NULL,
	custom_name TEXT
);

CREATE TABLE IF NOT EXISTS Playlist (
	username NUMERIC NOT NULL,
	name TEXT NOT NULL,
	creation_timestamp NUMERIC NOT NULL,
	description TEXT,
	public_playlist BOOL NOT NULL,
	last_update NUMERIC NOT NULL
);

CREATE TYPE UserData AS (
	public_profile BOOL,
	display_name TEXT,
	share_status BOOL,
	now_playing TEXT,
	public_status TEXT,
	recent_plays TEXT[],
	followers NUMERIC[],
	following NUMERIC[]
);
