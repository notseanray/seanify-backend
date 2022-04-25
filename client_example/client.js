const WebSocket = require('ws');
const fs = require('fs');
const url = 'ws://127.0.0.1:3030/seanify';
let totalrequest = 0;
let totaltime = 0;
let start = 0;

const connection = new WebSocket(url);

connection.onopen = () => {
	start = new Date();
	totalrequest++;
	connection.send("AUTH shafa test");
	connection.send("REMOVE_PLAYLIST hello")
	//connection.send("REMOVE_PLAYLIST test")
	connection.send("PING ");
	connection.send("CLOSE");
}

connection.onerror = (error) => {
	console.log("WebSocket error: ", error);
}

connection.onmessage = (e) => {
	console.log(e.data);
	let time = new Date() - start;
	totaltime += time;
	console.log(time + " ms");
	connection.close()
}

process.on('SIGINT', function() {
    console.log("\ntotal request: " + totalrequest + " in " + totaltime + " ms");
	console.log("average time: " + totalrequest / totaltime + " ms")
	process.exit();
});

