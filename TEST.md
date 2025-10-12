# Testing Rally

## How to Test

### Terminal 1 - Start the Server
```bash
cargo run server
```

You should see:
```
Starting Rally server...
Server listening on 127.0.0.1:7878
```

### Terminal 2 - Run the Client
```bash
cargo run client
```

You should see:
```
Sending message to server...
Client: Starting executor loop...
Client: Connecting to server...
Client: Connected!
Client: Serializing and sending data...
Client: Data sent!
Client: Waiting for response...
Client: Received 14 bytes
Client: Received result after X iterations
Server response: Hello, client!
```

### Expected Server Output
When a client connects, you should see:
```
Worker-1 Received connection: 127.0.0.1:XXXXX
Received message: Data { field1: 42, field2: 1337, field3: "Hello from Rally!" }
```

## What's Happening

1. Client connects to server on port 7878
2. Client sends a serialized `Data` struct with:
   - field1: 42 (u32)
   - field2: 1337 (u16)
   - field3: "Hello from Rally!" (String)
3. Server receives and deserializes the data
4. Server waits 1 second
5. Server responds with "Hello, client!"
6. Client receives and displays the response
