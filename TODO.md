# MVP
* P2P filesystem for user
  * locally exposed as FUSE fs

# Architecture

## API of the daemon
* open(local_service: str?) -> socket | error
* send_msg(sock: socket, remote_device: str, remote_service: str, data: byte[]) -> error?
* connect(sock: socket, remote_device: str, remote_service: str) -> connection
* send(conn: connection, data: byte[]) -> error?
* receive_msg(sock: socket) (remote_service: str, data: byte[])
* receive(conn: connection) -> byte[]

## Network Protocol
* ESTABLISH_CHANNEL(local_service: str, remote_service: str, channel_no: int32)
* DATA(channel_no: int32, data: byte[])
* ERROR{ NO_CHANNEL(channel_no: int32) }

## State the client needs to keep
* Information about Peers
  * Relays
  * Public addresses
* Connections to peers
* Available interfaces
  * State and addresses of interfaces
  * Own public addresses
* Own relays

## User stories
...

# P2P FS

## Network Protocol
* PUSH(hash, data)
* PULL(hash)