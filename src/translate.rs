//! AI lyrics translation via an OpenAI-compatible chat-completions endpoint.
//!
//! One code path covers both hosted providers (`OpenAI`, Gemini, Mistral, ...)
//! and local servers (Ollama, LM Studio): only the base URL and model differ,
//! and the API key is optional so keyless local servers work without one.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const DEFAULT_BASE_URL: &str = "http://localhost:11434/v1";
pub const DEFAULT_MODEL: &str = "llama3.1";

pub const DEFAULT_PROMPT: &str = "Translate the song lyrics below into the target language for a language learner. Keep every [mm:ss.xx] timestamp exactly as-is at the start of its line if present, keep one lyric per line in the original order, and translate only the lyric text. After any line whose vocabulary, grammar, or cultural reference a learner may find non-obvious, add a `# note` line with a brief explanation in the target language - only where genuinely helpful, not on every line. Output only the translated lyrics with timestamps and `# note` lines, no preamble or commentary.";

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: [ChatMessage; 2],
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    #[serde(default)]
    message: ChatResponseMessage,
}

#[derive(Debug, Default, Deserialize)]
struct ChatResponseMessage {
    #[serde(default)]
    content: String,
}

#[derive(Debug, Deserialize)]
struct ModelListResponse {
    #[serde(default)]
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    #[serde(default)]
    id: String,
}

pub fn completions_url(base_url: &str) -> String {
    format!("{}/chat/completions", base_url.trim_end_matches('/'))
}

/// Fetch the model ids advertised by an OpenAI-compatible server
/// (`GET {base_url}/models`, served by hosted providers and Ollama alike).
/// Empty ids are dropped; transport and HTTP errors carry the full context.
pub fn list_models(base_url: &str, api_key: &str) -> Result<Vec<String>> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let mut req = agent().get(&url).header(
        "User-Agent",
        "goosemusic/0.1 (https://github.com/gooseob/music_plr)",
    );
    if !api_key.trim().is_empty() {
        req = req.header("Authorization", &format!("Bearer {}", api_key.trim()));
    }
    let mut resp = req
        .call()
        .with_context(|| format!("request to {url} failed"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(status_error(status.as_u16(), resp.body_mut()));
    }
    let text = resp
        .body_mut()
        .read_to_string()
        .with_context(|| "model list response could not be read")?;
    parse_model_list(&text)
}

fn parse_model_list(text: &str) -> Result<Vec<String>> {
    let parsed: ModelListResponse =
        serde_json::from_str(text).with_context(|| "model list was not valid JSON")?;
    Ok(parsed
        .data
        .into_iter()
        .map(|m| m.id)
        .filter(|id| !id.trim().is_empty())
        .collect())
}

fn request_body(model: &str, prompt: &str, language: &str, lyrics: &str) -> ChatRequest {
    ChatRequest {
        model: model.to_string(),
        messages: [
            ChatMessage {
                role: "system",
                content: prompt.to_string(),
            },
            ChatMessage {
                role: "user",
                content: format!("Target language: {language}\n\nLyrics:\n{lyrics}"),
            },
        ],
    }
}

pub fn translate(
    base_url: &str,
    api_key: &str,
    model: &str,
    prompt: &str,
    language: &str,
    lyrics: &str,
) -> Result<String> {
    let url = completions_url(base_url);
    let body = request_body(model, prompt, language, lyrics);
    let mut req = agent()
        .post(&url)
        .header("Content-Type", "application/json")
        .header(
            "User-Agent",
            "goosemusic/0.1 (https://github.com/gooseob/music_plr)",
        );
    if !api_key.trim().is_empty() {
        req = req.header("Authorization", &format!("Bearer {}", api_key.trim()));
    }
    let mut resp = req
        .send_json(&body)
        .with_context(|| format!("request to {url} failed"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(status_error(status.as_u16(), resp.body_mut()));
    }
    let parsed: ChatResponse = resp
        .body_mut()
        .read_json()
        .with_context(|| "translation response was not valid JSON")?;
    let content = parsed
        .choices
        .first()
        .map(|c| c.message.content.trim().to_string())
        .filter(|s| !s.is_empty())
        .context("translation response contained no text")?;
    Ok(content)
}

fn agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::config::Config::builder()
            .timeout_connect(Some(std::time::Duration::from_secs(15)))
            .timeout_global(Some(std::time::Duration::from_mins(3)))
            // Non-2xx statuses arrive as `Ok` so the server's error body
            // (e.g. Ollama's "model not found, try pulling it first") can be
            // surfaced instead of a bare status code.
            .http_status_as_error(false)
            .build()
            .new_agent()
    })
}

/// Build the error for a non-2xx translation response, quoting the server's
/// error body (truncated) so the toast says *why* it failed.
fn status_error(status: u16, body: &mut ureq::Body) -> anyhow::Error {
    let snippet = error_body_snippet(body);
    if snippet.is_empty() {
        anyhow::anyhow!("server returned HTTP {status} with no error details")
    } else {
        anyhow::anyhow!("server returned HTTP {status}: {snippet}")
    }
}

fn error_body_snippet(body: &mut ureq::Body) -> String {
    truncate_snippet(&body.read_to_string().unwrap_or_default())
}

fn truncate_snippet(text: &str) -> String {
    const LIMIT: usize = 300;
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.len() <= LIMIT {
        return collapsed;
    }
    let mut end = LIMIT;
    while !collapsed.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &collapsed[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completions_url_trims_trailing_slash() {
        assert_eq!(
            completions_url("http://localhost:11434/v1"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            completions_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn request_body_carries_model_and_language() {
        let body = request_body("llama3.1", "sys", "Spanish", "[00:01.00]hi");
        assert_eq!(body.model, "llama3.1");
        assert_eq!(body.messages[0].role, "system");
        assert_eq!(body.messages[0].content, "sys");
        assert!(body.messages[1].content.contains("Spanish"));
        assert!(body.messages[1].content.contains("[00:01.00]hi"));
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["model"], "llama3.1");
        assert_eq!(json["messages"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn parses_chat_response_content() {
        let raw =
            r#"{"choices": [{"message": {"role": "assistant", "content": "[00:01.00]hola"}}]}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(
            parsed.choices.first().unwrap().message.content,
            "[00:01.00]hola"
        );
    }

    #[test]
    fn empty_choices_have_no_content() {
        let parsed: ChatResponse = serde_json::from_str("{}").unwrap();
        assert!(parsed.choices.is_empty());
    }

    #[test]
    fn snippet_keeps_short_bodies() {
        assert_eq!(
            truncate_snippet("{\"error\":\"bad key\"}"),
            "{\"error\":\"bad key\"}"
        );
        assert_eq!(truncate_snippet(""), "");
    }

    #[test]
    fn snippet_collapses_whitespace() {
        assert_eq!(
            truncate_snippet("{\n  \"error\":  \"nope\"\n}"),
            "{ \"error\": \"nope\" }"
        );
    }

    #[test]
    fn snippet_truncates_without_splitting_chars() {
        let long = format!("x{}", "а".repeat(150));
        assert!(long.len() > 300);
        let snippet = truncate_snippet(&long);
        assert!(snippet.ends_with('…'));
        assert!(snippet.len() <= 300 + '…'.len_utf8());
    }

    #[test]
    fn parses_openai_model_list() {
        let raw = r#"{"object":"list","data":[{"id":"llama3.1","object":"model"},{"id":"qwen3:8b","object":"model"},{"id":"","object":"model"}]}"#;
        assert_eq!(parse_model_list(raw).unwrap(), ["llama3.1", "qwen3:8b"]);
    }

    #[test]
    fn model_list_rejects_invalid_json() {
        assert!(parse_model_list("not json").is_err());
        assert!(parse_model_list("{}").unwrap().is_empty());
    }

    #[cfg(test)]
    fn serve_once(status: &str, body: &str) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 8192];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(response.as_bytes());
        });
        format!("http://{addr}/v1")
    }

    #[test]
    fn list_models_reads_ids_from_server() {
        let base = serve_once("200 OK", r#"{"data":[{"id":"b"},{"id":"a"}]}"#);
        assert_eq!(list_models(&base, "").unwrap(), ["b", "a"]);
    }

    #[test]
    fn list_models_reports_server_errors_with_body() {
        let base = serve_once("401 Unauthorized", r#"{"error":"bad key"}"#);
        let err = list_models(&base, "").unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("401"), "{msg}");
        assert!(msg.contains("bad key"), "{msg}");
    }

    #[test]
    fn alternate_display_keeps_error_chain() {
        let err = anyhow::anyhow!("connection refused").context("request to http://x failed");
        let msg = format!("{err:#}");
        assert!(msg.contains("request to http://x failed"));
        assert!(msg.contains("connection refused"));
    }
}
