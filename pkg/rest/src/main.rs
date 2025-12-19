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
    directory: Option<String>,
    plot: Option<i32>, // 1 or 0
    time_cpu_ratio: Option<f64>,
    mad_threshold: Option<i32>,
    mad_window_size: Option<i32>,
    security_level: Option<i32>,
    parallelism: Option<i32>,
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
    if let Some(ref v) = query.directory { cmd.arg("--directory").arg(v); }
    if let Some(ref v) = query.plot { cmd.arg("--plot").arg(v); }
    if let Some(ref v) = query.time_cpu_ratio { cmd.arg("--time-cpu-ratio").arg(v); }
    if let Some(ref v) = query.mad_threshold { cmd.arg("--mad-threshold").arg(v); }
    if let Some(ref v) = query.mad_window_size { cmd.arg("--mad-window-size").arg(v); }
    if let Some(ref v) = query.parallel { cmd.arg("--parallel").arg(v); }
    if let Some(ref v) = query.security_level { cmd.arg("--security-level").arg(v); }

    println!("{:?}", cmd); // Command implements Debug, so this should output something like './binary" "--directory" "/path" "--plot" "1" ...'

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
