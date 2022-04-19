CREATE TABLE IF NOT EXISTS auth (
	username NUMERIC NOT NULL,
	password NUMERIC NOT NULL,
	admin BOOL NOT NULL,
	last_login NUMERIC,
	userdata UserData NOT NULL
);

CREATE TABLE IF NOT EXISTS auth (
	username NUMERIC NOT NULL,
	password NUMERIC NOT NULL,
	admin BOOL NOT NULL,
	last_login NUMERIC,
	userdata test NOT NULL 
);

