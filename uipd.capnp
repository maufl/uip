@0x9944ed01e8923fdf;

interface LocalDevice @0x824df6098c4ffc41 {
    open @0 (local_service: Text) -> (sock: Socket);
}

interface Socket @0xbf9dbe409a5da900 {
    connect @0 (remote_device: Text, remote_service: Text) -> (chan: Channel);
    accept @1 () -> (remote_device: Text, remote_service: Text, chan: Channel);
}

interface Channel @0xffae2c9c441e3a4e {
    send @0 (data: Data);
    recieve @1 () -> (data: Data);
}
