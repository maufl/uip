use std::collections::HashMap;
use std::rc::Rc;
use std::cell::{Ref, RefCell, RefMut};
use std::io;
use std::error::Error;
use std::string::ToString;
use std::net::SocketAddr;
use openssl::x509::X509;
use openssl::ssl::{SslAcceptor, SslAcceptorBuilder, SslConnector, SslConnectorBuilder, SslMethod,
                   SslVerifyMode, SSL_VERIFY_PEER};
use openssl::stack::Stack;
use futures::{Future, IntoFuture, Stream};
use tokio_core::reactor::Handle;
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_openssl::{SslAcceptorExt, SslConnectorExt, SslStream};

use network::io::SharedSocket;
use network::{LocalAddress, NetworkState};
use network::transport::Connection;
use {Identifier, Identity};
use network::discovery::request_external_address;

pub struct Inner {
    id: Identity,
    address: LocalAddress,
    socket: SharedSocket,
    handle: Handle,
    pub connections: HashMap<Identifier, Connection>,
    state: NetworkState,
}

#[derive(Clone)]
pub struct Socket(Rc<RefCell<Inner>>);

impl Socket {
    fn new(
        socket: SharedSocket,
        address: LocalAddress,
        handle: Handle,
        state: NetworkState,
        id: Identity,
    ) -> Socket {
        Socket(Rc::new(RefCell::new(Inner {
            id: id,
            address: address,
            socket: socket,
            handle: handle,
            state: state,
            connections: HashMap::new(),
        })))
    }
    pub fn open(
        address: LocalAddress,
        handle: &Handle,
        id: Identity,
        state: NetworkState,
    ) -> io::Result<Socket> {
        let shared_socket = SharedSocket::bind(address, handle.clone())?;
        let socket = Self::new(
            shared_socket.clone(),
            address,
            handle.clone(),
            state.clone(),
            id,
        );
        socket.listen();
        socket.request_external_address();
        Ok(socket)
    }

    pub fn read(&self) -> Ref<Inner> {
        self.0.borrow()
    }

    fn write(&self) -> RefMut<Inner> {
        self.0.borrow_mut()
    }

    pub fn get_connection(&self, identifier: &Identifier) -> Option<Connection> {
        self.read().connections.get(identifier).cloned()
    }

    fn listen(&self) {
        let socket = self.clone();
        let handle = self.read().handle.clone();
        let network_state = self.read().state.clone();
        let acceptor = acceptor_for_id(&self.read().id);
        let task = self.read().socket.incoming().for_each(move |stream| {
            let socket = socket.clone();
            let handle = handle.clone();
            let network_state = network_state.clone();
            acceptor
                .accept_async(stream)
                .map_err(|err| err.description().to_string())
                .and_then(move |connection| {
                    let id = id_from_connection(&connection)?;
                    let conn = Connection::from_tls_stream(
                        connection,
                        id,
                        handle,
                        move |id, src_port, dst_port, data| {
                            network_state.deliver_frame(id, src_port, dst_port, data)
                        },
                    );
                    socket.write().connections.insert(id, conn);
                    Ok(())
                })
                .map_err(|err| warn!("Error while accepting connection: {}", err))
        });
        self.read().handle.spawn(task);
    }

    pub fn open_connection(
        &self,
        identifier: Identifier,
        address: SocketAddr,
    ) -> impl Future<Item = Connection, Error = io::Error> {
        let id = &self.read().id;
        let connector = connector_for_id(id);
        let state = self.read().state.clone();
        let handle = self.read().handle.clone();
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
                    handle,
                    move |id, src_port, dst_port, data| {
                        state2.deliver_frame(id, src_port, dst_port, data)
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
        let task = request_external_address(internal, &self.read().handle)
            .map(move |external| socket.write().address.external = Some(SocketAddr::V4(external)))
            .map_err(|err| warn!("Unable to request external address: {}", err));
        self.read().handle.spawn(task);
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
    let empty_chain: Stack<X509> = Stack::new().expect("unable to build empty cert chain");
    let mut builder = SslAcceptorBuilder::mozilla_modern(
        SslMethod::dtls(),
        &id.key,
        id.cert.as_ref(),
        empty_chain.as_ref(),
    ).expect("Unable to build new SSL acceptor");
    builder.set_verify_callback(SSL_VERIFY_PEER, |_valid, context| {
        context.current_cert().is_some()
    });
    builder.build()
}

fn connector_for_id(id: &Identity) -> SslConnector {
    let mut builder =
        SslConnectorBuilder::new(SslMethod::dtls()).expect("Unable to build new SSL connector");
    builder.set_verify(SslVerifyMode::empty());
    builder
        .set_certificate(&id.cert)
        .expect("Unable to get reference to client certificate");
    builder
        .set_private_key(&id.key)
        .expect("Unable to get a reference to the client key");
    builder.build()
}
