CREATE TABLE IF NOT EXISTS songs (
	id NUMERIC NOT NULL,
	title TEXT NOT NULL,
	upload_date TEXT,
	uploader TEXT,
	url TEXT,
	genre TEXT,
	thumbnail TEXT,
	album TEXT,
	album_artist TEXT,
	artist TEXT,
	creator TEXT,
	filesize BIGINT,
	downloaded_timestamp NUMERIC,	
	downloaded BOOL NOT NULL
);
