SU = sudo
PORT = 6000 

psql:
	cd ../seanify && psql --port=$(PORT) -d db_data

prepare:
	-mkdir -p ../seanify/db_data
	-cd ../seanify && initdb -D db_data
	-cd ../seanify && createdb --port=6000 db_data
	cp .env ../seanify

clean:
	-rm ../seanify/seanify

database:
	-prepare
	cd ../seanify && pg_ctl -D db_data -l logfile start

startpsql:
	-make prepare
	cd ../seanify && postgres -D db_data --port=$(PORT)

stoppsql:
	cd ../seanify && pg_ctl -D db_data stop

mkdatabase:
	cd ../seanify && createdb --port=$(PORT) db_data

install:
	cargo build --release
	cp target/release/seanify ../seanify/
