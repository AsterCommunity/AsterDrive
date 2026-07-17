//! Integration coverage for release-profile panic isolation.
//!
//! Stack overflow and allocation failure are process-fatal regardless of this test. This verifies
//! only that an ordinary handler panic does not require the whole Actix server to exit when panic
//! unwinding is enabled.

use std::time::Duration;

use actix_web::{App, HttpResponse, HttpServer, web};

async fn panic_handler() -> HttpResponse {
    panic!("intentional handler panic for worker isolation test");
}

async fn health_handler() -> HttpResponse {
    HttpResponse::Ok().finish()
}

#[actix_web::test]
async fn handler_panic_does_not_stop_other_actix_workers() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("panic isolation test listener should bind");
    let addr = listener
        .local_addr()
        .expect("panic isolation test listener should expose address");

    let server = HttpServer::new(|| {
        App::new()
            .route("/panic", web::get().to(panic_handler))
            .route("/health", web::get().to(health_handler))
    })
    .workers(2)
    .listen(listener)
    .expect("panic isolation test server should listen")
    .run();
    let handle = server.handle();
    let task = tokio::spawn(server);
    let client = reqwest::Client::new();
    let base_url = format!("http://{addr}");

    let panic_response = client.get(format!("{base_url}/panic")).send().await;
    assert!(
        !matches!(panic_response, Ok(response) if response.status().is_success()),
        "panic handler unexpectedly returned a success response"
    );

    let mut health_status = None;
    for _ in 0..20 {
        match client.get(format!("{base_url}/health")).send().await {
            Ok(response) if response.status().is_success() => {
                health_status = Some(response.status());
                break;
            }
            Ok(response) => health_status = Some(response.status()),
            Err(_) => {}
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    handle.stop(true).await;
    task.await
        .expect("panic isolation test server task should stop cleanly")
        .expect("panic isolation test server should stop without an I/O error");

    assert_eq!(health_status, Some(reqwest::StatusCode::OK));
}
