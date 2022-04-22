PORT=6000 
DBNAME=seanify_db
SU=doas

# TO USE DO make startpsql and make psql

psql:
	cd ../seanify && psql --port=$(PORT) -d $(DBNAME)

prepare:
	-mkdir -p ../seanify/$(DBNAME)
	cd ../seanify && initdb -D $(DBNAME)
	cd ../seanify && createdb --port=6000 $(DBNAME)
	cp .env ../seanify
	cd ../seanify && pg_ctl --port=$(PORT) -D $(DBNAME) -l logfile start

clean:
	-rm -rf ../seanify/$(DBNAME)

db:
	-mkdir -p ../seanify/$(DBNAME)
	cp .env ../seanify
	-cd ../seanify && initdb -D $(DBNAME) --port=$(PORT)
	cd ../seanify && pg_ctl -o "-F -p $(PORT)" -D $(DBNAME) -l logfile start
	cd ../seanify && postgres -D $(DBNAME) --port=$(PORT) 2>&1 &
	cd ../seanify && createdb --port=6000 $(DBNAME)

startpsql:
	-$(SU) mkdir /run/postgresql
	-$(SU) chown -R sean /run/postgresql

stoppsql:
	cd ../seanify && pg_ctl -D $(DBNAME) stop

install:
	cargo build --release
	cp target/release/seanify ../seanify/

run:
	RUST_LOG=trace cargo run
