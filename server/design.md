#Rust Poker Server 

##Endpoints
All request bodies and responses are JSON

###GET /ping 
Ping endpoint to verify server is up

###POST /register
#[derive(Deserialize)]
struct HandleRegisterBody {
    name: String,
}
Creates a user id. Right now we don't have a long term concept of a user, it is just an UUID with a name and bank balance attached. No password, or storage. Users can create new user id every time they come. There is no database so everything is reset when server reboots. Just a quick casual game.

###GET /user/{id}
#[derive(Debug, Clone, Serialize)]
struct User {
    id: Uuid,
    name: String,
    balance: u32,
}
Gets used as JSON

WEBSOCKET /ws
Opens websocket

##Web Socket Commands
###{"command": "setAuthentication", "userId": <userId>}
Sets the authentication for the web socket

###{"command": "getAuthenitcation"}

###{"command": "joinTable", "tableId": <tableId>, "bank": <bankAmount>}
Join a table with bank amount. Must have authentication set.

###{"command": "leaveTable"}
Leaves a table.

###{"command": "table", "action": {"action": "xxx", ...}}
Sends an action to the table. Must be joined to a table

##Web Socket Table Commands
Must be joined to a table
###{"command": "sit"}

###{"command": "stand}

###{"command": "bet", "betAmount": <betAmount>}

###{"command": "fold"}

##Web Socket Messages
