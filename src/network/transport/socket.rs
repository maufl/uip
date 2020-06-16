use bytes::Bytes;
use openssl::ssl::{SslAcceptor, SslConnector, SslMethod, SslVerifyMode};
use std::collections::HashMap;
use std::io;
use std::net::SocketAddrV6;
use std::string::ToString;
use tokio;
use tokio::prelude::*;
use tokio::stream::StreamExt;
use tokio::sync::mpsc::Sender;
use tokio_openssl::SslStream;

use crate::network::io::Socket as IoSocket;
use crate::network::transport::Connection;
use crate::network::LocalAddress;
use crate::{Identifier, Identity, Shared};

pub struct Socket {
    id: Identity,
    address: LocalAddress,
    io_socket: Shared<IoSocket>,
    pub connections: HashMap<Identifier, Connection>,
    //state: Shared<NetworkState>,
    deliver_frame: Sender<(Identifier, u16, u16, Bytes)>,
}

impl Socket {
    pub fn new(
        socket: Shared<IoSocket>,
        address: LocalAddress,
        deliver_frame: Sender<(Identifier, u16, u16, Bytes)>,
        //state: Shared<NetworkState>,
        id: Identity,
    ) -> Socket {
        Socket {
            id: id,
            address: address,
            io_socket: socket,
            //state: state,
            deliver_frame: deliver_frame,
            connections: HashMap::new(),
        }
    }

    pub fn shared(self) -> Shared<Socket> {
        Shared::new(self)
    }
}

impl Shared<Socket> {
    pub async fn open(
        address: LocalAddress,
        id: Identity,
        deliver_frame: Sender<(Identifier, u16, u16, Bytes)>,
    ) -> io::Result<Shared<Socket>> {
        let (shared_socket, incomming_connections) = Shared::<IoSocket>::bind(address).await?;
        let socket = Socket::new(shared_socket.clone(), address, deliver_frame, id).shared();
        socket
            .spawn_accept_connections_task(incomming_connections)
            .map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Unable to spawn DTLS acceptor: {}", e),
                )
            })?;
        Ok(socket)
    }

    fn spawn_accept_connections_task(
        &self,
        mut incomming_connections: tokio::sync::mpsc::Receiver<crate::network::io::Connection>,
    ) -> Result<(), openssl::error::ErrorStack> {
        let acceptor = acceptor_for_id(&self.read().id)?;
        let socket = self.clone();
        tokio::spawn(async move {
            loop {
                let stream = match incomming_connections.next().await {
                    Some(s) => s,
                    None => return,
                };
                let ssl_stream = match tokio_openssl::accept(&acceptor, stream).await {
                    Ok(s) => s,
                    Err(err) => {
                        warn!("Error performing TLS handshake: {}", err);
                        continue;
                    }
                };
                let id = match id_from_connection(&ssl_stream) {
                    Ok(id) => id,
                    Err(err) => {
                        warn!("Error authenticating peer: {}", err);
                        continue;
                    }
                };
                let conn = Connection::from_tls_stream(
                    ssl_stream,
                    id,
                    socket.read().deliver_frame.clone(),
                );
                socket.write().connections.insert(id, conn);
            }
        });
        Ok(())
    }

    pub fn close(&self) {
        debug!("Closing transport socket");
        let mut socket = self.write();
        socket.io_socket.close();
        socket.connections.clear();
    }

    pub fn get_connection(&self, identifier: &Identifier) -> Option<Connection> {
        self.read().connections.get(identifier).cloned()
    }

    pub async fn open_connection(
        &self,
        identifier: Identifier,
        address: SocketAddrV6,
    ) -> Result<Connection, io::Error> {
        let connector = match connector_for_id(&self.read().id).configure() {
            Ok(c) => c,
            Err(e) => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Unable to build TLS client configuration: {}", e),
                ))
            }
        };
        let conn = self.read().io_socket.connect(address.into())?;
        let ssl_stream =
            match tokio_openssl::connect(connector, &identifier.to_string(), conn).await {
                Ok(s) => s,
                Err(err) => {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        format!("TLS handshake error: {}", err),
                    ))
                }
            };
        let conn =
            Connection::from_tls_stream(ssl_stream, identifier, self.read().deliver_frame.clone());
        Ok(conn)
    }

    pub fn local_address(&self) -> SocketAddrV6 {
        self.read().address.address
    }
}

fn id_from_connection<S>(connection: &SslStream<S>) -> Result<Identifier, String>
where
    S: AsyncRead + AsyncWrite + 'static,
{
    let session = connection.ssl();
    let x509 = session
        .peer_certificate()
        .ok_or("Client did not provide a certificate")?;
    Identifier::from_x509_certificate(&x509)
        .map_err(|err| format!("Unable to generate identifier from certificate: {}", err))
}

fn acceptor_for_id(id: &Identity) -> Result<SslAcceptor, openssl::error::ErrorStack> {
    let mut builder =
        SslAcceptor::mozilla_modern(SslMethod::dtls()).expect("Unable to build new SSL acceptor");
    builder.set_private_key(&id.key)?;
    builder.set_certificate(id.cert.as_ref())?;
    builder.set_verify_callback(SslVerifyMode::PEER, |_valid, context| {
        context.current_cert().is_some()
    });
    Ok(builder.build())
}

fn connector_for_id(id: &Identity) -> SslConnector {
    let mut builder =
        SslConnector::builder(SslMethod::dtls()).expect("Unable to build new SSL connector");
    builder.set_verify(SslVerifyMode::empty());
    builder
        .set_certificate(&id.cert)
        .expect("Unable to get reference to client certificate");
    builder
        .set_private_key(&id.key)
        .expect("Unable to get a reference to the client key");
    builder.build()
}
