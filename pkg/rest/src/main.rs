use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use serde::Deserialize;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;

const BINARY_PATH: &str = "/opt/jas-min/jas-min";

struct AppState {
    is_running: Mutex<bool>,
}

#[derive(Deserialize)]
struct RunParams {
    param1: Option<String>,
    param2: Option<String>,
}

#[get("/jas-min/status")]
async fn status(data: web::Data<Arc<AppState>>) -> impl Responder {
    let is_running = data.is_running.lock().await;
    if *is_running { "running" } else { "idle" }
}

#[get("/jas-min/run")]
async fn run(data: web::Data<Arc<AppState>>, query: web::Query<RunParams>) -> impl Responder {
    {
        let is_running = data.is_running.lock().await;
        if *is_running {
            return HttpResponse::Conflict().body("already-running");
        }
    }

    {
        let mut is_running = data.is_running.lock().await;
        *is_running = true;
    }

    let mut cmd = Command::new(BINARY_PATH);
    if let Some(ref v) = query.param1 { cmd.arg("--param1").arg(v); }
    if let Some(ref v) = query.param2 { cmd.arg("--param2").arg(v); }
    cmd.stdout(Stdio::null()).stderr(Stdio::null());

    let state = data.get_ref().clone();

    match cmd.spawn() {
        Ok(child) => {
            tokio::spawn(async move {
                let _ = child.wait_with_output().await;
                *state.is_running.lock().await = false;
            });
            HttpResponse::Ok().body("started")
        }
        Err(e) => {
            *data.is_running.lock().await = false;
            HttpResponse::InternalServerError().body(format!("error: {}", e))
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let state = Arc::new(AppState { is_running: Mutex::new(false) });
    println!("JAS-MIN Rest running at http://0.0.0.0:8080");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .service(status)
            .service(run)
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
