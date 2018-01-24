use std::collections::HashMap;
use std::rc::Rc;
use std::cell::{RefCell, Ref, RefMut};
use std::io;
use std::error::Error;
use std::string::ToString;
use std::net::SocketAddr;
use openssl::x509::X509;
use openssl::ssl::{SslConnectorBuilder, SslConnector, SslAcceptorBuilder, SslAcceptor, SslMethod,
                   SslVerifyMode, SSL_VERIFY_PEER};
use openssl::stack::Stack;
use futures::{Stream, Future, IntoFuture};
use tokio_core::reactor::Handle;
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_openssl::{SslStream, SslConnectorExt, SslAcceptorExt};

use network::io::{Connection as IoConnection, SharedSocket};
use network::{LocalAddress, NetworkState};
use network::transport::Connection as TransportConnection;
use {Identity, Identifier};

struct Inner {
    id: Identity,
    socket: SharedSocket,
    handle: Handle,
    connections: HashMap<Identifier, TransportConnection>,
    state: NetworkState,
}

#[derive(Clone)]
pub struct Socket(Rc<RefCell<Inner>>);

impl Socket {
    fn new(socket: SharedSocket, handle: Handle, state: NetworkState, id: Identity) -> Socket {
        Socket(Rc::new(RefCell::new(Inner {
            id: id,
            socket: socket,
            handle: handle,
            state: state,
            connections: HashMap::new(),
        })))
    }
    pub fn open(
        address: LocalAddress,
        handle: Handle,
        id: Identity,
        state: NetworkState,
    ) -> io::Result<Socket> {
        let shared_socket = SharedSocket::bind(address, handle.clone())?;
        let acceptor = acceptor_for_id(&id);
        let socket = Self::new(shared_socket.clone(), handle.clone(), state.clone(), id);
        let socket2 = socket.clone();
        let handle2 = handle.clone();
        let task = shared_socket.incoming().for_each(move |stream| {
            let socket2 = socket2.clone();
            let handle2 = handle2.clone();
            let state = state.clone();
            acceptor
                .accept_async(stream)
                .map_err(|err| err.description().to_string())
                .and_then(move |connection| {
                    let id = id_from_connection(&connection)?;
                    let conn = TransportConnection::from_tls_stream(
                        connection,
                        id,
                        handle2,
                        move |id, channel, data| state.deliver_frame(id, channel, data),
                    );
                    socket2.write().connections.insert(id, conn);
                    Ok(())
                })
                .map_err(|err| warn!("Error while accepting connection: {}", err))
        });
        handle.spawn(task);
        Ok(socket)
    }

    fn read(&self) -> Ref<Inner> {
        self.0.borrow()
    }

    fn write(&self) -> RefMut<Inner> {
        self.0.borrow_mut()
    }

    fn get_connection(&self, identifier: &Identifier) -> Option<TransportConnection> {
        self.read().connections.get(identifier).cloned()
    }

    fn open_connection(
        &self,
        identifier: Identifier,
        address: SocketAddr,
    ) -> impl Future<Item = TransportConnection, Error = io::Error> {
        let id = &self.read().id;
        let identifier = id.identifier;
        let connector = connector_for_id(&id);
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
                let conn = TransportConnection::from_tls_stream(
                    stream,
                    identifier,
                    handle,
                    move |id, channel, data| state2.deliver_frame(id, channel, data),
                );
                Ok(conn)
            })
    }
}

fn id_from_connection<S>(connection: &SslStream<S>) -> Result<Identifier, String>
where
    S: AsyncRead + AsyncWrite + 'static,
{
    let session = connection.get_ref().ssl();
    let x509 = session.peer_certificate().ok_or(
        "Client did not provide a certificate",
    )?;
    Identifier::from_x509_certificate(&x509).map_err(|err| {
        format!("Unable to generate identifier from certificate: {}", err)
    })
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
    builder.set_certificate(&id.cert).expect(
        "Unable to get reference to client certificate",
    );
    builder.set_private_key(&id.key).expect(
        "Unable to get a reference to the client key",
    );
    builder.build()
}
