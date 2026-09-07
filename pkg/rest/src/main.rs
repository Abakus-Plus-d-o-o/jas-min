use actix_web::{App, HttpRequest, HttpResponse, HttpServer, Responder, get, web};
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;

const BINARY_PATH: &str = "/opt/jas-min/jas-min";

// Keep this allowlist in sync with Args in the main project's src/main.rs.
// Defaults belong to the CLI: only explicitly supplied options are forwarded.
const VALUE_OPTIONS: &[&str] = &[
    "file",
    "directory",
    "outfile",
    "time-cpu-ratio",
    "filter-db-time",
    "id-sqls",
    "json-file",
    "snap-range",
    "ai",
    "mad-top",
    "mad-window-size",
    "top-cluster-anomalies",
    "parallel",
    "security-level",
    "url-context-file",
    "tokens-budget",
    "ridge-lambda",
    "en-lambda",
    "en-alpha",
    "en-max-iter",
    "en-tol",
    "top-gradient",
    "convert-md2html",
    "gradient-custom",
    "max-tool-iterations",
    "mcp",
];
const FLAG_OPTIONS: &[&str] = &["quiet", "tools-mode", "help", "version"];

struct AppState {
    is_running: Mutex<bool>,
}

#[derive(Debug)]
struct RunParams {
    arguments: Vec<String>,
    workdir: PathBuf,
    ai: Option<String>,
    ai_url: Option<String>,
    ai_key: Option<String>,
}

impl RunParams {
    fn from_query(query: &str, service_dir: &Path) -> Result<Self, String> {
        // Decode into pairs to preserve repeated directory/json_file parameters.
        let pairs: Vec<(String, String)> =
            serde_urlencoded::from_str(query).map_err(|error| format!("invalid query: {error}"))?;
        let mut params = Self {
            arguments: Vec::new(),
            workdir: service_dir.to_path_buf(),
            ai: None,
            ai_url: None,
            ai_key: None,
        };
        let mut seen = HashSet::new();
        let mut first_source = None;
        let mut project_count = 0;
        let mut informational = false;
        for (key, mut value) in pairs {
            // Preserve existing snake_case query names and also accept CLI names.
            let name = key.replace('_', "-");
            match name.as_str() {
                "plot" => {
                    return Err(
                        "plot was removed; omit it (reports include plots automatically)".into(),
                    );
                }
                "mad-threshold" => {
                    return Err(
                        "mad_threshold was removed; use mad_top (an integer result count)".into(),
                    );
                }
                _ => {}
            }
            if !VALUE_OPTIONS.contains(&name.as_str())
                && !FLAG_OPTIONS.contains(&name.as_str())
                && !matches!(name.as_str(), "ai-url" | "ai-key")
            {
                return Err(format!("unsupported parameter: {key}"));
            }
            if !seen.insert(name.clone()) && !matches!(name.as_str(), "directory" | "json-file") {
                return Err(format!("parameter may only be supplied once: {key}"));
            }
            if FLAG_OPTIONS.contains(&name.as_str()) {
                match value.as_str() {
                    "true" | "1" => {
                        params.arguments.push(format!("--{name}"));
                        informational |= matches!(name.as_str(), "help" | "version");
                    }
                    "false" | "0" => {}
                    _ => return Err(format!("{key} must be true, false, 1, or 0")),
                }
                continue;
            }
            if value.is_empty() {
                return Err(format!("parameter requires a nonempty value: {key}"));
            }
            match name.as_str() {
                "ai" => {
                    params.ai = Some(value);
                    continue;
                }
                "ai-url" => {
                    params.ai_url = Some(value);
                    continue;
                }
                "ai-key" => {
                    params.ai_key = Some(value);
                    continue;
                }
                "directory" | "json-file" => project_count += 1,
                _ => {}
            }
            if matches!(
                name.as_str(),
                "directory" | "json-file" | "file" | "convert-md2html" | "url-context-file"
            ) {
                // Resolve input paths before changing the child process's cwd.
                let path = service_dir.join(&value);
                if name != "url-context-file" && first_source.is_none() {
                    first_source = Some(path.clone());
                }
                value = path.to_string_lossy().into_owned();
            }
            // Bind each value to its option, even if it starts with a dash.
            params.arguments.push(format!("--{name}={value}"));
        }
        if !informational {
            if seen.contains("mcp") {
                if project_count == 0 || seen.contains("file") || seen.contains("convert-md2html") {
                    return Err("mcp requires directory/json_file inputs and cannot be combined with file or convert_md2html".into());
                }
            } else if project_count > 1 {
                return Err("repeated directory/json_file inputs require mcp".into());
            } else if seen.contains("file") && project_count > 0 {
                return Err("file cannot be combined with directory or json_file".into());
            }
            if first_source.is_none() {
                return Err("supply directory, json_file, file, or convert_md2html".into());
            }
        }
        if let Some(source) = first_source {
            params.workdir = source.parent().unwrap_or(&source).to_path_buf();
        }
        Ok(params)
    }

    fn command(&self) -> Result<Command, String> {
        let mut cmd = Command::new(BINARY_PATH);
        cmd.args(&self.arguments).current_dir(&self.workdir);
        if let Some(ai) = &self.ai {
            let (vendor, model_lang) = ai
                .split_once(':')
                .ok_or("ai must have the format VENDOR:MODEL:LANGUAGE")?;
            let (model, language) = model_lang
                .rsplit_once(':')
                .ok_or("ai must have the format VENDOR:MODEL:LANGUAGE")?;
            if model.is_empty() || language.is_empty() {
                return Err("ai must have the format VENDOR:MODEL:LANGUAGE".into());
            }
            let (vendor, key_name, url_name) = match vendor {
                "openai" => ("openai", "OPENAI_API_KEY", Some("OPENAI_URL")),
                "google" | "gemini" => ("google", "GEMINI_API_KEY", None),
                "openrouter" => ("openrouter", "OPENROUTER_API_KEY", None),
                "local" => ("local", "LOCAL_API_KEY", Some("LOCAL_BASE_URL")),
                // Keep the wrapper's existing Ollama alias for OpenAI-compatible servers.
                "ollama" => {
                    if self.ai_url.is_none() {
                        return Err("ollama requires ai_url".into());
                    }
                    cmd.env("OPENAI_API_KEY", "whatever");
                    ("openai", "OPENAI_API_KEY", Some("OPENAI_URL"))
                }
                _ => return Err(format!("unsupported AI vendor: {vendor}")),
            };
            if let Some(key) = &self.ai_key {
                cmd.env(key_name, key);
            }
            if let Some(url) = &self.ai_url {
                let name =
                    url_name.ok_or("ai_url is supported only for openai, ollama, and local")?;
                cmd.env(name, url);
            }
            // Omitted credentials are inherited from the service or the CLI's .env.
            cmd.arg(format!("--ai={vendor}:{model_lang}"));
        } else if self.ai_key.is_some() || self.ai_url.is_some() {
            return Err("ai_key and ai_url require ai".into());
        }
        Ok(cmd)
    }
}

#[get("/jas-min/status")]
async fn status(data: web::Data<Arc<AppState>>) -> impl Responder {
    let is_running = data.is_running.lock().await;
    if *is_running { "running" } else { "idle" }
}

#[get("/jas-min/run")]
async fn run(data: web::Data<Arc<AppState>>, request: HttpRequest) -> impl Responder {
    let service_dir = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(error) => return HttpResponse::InternalServerError().body(error.to_string()),
    };
    // Validate before reserving the worker or truncating the previous output log.
    let params = match RunParams::from_query(request.query_string(), &service_dir) {
        Ok(params) => params,
        Err(error) => return HttpResponse::BadRequest().body(error),
    };
    let mut cmd = match params.command() {
        Ok(cmd) => cmd,
        Err(error) => return HttpResponse::BadRequest().body(error),
    };
    {
        let mut is_running = data.is_running.lock().await;
        if *is_running {
            return HttpResponse::Conflict().body("already-running");
        }
        *is_running = true;
    }

    let log_path = params.workdir.join("output.log");
    let log_file = match OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
    {
        Ok(file) => file,
        Err(e) => {
            *data.is_running.lock().await = false;
            return HttpResponse::InternalServerError()
                .body(format!("error opening log file: {e}"));
        }
    };
    let stdout_file = match log_file.try_clone() {
        Ok(file) => file,
        Err(e) => {
            *data.is_running.lock().await = false;
            return HttpResponse::InternalServerError()
                .body(format!("error cloning log file: {e}"));
        }
    };
    // Do not debug-print Command: it includes API keys from the child environment.
    println!("Running jas-min (cwd={})", params.workdir.display());
    cmd.stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(log_file));
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
            HttpResponse::InternalServerError().body(format!("error: {e}"))
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let state = Arc::new(AppState {
        is_running: Mutex::new(false),
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    fn params(query: &str) -> Result<RunParams, String> {
        RunParams::from_query(query, Path::new("/service"))
    }

    fn args(cmd: &Command) -> Vec<String> {
        cmd.as_std()
            .get_args()
            .map(|arg| arg.to_str().unwrap().to_owned())
            .collect()
    }

    fn env(cmd: &Command, name: &str) -> Option<String> {
        cmd.as_std()
            .get_envs()
            .find(|(key, _)| *key == name)
            .and_then(|(_, value)| value.map(|v| v.to_str().unwrap().to_owned()))
    }

    #[test]
    fn omitted_options_use_cli_defaults_including_automatic_lambda() {
        let parsed = params("directory=reports").unwrap();
        assert_eq!(
            args(&parsed.command().unwrap()),
            ["--directory=/service/reports"]
        );
        assert_eq!(parsed.workdir, Path::new("/service"));
    }

    #[test]
    fn values_are_decoded_and_bound_to_their_options() {
        let cmd = params("directory=reports&gradient_custom=WAIT%3Dlog+file+sync&id_sqls=--quiet&en_lambda=0.02&en-alpha=0.3").unwrap().command().unwrap();
        assert_eq!(
            args(&cmd),
            [
                "--directory=/service/reports",
                "--gradient-custom=WAIT=log file sync",
                "--id-sqls=--quiet",
                "--en-lambda=0.02",
                "--en-alpha=0.3",
            ]
        );
    }

    #[test]
    fn flags_accept_boolean_and_numeric_forms_without_passing_a_value() {
        for enabled in ["true", "1"] {
            let cmd = params(&format!(
                "directory=reports&quiet={enabled}&tools_mode={enabled}"
            ))
            .unwrap()
            .command()
            .unwrap();
            assert_eq!(
                args(&cmd),
                ["--directory=/service/reports", "--quiet", "--tools-mode"]
            );
        }
        for disabled in ["false", "0"] {
            let cmd = params(&format!(
                "directory=reports&quiet={disabled}&tools_mode={disabled}"
            ))
            .unwrap()
            .command()
            .unwrap();
            assert_eq!(args(&cmd), ["--directory=/service/reports"]);
        }
        assert!(params("directory=reports&quiet=yes").is_err());
        assert_eq!(
            args(&params("help=1").unwrap().command().unwrap()),
            ["--help"]
        );
        assert_eq!(
            args(&params("version=true").unwrap().command().unwrap()),
            ["--version"]
        );
    }

    #[test]
    fn mcp_preserves_repeated_and_mixed_inputs() {
        let parsed = params(
            "directory=before&directory=after&json_file=saved.json&mcp=127.0.0.1%3A4242%2Fmcp",
        )
        .unwrap();
        assert_eq!(
            args(&parsed.command().unwrap()),
            [
                "--directory=/service/before",
                "--directory=/service/after",
                "--json-file=/service/saved.json",
                "--mcp=127.0.0.1:4242/mcp",
            ]
        );
    }

    #[test]
    fn input_modes_resolve_paths_before_changing_cwd() {
        for mode in ["file", "json_file", "convert_md2html", "directory"] {
            let parsed = params(&format!(
                "{mode}=jobs/input&url_context_file=context.json&outfile=result.json"
            ))
            .unwrap();
            assert_eq!(parsed.workdir, Path::new("/service/jobs"));
            assert_eq!(
                args(&parsed.command().unwrap()),
                [
                    format!("--{}=/service/jobs/input", mode.replace('_', "-")),
                    "--url-context-file=/service/context.json".into(),
                    "--outfile=result.json".into(),
                ]
            );
        }
        let parsed = params("directory=/data/reports/").unwrap();
        assert_eq!(parsed.workdir, Path::new("/data"));
        assert_eq!(
            args(&parsed.command().unwrap()),
            ["--directory=/data/reports/"]
        );
    }

    #[test]
    fn rejects_removed_unknown_duplicate_and_incompatible_parameters() {
        for query in [
            "directory=reports&plot=1",
            "directory=reports&mad_threshold=7",
            "directory=reports&typo=1",
            "directory=reports&mad_top=3&mad-top=4",
            "directory=reports&directory=other",
            "directory=reports&json_file=other.json",
            "file=report.html&directory=reports",
            "file=report.html&mcp=127.0.0.1:4242/mcp",
            "convert_md2html=report.md&mcp=127.0.0.1:4242/mcp",
            "mcp=127.0.0.1:4242/mcp",
            "directory=",
            "quiet=true",
            "",
        ] {
            assert!(params(query).is_err(), "accepted {query}");
        }
    }

    #[test]
    fn ai_providers_map_keys_and_preserve_colons_in_model_names() {
        for (vendor, expected, key) in [
            ("openai", "openai", "OPENAI_API_KEY"),
            ("google", "google", "GEMINI_API_KEY"),
            ("gemini", "google", "GEMINI_API_KEY"),
            ("openrouter", "openrouter", "OPENROUTER_API_KEY"),
            ("local", "local", "LOCAL_API_KEY"),
        ] {
            let cmd = params(&format!(
                "directory=reports&ai={vendor}:model:32b:EN&ai_key=test-key"
            ))
            .unwrap()
            .command()
            .unwrap();
            assert_eq!(
                args(&cmd).last().unwrap(),
                &format!("--ai={expected}:model:32b:EN")
            );
            assert_eq!(env(&cmd, key).as_deref(), Some("test-key"));
            let inherited = params(&format!("directory=reports&ai={vendor}:model:EN"))
                .unwrap()
                .command()
                .unwrap();
            assert_eq!(env(&inherited, key), None);
        }
        for (vendor, url_var) in [
            ("openai", "OPENAI_URL"),
            ("ollama", "OPENAI_URL"),
            ("local", "LOCAL_BASE_URL"),
        ] {
            let cmd = params(&format!(
                "directory=reports&ai={vendor}:model:EN&ai_url=http%3A%2F%2Flocalhost%3A1234%2Fv1"
            ))
            .unwrap()
            .command()
            .unwrap();
            assert_eq!(
                env(&cmd, url_var).as_deref(),
                Some("http://localhost:1234/v1")
            );
            if vendor == "ollama" {
                assert_eq!(args(&cmd).last().unwrap(), "--ai=openai:model:EN");
                assert_eq!(env(&cmd, "OPENAI_API_KEY").as_deref(), Some("whatever"));
            }
        }
    }

    #[actix_web::test]
    async fn invalid_requests_do_not_reserve_the_worker() {
        let state = Arc::new(AppState {
            is_running: Mutex::new(false),
        });
        let app = actix_web::test::init_service(
            App::new()
                .app_data(web::Data::new(state.clone()))
                .service(run)
                .service(status),
        )
        .await;
        for query in [
            "directory=reports&plot=1",
            "directory=reports&ai=ollama:model:EN",
            "directory=reports&ai=openai",
            "directory=reports&ai=unknown:model:EN",
            "directory=reports&ai_key=key",
            "directory=reports&ai=google:model:EN&ai_url=http://localhost",
        ] {
            let request = actix_web::test::TestRequest::get()
                .uri(&format!("/jas-min/run?{query}"))
                .to_request();
            let response = actix_web::test::call_service(&app, request).await;
            assert_eq!(
                response.status(),
                actix_web::http::StatusCode::BAD_REQUEST,
                "{query}"
            );
            assert!(!*state.is_running.lock().await);
        }
        *state.is_running.lock().await = true;
        let request = actix_web::test::TestRequest::get()
            .uri("/jas-min/run?directory=reports")
            .to_request();
        assert_eq!(
            actix_web::test::call_service(&app, request).await.status(),
            actix_web::http::StatusCode::CONFLICT
        );
        let request = actix_web::test::TestRequest::get()
            .uri("/jas-min/status")
            .to_request();
        assert_eq!(
            actix_web::test::call_and_read_body(&app, request).await,
            "running"
        );
    }
}
