use std::{
    net::SocketAddr,
    sync::{mpsc, Arc},
    thread,
    time::Duration,
};

use bytes::Bytes;
use futures::channel::oneshot;
use http_body_util::{BodyExt, Full};
use hyper::server::conn::http1;
use hyper::service::Service;
use hyper::{Request, Response, StatusCode};
use serde::Deserialize;
use tokio::runtime::Runtime;

#[derive(Debug, Clone)]
pub enum Message {
    Start,
    Stop,
    Messages(Vec<RobloxMessage>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum RobloxMessage {
    Output { level: OutputLevel, body: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum OutputLevel {
    Print,
    Info,
    Warning,
    Error,
}

#[derive(Debug)]
pub struct MessageReceiverOptions {
    pub port: u16,
    pub server_id: String,
}

pub struct MessageReceiver {
    shutdown_tx: oneshot::Sender<()>,
    message_rx: mpsc::Receiver<Message>,
}

impl MessageReceiver {
    pub fn start(options: MessageReceiverOptions) -> MessageReceiver {
        let (message_tx, message_rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let server_id = Arc::new(options.server_id.clone());

        thread::spawn(move || {
            // Build a Tokio runtime per thread; small scope so it drops when server exits.
            let rt = Runtime::new().expect("Failed to build Tokio runtime");
            rt.block_on(async move {
                let addr: SocketAddr = ([127, 0, 0, 1], options.port).into();
                let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
                let mut shutdown_rx = shutdown_rx;
                loop {
                    tokio::select! {
                        _ = &mut shutdown_rx => {
                            break;
                        }
                        incoming = listener.accept() => {
                            match incoming {
                                Ok((stream, _peer)) => {
                                    let server_id = server_id.clone();
                                    let message_tx = message_tx.clone();
                                    tokio::spawn(async move {
                                        let service = HyperService { server_id, message_tx };
                                        let io = hyper_util::rt::TokioIo::new(stream);
                                        if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                                            log::error!("hyper connection error: {err}");
                                        }
                                    });
                                }
                                Err(err) => {
                                    log::error!("Accept error: {err}");
                                    break;
                                }
                            }
                        }
                    }
                }
            });
        });

        MessageReceiver {
            shutdown_tx,
            message_rx,
        }
    }

    pub fn recv(&self) -> Message {
        self.message_rx.recv().unwrap()
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Option<Message> {
        self.message_rx.recv_timeout(timeout).ok()
    }

    pub fn stop(self) {
        let _dont_care = self.shutdown_tx.send(());
    }
}

// Hyper service implementation for handling requests.
struct HyperService {
    server_id: Arc<String>,
    message_tx: mpsc::Sender<Message>,
}

impl Service<hyper::Request<hyper::body::Incoming>> for HyperService {
    type Response = Response<Full<Bytes>>;
    type Error = hyper::Error;
    type Future = futures::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn call(&self, req: Request<hyper::body::Incoming>) -> Self::Future {
        let server_id = self.server_id.clone();
        let message_tx = self.message_tx.clone();
        Box::pin(async move {
            let mut response = Response::new(Full::new(Bytes::new()));
            let path = req.uri().path().to_string();
            let method = req.method().clone();
            log::debug!("Request: {} {}", method, path);
            match (method, path.as_str()) {
                (hyper::Method::GET, "/") => {
                    *response.body_mut() = Full::from(server_id.as_str().to_owned());
                }
                (hyper::Method::POST, "/start") => {
                    let _ = message_tx.send(Message::Start);
                    *response.body_mut() = Full::from("Started");
                }
                (hyper::Method::POST, "/stop") => {
                    let _ = message_tx.send(Message::Stop);
                    *response.body_mut() = Full::from("Finished");
                }
                (hyper::Method::POST, "/messages") => {
                    let body_bytes = req.into_body().collect().await?.to_bytes();
                    let messages: Vec<RobloxMessage> = serde_json::from_slice(&body_bytes)
                        .expect("Failed deserializing message from Roblox Studio");
                    let _ = message_tx.send(Message::Messages(messages));
                    *response.body_mut() = Full::from("Got it!");
                }
                _ => {
                    *response.status_mut() = StatusCode::NOT_FOUND;
                }
            }
            Ok(response)
        })
    }
}
