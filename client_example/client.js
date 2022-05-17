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
	//connection.send("SIGN kjsfgdsah lasdjfklsdajf s")
	//connection.send("AUTH nate dumb")
	connection.send("AUTH sean ray")
	//connection.send("QUEUE https://www.youtube.com/watch?v=_hZCsgcKa-g")
	//connection.send("QUEUE https://www.youtube.com/watch?v=j1ZBfxQ8oP0")
	//connection.send("RESET_PFP ")
	connection.send("QUEUE_LIST ")
	connection.send("PING ")
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

