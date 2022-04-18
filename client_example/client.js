const WebSocket = require('ws')
const fs = require('fs')
const url = 'ws://127.0.0.1:3030/seanify'
const connection = new WebSocket(url)

console.log("sending", "ping")

connection.onopen = () => {
	connection.send("AUTH PATRICK TEST")
	console.log("PING")
	connection.send("DOWNLOAD https://www.youtube.com/watch?v=pCIc0FKD3dM")
}

connection.onerror = (error) => {
	console.log("WebSocket error: ", error)
}

connection.onmessage = (e) => {
	console.log(e.data)
}

