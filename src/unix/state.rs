use std::collections::HashMap;
use tokio::sync::mpsc::{channel, Sender};
use tokio::net::{UnixListener, UnixStream};
use tokio_util::codec::{FramedRead,FramedWrite};
use tokio;
use tokio::prelude::*;
use futures::{pin_mut, Sink, future::poll_fn};
use futures_util::StreamExt;
use bytes::{Bytes,BytesMut};
use std::io::{Error};
use std::path::Path;
use std::fs;
use std::u16;

use crate::{Identifier, Shared};
use crate::unix::{ControlProtocolCodec, Frame};

#[derive(Clone)]
pub struct UnixState {
    pub ctl_socket: String,
    connections: HashMap<u16, Sender<Frame>>,
    upstream: Sender<(Identifier, u16, u16, Bytes)>,
}

impl Drop for UnixState {
    fn drop(&mut self) {
        let path = &self.ctl_socket;
        if Path::new(path).exists() {
            let _ = fs::remove_file(path);
        };
    }
}

impl UnixState {
    pub fn new(
        ctl_socket: String,
        upstream: Sender<(Identifier, u16, u16, Bytes)>,
    ) -> UnixState {
        UnixState {
            ctl_socket: ctl_socket,
            connections: HashMap::new(),
            upstream: upstream,
        }
    }

    pub fn shared(self) -> Shared<UnixState> {
        Shared::new(self)
    }
}

impl Shared<UnixState> {
    pub fn ctl_socket(&self) -> String {
        self.read().ctl_socket.clone()
    }

    async fn open_ctl_socket(&self) -> Result<(), tokio::io::Error> {
        if self.read().ctl_socket == "" {
            warn!("No unix control socket path is given");
            return Err(tokio::io::Error::new(tokio::io::ErrorKind::NotFound, "No unix control socket path is given"));
        }
        let mut listener = UnixListener::bind(&self.read().ctl_socket)?;
        loop {
            let (con, _addr) = listener.accept().await?;
            self.handle_new_stream(con).await;
        }
    }

    pub async fn handle_new_stream(
        &self,
        mut connection: UnixStream,
    ) -> Result<(), Error> {
        let (read_half, write_half) = tokio::io::split(connection);
        let mut read_half = FramedRead::new(read_half, ControlProtocolCodec{});
        let write_half = FramedWrite::new(write_half, ControlProtocolCodec{});
        match read_half.next().await {
            Some(Ok(Frame::Bind(src_port))) => {
                    debug!("Received a listen message for port {}", src_port);
                    self.spawn_write_task(src_port, write_half);
                    self.spawn_read_task(src_port, read_half);
            },
            Some(Ok(Frame::Data(_,_,_))) => warn!("Received data on an unix connection that was not bound"),
            Some(Ok(Frame::Error(err))) => warn!("Received an error on an unix connection tat was not bound: {:?}", err),
            Some(Err(err)) => warn!("Error in unix stream: {}", err),
            None => {},
        };
        Ok(())
    }

    fn spawn_read_task(&self, src_port: u16, mut read_half: FramedRead<tokio::io::ReadHalf<tokio::net::UnixStream>, ControlProtocolCodec>) {
        let state = self.clone();
        tokio::spawn(async move {
            loop {
                match read_half.next().await {
                    Some(Ok(Frame::Data(host_id, dst_port, data))) => state.send_frame(host_id, src_port, dst_port, data.into()).await,
                    Some(_) => return warn!("Unhandled frame received on Unix stream."),
                    None => return info!("Unix socket closed.")
                }
            }
        });
    }

    fn spawn_write_task(&self, src_port: u16, write_half: FramedWrite<tokio::io::WriteHalf<tokio::net::UnixStream>, ControlProtocolCodec>) {
        let (sender, receiver) = channel::<Frame>(10);
        self.write().connections.insert(src_port, sender);
        tokio::spawn(async move {
            receiver.map(|f| Ok(f)).forward(write_half).await;
        });
    }

    pub async fn send_frame(&self, host_id: Identifier, src_port: u16, dst_port: u16, data: Bytes) {
        let mut sender = self.write().upstream.clone();
        let res = sender.send((host_id, src_port, dst_port, data))
            .await;
        if let Err(err) = res {
            warn!("Failed to pass message to upstream: {}", err);
        };
    }

    pub fn used_ports(&self) -> Vec<u16> {
        self.read()
            .connections
            .keys()
            .cloned()
            .collect()
    }

    pub async fn deliver_frame(
        &self,
        host_id: Identifier,
        remote_port: u16,
        local_port: u16,
        data: Bytes,
    ) {
        debug!(
            "Received new data from {}:{} to port {} {:?}",
            host_id, remote_port, local_port, data
        );
        let mut socket = if let Some(socket) = self.read().connections.get(&local_port).cloned() {
            socket
        } else {
            return;
        };
        let res = socket.send(Frame::Data(host_id, remote_port, data.to_vec())).await;
        if res.is_err() {
            warn!("Unable to deliver frame locally");
        }
    }

    pub async fn run(&self) -> Result<(), tokio::io::Error> {
        self.open_ctl_socket().await
    }
}
