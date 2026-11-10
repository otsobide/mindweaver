//! Binding, routing and serving.

use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::Arc;

use axum::middleware;
use axum::routing::{any, MethodRouter};
use axum::Router;

use crate::api::{self, Api};
use crate::auth::{self, Gate};
use crate::events::Events;
use crate::guard::{self, Hosts};
use crate::log::{self, Log, Sink};
use crate::panel::{self, Panel};
use crate::route::{self, Own, API};
use crate::token::Token;
use crate::transcript::Transcript;
use crate::watcher::Watcher;
use crate::writer::{Current, Writer};
use crate::Backend;

/// A server not yet listening: a backend, and how to serve it.
pub struct Server<B: Backend> {
    backend: B,
    panel: Panel,
    log: Option<Sink>,
    port: u16,
    dev_server: Option<String>,
}

impl<B: Backend> Server<B> {
    /// A server for the panel built into this binary, that logs nothing.
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            panel: Panel::default(),
            log: None,
            port: 0,
            dev_server: None,
        }
    }

    /// Serve these files as the panel instead.
    pub fn panel(mut self, panel: Panel) -> Self {
        self.panel = panel;
        self
    }

    /// Hand `sink` a line for every request once it has been answered, with
    /// how long that took, and for every job the writer thread takes: a run
    /// once its lines are worked out, before any of them runs, and again
    /// once they have run, and a switch once it is made or refused, each
    /// with how long it waited its turn and took. The launch token is
    /// taken out of every line, wherever it appears.
    pub fn log(mut self, sink: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.log = Some(Arc::new(sink));
        self
    }

    /// Listen on `port` rather than on one the system picks; 0 still asks
    /// for any.
    ///
    /// For a client that must know the port before the server starts, as
    /// the panel's dev server does, whose proxy names it in its settings.
    /// A port somebody else holds makes [`bind`](Self::bind) fail rather
    /// than take another, where that client would never find it.
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Serve the panel through its dev server, at `origin` as a browser
    /// writes one (`http://127.0.0.1:5173`): take requests from that page
    /// as from the server's own, and give the launch link there.
    ///
    /// For working on the panel itself. The dev server serves its files as
    /// they are edited, and proxies `/api` and the launch link here, naming
    /// this server in Host; but a change still says the browser's Origin,
    /// the dev server's, which the check that keeps other pages out refuses
    /// from any page but the server's own. So one exact origin is added,
    /// only when a front end asks for it. The link points there because the
    /// panel is served there: its token is traded for the cookie through
    /// the proxy, and the redirect lands on the dev server's page.
    pub fn dev_server(mut self, origin: impl Into<String>) -> Self {
        self.dev_server = Some(origin.into());
        self
    }

    /// Take a port and issue the launch's token.
    ///
    /// 127.0.0.1 and nothing else, not even as an option: the panel is for
    /// the person at this machine, and every address beyond loopback is a
    /// network where the token would travel in clear text. Port 0 unless the
    /// front end named one ([`port`](Self::port)), so the system picks one
    /// that is free rather than the server failing on a port somebody else
    /// took.
    ///
    /// Synchronous, and std's listener rather than tokio's, so that a front
    /// end can learn the port and print the link before it has a runtime.
    ///
    /// Fails, with nothing served, on a table of routes the backend hands
    /// over that cannot be served ([`Backend::routes`]): a path taken twice,
    /// or no path the router could take as it is spelled.
    pub fn bind(self) -> io::Result<Listening> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, self.port))?;
        let address = listener.local_addr()?;
        let token = Token::issue();
        let dev_server = self.dev_server.clone();
        let events = Events::new();
        let current = Current::default();
        let router = route(
            self,
            &token,
            address.port(),
            events.clone(),
            current.clone(),
        )?;
        Ok(Listening {
            listener,
            address,
            token,
            router,
            dev_server,
            events,
            current,
        })
    }
}

/// A server with its port and its token, ready to serve.
pub struct Listening {
    listener: TcpListener,
    address: SocketAddr,
    token: Token,
    router: Router,
    /// Where the panel is served from instead, when that is a dev server.
    dev_server: Option<String>,
    /// What `/api/v1/events` streams.
    events: Events,
    /// The run under way, which a stop asks to stop.
    current: Current,
}

impl Listening {
    /// Where it listens: 127.0.0.1, on the port the system chose.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// The launch's token, for a front end that talks to the server itself.
    /// A browser gets it through [`Listening::url`] instead.
    pub fn token(&self) -> &str {
        self.token.as_str()
    }

    /// The link that opens the panel: `http://127.0.0.1:<port>/?token=...`,
    /// or the same at the dev server's origin when the panel is served
    /// there ([`Server::dev_server`]). The first request trades the token
    /// for a cookie and drops it from the address bar.
    pub fn url(&self) -> String {
        let origin = match &self.dev_server {
            Some(dev_server) => dev_server.clone(),
            None => format!("http://{}", self.address),
        };
        format!("{origin}/?{}={}", auth::PARAMETER, self.token.as_str())
    }

    /// Where the API is served, every route under it:
    /// `http://127.0.0.1:<port>/api/v1`, for a front end that hands it to a
    /// script beside [`Listening::token`].
    ///
    /// The server's own address even through a dev server
    /// ([`Server::dev_server`]), unlike the link: a script sends the token
    /// as Bearer, which needs no page, and so no dev server to serve one.
    pub fn api(&self) -> String {
        format!("http://{}{API}", self.address)
    }

    /// The name of the cookie the token is kept in on this port.
    pub fn cookie(&self) -> String {
        auth::cookie_name(self.address.port())
    }

    /// The bus `/api/v1/events` streams, for a front end that tells the panel
    /// something of its own or listens without HTTP, as a desktop shell
    /// would.
    pub fn events(&self) -> Events {
        self.events.clone()
    }

    /// Every route, for a caller that wants to send requests in process
    /// rather than over the socket. Each still needs the Host header one over
    /// the socket would carry, naming this server.
    pub fn router(&self) -> Router {
        self.router.clone()
    }

    /// Answer requests until `shutdown` completes, then finish those under
    /// way.
    ///
    /// The run under way is asked to stop first, when it takes a cancel, as
    /// leaving the TUI stops its gather: a run waiting on the network would
    /// otherwise hold the stop for as long as the network takes, and one
    /// that takes no cancel runs to its end, as before. Then the event
    /// streams are ended, by closing their bus: each is a request under way
    /// that would otherwise last as long as the browser keeps the page
    /// open, and a graceful stop would wait for it.
    pub async fn serve(
        self,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> io::Result<()> {
        self.listener.set_nonblocking(true)?;
        let listener = tokio::net::TcpListener::from_std(self.listener)?;
        let events = self.events;
        let current = self.current;
        axum::serve(listener, self.router)
            .with_graceful_shutdown(async move {
                shutdown.await;
                // Off the runtime, as every call of the backend's is.
                let asked = events.clone();
                let _ = tokio::task::spawn_blocking(move || current.cancel_any(&asked)).await;
                events.close();
            })
            .await
    }
}

/// Every route, behind what guards it.
fn route<B: Backend>(
    server: Server<B>,
    token: &Token,
    port: u16,
    events: Events,
    current: Current,
) -> io::Result<Router> {
    let routes = B::routes();
    route::check(&routes)?;
    let gate = Arc::new(Gate::new(token.clone(), port));
    let log = server
        .log
        .map(|sink| Arc::new(Log::new(sink, token.clone())));
    let backend = Arc::new(server.backend);
    let transcript = Transcript::default();
    let watched = Arc::clone(&backend);
    let watcher = Arc::new(Watcher::start(
        move || watched.watched_paths(),
        events.clone(),
    ));
    let api = Arc::new(Api {
        writer: Writer::start(
            Arc::clone(&backend),
            transcript.clone(),
            events.clone(),
            current,
            Arc::clone(&watcher),
            log.clone(),
        )?,
        _watcher: watcher,
        backend,
        transcript,
        events,
    });
    // Each of the backend's reads at its path under the version, as its
    // table says: the server matches it, and knows nothing of what it is.
    // Then each of its own, as its own table says, so what a description
    // of the API walks is what is served.
    let mut router = Router::new();
    for read in routes {
        let path = format!("{API}{}", read.path);
        router = router.route(&path, endpoint(api::read::<B>(read)));
    }
    for own in Own::ALL {
        let path = format!("{API}{}", own.path());
        router = router.route(&path, endpoint(api::own::<B>(own)));
    }
    let router = router
        // The bare prefix and everything under it that names no endpoint,
        // so a typo in a script gets a JSON 404 rather than the panel, and
        // so does a path of another version, or of none: the panel's own
        // unversioned endpoints went when it moved under the version, since
        // it ships in the same binary and was their only client. `/api/`
        // needs a route of its own: the catch-all wants at least one
        // character after the slash, and without one the prefix would reach
        // the panel past the gate.
        .route("/api", any(api::missing))
        .route("/api/", any(api::missing))
        .route("/api/{*rest}", any(api::missing))
        // `route_layer`, so the gate guards the routes above and nothing
        // added after them: not the panel, whose files are the same for
        // everybody.
        .route_layer(middleware::from_fn_with_state(
            Arc::clone(&gate),
            auth::require,
        ))
        .with_state(api)
        .fallback(panel::serve)
        .with_state(server.panel)
        // Outside every route, so it sees `/` before anything that serves it.
        .layer(middleware::from_fn_with_state(gate, auth::exchange))
        // Outside the exchange, so nothing is answered to a request that
        // names another host, not even the launch link.
        .layer(middleware::from_fn_with_state(
            Arc::new(Hosts::new(port).dev_server(server.dev_server)),
            guard::same_origin,
        ))
        // Outside the guard, so its refusals carry them too.
        .layer(middleware::from_fn(guard::restrict));
    // Outermost, so a request the guard, the gate or the exchange turns away
    // is logged too.
    Ok(match log {
        Some(log) => router.layer(middleware::from_fn_with_state(log, log::record)),
        None => router,
    })
}

/// An endpoint that answers a method it does not take in JSON, like every
/// other refusal under `/api`.
fn endpoint<S: Clone + Send + Sync + 'static>(route: MethodRouter<S>) -> MethodRouter<S> {
    route.fallback(api::not_allowed)
}
