# JAS-MIN REST package

`rest/` is an independent Rust project that runs `/opt/jas-min/jas-min`.
The option mapping follows the checked-in JAS-MIN 0.9.3 `src/main.rs`.

## Calling the service

`GET /jas-min/run` starts a background process and returns `started`.
`GET /jas-min/status` returns `running` or `idle`; a second run while busy
returns HTTP 409 (`already-running`). Query/AI configuration errors return
HTTP 400. Numeric values, filesystem existence, and MCP endpoint validity
are checked by the child CLI; `started` confirms process creation, not
successful analysis. Its stdout/stderr are written to `output.log`.

Query parameters use underscores (`mad_top`) or the CLI's hyphens (`mad-top`),
without the leading `--`. Values should be URL encoded. Boolean flags accept
`true`/`1` and `false`/`0`; false flags are omitted. All other omitted options
retain the installed CLI's defaults, including automatic Elastic Net lambda
selection. Unknown parameters and duplicate scalar parameters are rejected.

```bash
curl --get http://localhost:8080/jas-min/run \
  --data-urlencode 'directory=/data/awr_reports' \
  --data-urlencode 'mad_top=10' \
  --data-urlencode 'top_cluster_anomalies=5' \
  --data-urlencode 'quiet=true'

curl --get http://localhost:8080/jas-min/run \
  --data-urlencode 'json_file=/data/awr_reports.json' \
  --data-urlencode 'ai=local:qwen/qwen3.6-35b-a3b:EN' \
  --data-urlencode 'ai_url=http://localhost:1234/v1' \
  --data-urlencode 'tokens_budget=128000' \
  --data-urlencode 'max_tool_iterations=8'

curl --get http://localhost:8080/jas-min/run \
  --data-urlencode 'directory=/data/before' \
  --data-urlencode 'directory=/data/after' \
  --data-urlencode 'json_file=/data/saved.json' \
  --data-urlencode 'mcp=127.0.0.1:4242/mcp'
```

`directory` and `json_file` may be repeated or mixed only with `mcp`.
MCP cannot be combined with `file` or `convert_md2html`. MCP keeps the child
alive, so REST status stays `running` until that process exits. The MCP
listener is loopback-only on the REST service host; this wrapper does not
proxy MCP or provide a stop endpoint.

Input paths (`directory`, `json_file`, `file`, `convert_md2html`, and
`url_context_file`) are resolved relative to the REST service's working
directory before starting the child. The parent of the first report input
in query order is the job's working directory and contains `output.log`.
Relative `outfile` paths resolve there. For `help=true` or `version=true`
without an input, the working directory is the service's directory; the
CLI's informational output is also written to `output.log`.

## Supported options and changes to REST coverage

“Added” means newly exposed by this REST update; some options already existed
in older versions of the main CLI. Defaults shown are from the checked-in CLI,
not defaults imposed by the wrapper.

| CLI option | REST query parameter | CLI default / form | REST change |
| --- | --- | --- | --- |
| `--file` | `file` | Report path | Added |
| `-d, --directory` | `directory` | Directory path, repeatable with MCP | Repeatable |
| `-o, --outfile` | `outfile` | CLI-selected output filename | Added |
| `-t, --time-cpu-ratio` | `time_cpu_ratio` | `0.666` | Retained |
| `-f, --filter-db-time` | `filter_db_time` | `0` | Added |
| `-i, --id-sqls` | `id_sqls` | Comma-separated SQL IDs | Added |
| `-j, --json-file` | `json_file` | JSON path, repeatable with MCP | Added |
| `-s, --snap-range` | `snap_range` | `0-666666666` | Added |
| `-q, --quiet` | `quiet` | `false` | Added |
| `-a, --ai` | `ai` | `VENDOR:MODEL:LANGUAGE` | Updated providers; see below |
| `-m, --mad-top` | `mad_top` | `10` (integer result count) | Added |
| `-W, --mad-window-size` | `mad_window_size` | `100` (percent of probes) | Retained |
| `-T, --top-cluster-anomalies` | `top_cluster_anomalies` | `0` (no trimming) | Added |
| `-P, --parallel` | `parallel` | `4` | Retained |
| `-S, --security-level` | `security_level` | `0` | Retained |
| `-u, --url-context-file` | `url_context_file` | URL context file path | Added |
| `-B, --tokens-budget` | `tokens_budget` | `256000` | Added |
| `-R, --ridge-lambda` | `ridge_lambda` | `0.05` | Added |
| `-E, --en-lambda` | `en_lambda` | Omitted: automatic selection | Added |
| `-A, --en-alpha` | `en_alpha` | `0.2` | Added |
| `-I, --en-max-iter` | `en_max_iter` | `5000` | Added |
| `--en-tol` | `en_tol` | `0.000001` | Added |
| `--top-gradient` | `top_gradient` | `10` | Added |
| `-c, --convert-md2html` | `convert_md2html` | Markdown path | Added |
| `-G, --gradient-custom` | `gradient_custom` | `SQL=<id>` or `WAIT=<wait event>` | Added |
| `--tools-mode` | `tools_mode` | `false` (local AI always uses tools) | Added |
| `--max-tool-iterations` | `max_tool_iterations` | `10` | Added |
| `--mcp` | `mcp` | Loopback `ADDRESS/PATH` | Added |
| `-h, --help` | `help` | Boolean flag | Added |
| `-V, --version` | `version` | Boolean flag | Added |

For custom wait-event gradients, use `WAIT=log file sync`: the current analysis
parser expects `WAIT`, although the CLI help comment says `EVENT`.

### Removed and changed CLI options

The repository history confirms these migrations relevant to older wrappers:

- `-p, --plot` was removed (`ea177aa`); plots no longer require this switch.
  REST now rejects `plot` with a migration message instead of launching an
  unsupported CLI command.
- `-m, --mad-threshold` was renamed to `-m, --mad-top` (`4882b1e`).
  Older versions used a floating-point threshold; the current option retains
  the top N MAD anomalies and takes an integer (default `10`). The last REST
  revision had already dropped `mad_threshold`; this update exposes `mad_top`.
- `--token-count-factor` and `-b, --backend-assistant` were removed (`19d41fe`);
  `-D, --deep-check` was removed (`fb73f75`). They are not forwarded.
  Current AI configuration uses `tokens_budget`, `tools_mode`, and
  `max_tool_iterations`; these are not one-to-one replacements.
- `--directory` and `--json-file` now support multiple projects with `--mcp`
  (`76e5133`). Without MCP, only one directory/JSON input is allowed.
- `--ridge-lambda` changed from `50` to `0.05`; `--en-alpha` from `0.333`
  to `0.2`; `--en-lambda` changed from a fixed default of `30` to automatic
  selection when omitted (`a345532`). Fixed lambdas now apply to a standardized
  target and are not numerically interchangeable with older values. Automatic
  selection requires `en_alpha > 0`; supply `en_lambda` for pure L2 (`en_alpha=0`).
- `--top-cluster-anomalies` now limits the largest snapshot-date clusters across
  categories, with `0` disabling trimming. It uses `-T`; `--en-tol` has no short flag.
- `--tokens-budget` now defaults to `256000` (older CLI: `80000`); local mode
  treats it as its context ceiling. `--ai local:...` always uses the tools workflow.

### AI environment parameters

`ai_key` and `ai_url` are REST helpers, not JAS-MIN command-line options.
They set environment variables on the child process; the wrapper does not
write `.env` files. Omitted values allow the service environment or the CLI's
`.env` loading to supply configuration.

| `ai` vendor | `ai_key` sets | `ai_url` sets |
| --- | --- | --- |
| `openai` | `OPENAI_API_KEY` | `OPENAI_URL` |
| `google` / legacy `gemini` alias | `GEMINI_API_KEY` | Unsupported |
| `openrouter` | `OPENROUTER_API_KEY` | Unsupported |
| `local` | `LOCAL_API_KEY` | `LOCAL_BASE_URL` |
| Legacy `ollama` alias | `OPENAI_API_KEY` (default `whatever`) | `OPENAI_URL` (required) |

The wrapper preserves `gemini` → `google` and `ollama` → `openai` aliases.
It now handles OpenRouter and local model credentials explicitly and accepts
an OpenAI URL override. Models containing colons, such as `ollama:qwen3:32b:EN`,
are preserved. Invalid AI requests no longer leave the worker marked running,
and command logging no longer includes API keys.

## Verification

```bash
cargo test --manifest-path pkg/rest/Cargo.toml
cargo fmt --manifest-path pkg/rest/Cargo.toml -- --check
python3 -m unittest discover -s tests -p test_rest_cli_options.py
```

The Rust tests exercise query decoding, argument construction, flags, repeated
inputs, paths, AI environments, and HTTP error/busy handling. The Python test
compares the REST allowlist with the main CLI's argument declarations to detect
future option drift without coupling the two Cargo projects. The package
builder currently clones upstream JAS-MIN during its build, so its installed
binary can differ from the checked-in version used for this comparison.
