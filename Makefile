PORT = 6000 
DBNAME = seanify_db

psql:
	cd ../seanify && createdb --port=$(PORT) $(DBNAME)
	cd ../seanify && psql --port=$(PORT) -d $(DBNAME)

prepare:
	-mkdir -p ../seanify/$(DBNAME)
	-cd ../seanify && initdb -D $(DBNAME)
	-cd ../seanify && createdb --port=6000 $(DBNAME)
	cp .env ../seanify

clean:
	-rm ../seanify/seanify

database:
	-prepare
	cd ../seanify && pg_ctl -D $(DBNAME) -l logfile start

startpsql:
	-make prepare
	cd ../seanify && postgres -D $(DBNAME) --port=$(PORT) 2>&1 &

stoppsql:
	cd ../seanify && pg_ctl -D $(DBNAME) stop

install:
	cargo build --release
	cp target/release/seanify ../seanify/

run:
	RUST_LOG=trace cargo run
