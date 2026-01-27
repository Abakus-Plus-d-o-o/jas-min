use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use serde::Deserialize;
use std::fs::OpenOptions;
use std::path::PathBuf;
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
    plot: Option<String>, // 1 or 0
    time_cpu_ratio: Option<String>,
    mad_threshold: Option<String>,
    mad_window_size: Option<String>,
    security_level: Option<String>,
    parallel: Option<String>,
    ai: Option<String>,          // --ai (model name)
    ai_url: Option<String>,      // .env file
    ai_key: Option<String>,      // .env file
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

    // Resolve directory to one folder higher
    let resolved_dir = if let Some(ref dir) = query.directory {
        PathBuf::from(dir).parent().unwrap_or(PathBuf::from(dir).as_path()).to_path_buf()
    } else {
        PathBuf::from(".")
    };

    // Create output.log file in the resolved directory
    let log_path = resolved_dir.join("output.log");
    let log_file = match OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
    {
        Ok(file) => file,
        Err(e) => {
            *data.is_running.lock().await = false;
            return HttpResponse::InternalServerError().body(format!("error opening log file: {}", e));
        }
    };

    let stdout_file = match log_file.try_clone() {
        Ok(file) => file,
        Err(e) => {
            *data.is_running.lock().await = false;
            return HttpResponse::InternalServerError().body(format!("error cloning log file: {}", e));
        }
    };

    let mut cmd = Command::new(BINARY_PATH);
    if let Some(ref v) = query.directory { cmd.arg("--directory").arg(v); }
    if let Some(ref v) = query.plot { cmd.arg("--plot").arg(v); }
    if let Some(ref v) = query.time_cpu_ratio { cmd.arg("--time-cpu-ratio").arg(v); }
    if let Some(ref v) = query.mad_threshold { cmd.arg("--mad-threshold").arg(v); }
    if let Some(ref v) = query.mad_window_size { cmd.arg("--mad-window-size").arg(v); }
    if let Some(ref v) = query.parallel { cmd.arg("--parallel").arg(v); }
    if let Some(ref v) = query.security_level { cmd.arg("--security-level").arg(v); }
    if let Some(ref v) = query.ai {

        let mut model_name = v;
        // Ollama
        if v.starts_with("ollama") {
            model_name = v.replace("ollama:", "openai:");

            if let Some (ref v) = query.ai_url {
                cmd.env("OPENAI_URL", v);
                cmd.env("OPENAI_API_KEY", "whatever");
            } else {
                return HttpResponse::BadRequest().body("ollama requires --ai-url");
            }
        // OpenAI
        } else if v.starts_with("openai") {
            if let Some(ref v) = query.ai_key {
                cmd.env("OPENAI_API_KEY", v);
            } else {
                return HttpResponse::BadRequest().body("openai requires --ai-key");
            }
        // Google
        } else if v.starts_with("gemini") || v.starts_with("google") {
            model_name = v.replace("gemini:", "google:");
            if let Some(ref v) = query.ai_key {
                cmd.env("GEMINI_API_KEY", query.ai_key);
            } else {
                return HttpResponse::BadRequest().body("openai requires --ai-key");
            }
        }

        cmd.arg("--ai").arg(model_name);
    }

    println!("Running: {:?}", cmd); // Command implements Debug, so this should output something like './binary" "--directory" "/path" "--plot" "1" ...'

    cmd.stdout(Stdio::from(stdout_file)).stderr(Stdio::from(log_file));

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
    println!("JAS-MIN Rest running at http://0.0.0.0:8080/");

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
