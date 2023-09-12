# MVP

- P2P filesystem for user
  - locally exposed as FUSE fs

# Architecture

## Layers

### IO Layer: multiplexing of an UDP socket

I need one UDP socket per inferface, so that I have a fixed IP and port that I
can advertise. It can't be connected to a remote, because I need to use it with
all possible remotes. I still need an Connection abstraction because that is
what a DTLS connection needs.

### DTLS

Each IO Connection is wrapped in a DTLS connection.

### Transport

The transport layer implements the UIP transport protocol.

## Daemon implementation

### API of the daemon

- open(local_service: str?) -> socket | error
- send_msg(sock: socket, remote_device: str, remote_service: str, data: byte[])
  -> error?
- connect(sock: socket, remote_device: str, remote_service: str) -> channel
- receive_msg(sock: socket) (remote_device: str, remote_service: str, data:
  byte[])
- send(chan: channel, data: byte[]) -> error?
- receive(channel: chan) -> byte[]

### Interface to the network layer

- send(remote_device: str, remove_service: str, local_service: str, data:
  byte[])
- receive() -> (remote_device: str, remote_service: str, local_service: str,
  data: byte[])

## Transport Protocol

- ESTABLISH_CHANNEL(local_service: str, remote_service: str, channel_no: int32)
- DATA(channel_no: int32, data: byte[])
- ERROR{ NO_CHANNEL(channel_no: int32) }

## State the client needs to keep

- Information about Peers
  - Relays
  - Public addresses
- Connections to peers
- Open sockets
- Available interfaces
  - State and addresses of interfaces
  - Own public addresses
- Own relays

## User stories

...

# P2P FS

## Network Protocol

- PUSH(hash, data)
- PULL(hash)
