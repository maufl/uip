use std::collections::HashMap;
use std::io;
use std::error::Error;
use std::string::ToString;
use std::net::SocketAddr;
use openssl::ssl::{SslAcceptor, SslConnector, SslMethod, SslVerifyMode};
use bytes::BytesMut;
use tokio;
use tokio::prelude::*;
use tokio_openssl::{SslAcceptorExt, SslConnectorExt, SslStream};

use network::io::Socket as IoSocket;
use network::LocalAddress;
use network::transport::Connection;
use {Identifier, Identity, Shared};
use network::discovery::request_external_address;

pub struct Socket {
    id: Identity,
    address: LocalAddress,
    socket: Shared<IoSocket>,
    pub connections: HashMap<Identifier, Connection>,
    //state: Shared<NetworkState>,
    deliver_frame: Box<Fn(Identifier, u16, u16, BytesMut) + Send + Sync>,
}

impl Socket {
    pub fn new<F>(
        socket: Shared<IoSocket>,
        address: LocalAddress,
        deliver_frame: F,
        //state: Shared<NetworkState>,
        id: Identity,
    ) -> Socket
    where
        F: Fn(Identifier, u16, u16, BytesMut) + Send + Sync + 'static,
    {
        Socket {
            id: id,
            address: address,
            socket: socket,
            //state: state,
            deliver_frame: Box::new(deliver_frame),
            connections: HashMap::new(),
        }
    }

    pub fn shared(self) -> Shared<Socket> {
        Shared::new(self)
    }
}

impl Shared<Socket> {
    pub fn open<F>(
        address: LocalAddress,
        id: Identity,
        deliver_frame: F,
    ) -> io::Result<Shared<Socket>>
    where
        F: Fn(Identifier, u16, u16, BytesMut) + Send + Sync + 'static,
    {
        let shared_socket = Shared::<IoSocket>::bind(address)?;
        let socket = Socket::new(shared_socket.clone(), address, deliver_frame, id).shared();
        socket.listen();
        socket.request_external_address();
        Ok(socket)
    }

    pub fn close(&self) {
        debug!("Closing transport socket");
        self.read().socket.close();
        self.write().connections.clear();
    }

    pub fn get_connection(&self, identifier: &Identifier) -> Option<Connection> {
        self.read().connections.get(identifier).cloned()
    }

    fn listen(&self) {
        let socket = self.clone();
        let acceptor = acceptor_for_id(&self.read().id);
        let task = self.read().socket.incoming().for_each(move |stream| {
            let socket = socket.clone();
            acceptor
                .accept_async(stream)
                .map_err(|err| err.description().to_string())
                .and_then(move |connection| {
                    let id = id_from_connection(&connection)?;
                    let socket2 = socket.clone();
                    let conn = Connection::from_tls_stream(
                        connection,
                        id,
                        move |id, src_port, dst_port, data| {
                            (socket2.read().deliver_frame)(id, src_port, dst_port, data)
                        },
                    );
                    socket.write().connections.insert(id, conn);
                    Ok(())
                })
                .map_err(|err| warn!("Error while accepting connection: {}", err))
        }).map(|_| debug!("Finished accepting incoming connections") )
            .map_err(|_| debug!("Aborted accepting incoming connections") );
        tokio::spawn(task);
    }

    pub fn open_connection(
        &self,
        identifier: Identifier,
        address: SocketAddr,
    ) -> impl Future<Item = Connection, Error = io::Error> {
        let id = &self.read().id;
        let connector = connector_for_id(id);
        let state = self.clone();
        self.read()
            .socket
            .connect(address)
            .into_future()
            .and_then(move |stream| {
                connector
                    .connect_async(&identifier.to_string(), stream)
                    .map_err(|err| {
                        io::Error::new(io::ErrorKind::Other, format!("Handshake error: {:?}", err))
                    })
            })
            .and_then(move |stream| {
                let state2 = state.clone();
                let conn = Connection::from_tls_stream(
                    stream,
                    identifier,
                    move |id, src_port, dst_port, data| {
                        (state2.read().deliver_frame)(id, src_port, dst_port, data)
                    },
                );
                Ok(conn)
            })
    }

    pub fn request_external_address(&self) {
        let socket = self.clone();
        let internal = match self.read().address.internal {
            SocketAddr::V4(addr) => addr,
            _ => return,
        };
        let task = request_external_address(internal)
            .map(move |external| socket.write().address.external = Some(SocketAddr::V4(external)))
            .map_err(|err| warn!("Unable to request external address: {}", err));
        tokio::spawn(task);
    }

    pub fn public_address(&self) -> Option<SocketAddr> {
        let address = self.read().address;
        if let Some(addr) = address.external {
            return Some(addr);
        }
        let internal = address.internal;
        if internal.ip().is_global() {
            return Some(internal);
        }
        None
    }
}

fn id_from_connection<S>(connection: &SslStream<S>) -> Result<Identifier, String>
where
    S: AsyncRead + AsyncWrite + 'static,
{
    let session = connection.get_ref().ssl();
    let x509 = session
        .peer_certificate()
        .ok_or("Client did not provide a certificate")?;
    Identifier::from_x509_certificate(&x509)
        .map_err(|err| format!("Unable to generate identifier from certificate: {}", err))
}

fn acceptor_for_id(id: &Identity) -> SslAcceptor {
    let mut builder =
        SslAcceptor::mozilla_modern(SslMethod::dtls()).expect("Unable to build new SSL acceptor");
    builder.set_private_key(&id.key);
    builder.set_certificate(id.cert.as_ref());
    builder.set_verify_callback(SslVerifyMode::PEER, |_valid, context| {
        context.current_cert().is_some()
    });
    builder.build()
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
