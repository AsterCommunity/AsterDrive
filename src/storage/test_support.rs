//! Scripted HTTP peers for storage data-plane and credential protocol tests.
//!
//! A reply belongs to exactly one request. Optional gates pause a response after
//! the request has arrived, allowing tests to exercise real in-flight CAS races
//! without sleeps or depending on scheduler timing.
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
};
use tokio::sync::Notify;

#[derive(Default)]
pub(crate) struct ResponseGate {
    pub entered: Notify,
    pub release: Notify,
}

pub(crate) struct HttpReply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    gate: Option<Arc<ResponseGate>>,
}

impl HttpReply {
    pub fn text(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: body.into().into_bytes(),
            gate: None,
        }
    }
    pub fn json(status: u16, body: serde_json::Value) -> Self {
        Self::text(status, body.to_string()).header("content-type", "application/json")
    }
    pub fn header(mut self, key: &str, value: impl Into<String>) -> Self {
        self.headers.push((key.into(), value.into()));
        self
    }
    pub fn gated(mut self, gate: Arc<ResponseGate>) -> Self {
        self.gate = Some(gate);
        self
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub query: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Default)]
struct Script {
    replies: VecDeque<HttpReply>,
    requests: Vec<RecordedRequest>,
}

pub(crate) struct HttpFixture {
    pub base: String,
    state: Arc<Mutex<Script>>,
    handle: actix_web::dev::ServerHandle,
}

impl HttpFixture {
    pub async fn new(replies: impl IntoIterator<Item = HttpReply>) -> Self {
        async fn respond(
            req: HttpRequest,
            body: web::Bytes,
            state: web::Data<Arc<Mutex<Script>>>,
        ) -> HttpResponse {
            let reply = {
                let mut script = state.lock().unwrap();
                script.requests.push(RecordedRequest {
                    method: req.method().to_string(),
                    path: req.path().to_owned(),
                    query: url::form_urlencoded::parse(req.query_string().as_bytes())
                        .into_owned()
                        .collect(),
                    headers: req
                        .headers()
                        .iter()
                        .map(|(key, value)| (key.to_string(), value.to_str().unwrap().into()))
                        .collect(),
                    body: body.to_vec(),
                });
                script.replies.pop_front()
            };
            let Some(reply) = reply else {
                return HttpResponse::InternalServerError().body("unexpected storage mock request");
            };
            if let Some(gate) = reply.gate {
                gate.entered.notify_one();
                gate.release.notified().await;
            }
            let mut response =
                HttpResponse::build(actix_web::http::StatusCode::from_u16(reply.status).unwrap());
            for (key, value) in reply.headers {
                let value = value.replace(
                    "{base}",
                    &format!("http://{}", req.connection_info().host()),
                );
                response.insert_header((key, value));
            }
            response.body(reply.body)
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(Script {
            replies: replies.into_iter().collect(),
            requests: Vec::new(),
        }));
        let app_state = state.clone();
        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(app_state.clone()))
                .app_data(web::PayloadConfig::new(8 * 1024 * 1024 + 1024))
                .default_service(web::to(respond))
        })
        .workers(1)
        .listen(listener)
        .unwrap()
        .run();
        let handle = server.handle();
        tokio::spawn(server);
        Self {
            base,
            state,
            handle,
        }
    }
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.state.lock().unwrap().requests.clone()
    }
    pub async fn stop(self) {
        self.handle.stop(true).await;
        assert!(
            self.state.lock().unwrap().replies.is_empty(),
            "storage mock left unconsumed replies"
        );
    }
}
