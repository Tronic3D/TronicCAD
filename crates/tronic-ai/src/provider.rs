//! The exchangeable provider interface and its HTTP implementations.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use thiserror::Error;
use url::{Host, Url};

const ANTHROPIC_VERSION: &str = "2026-10-21";
/// Models that accept server-side refusal fallbacks in the `"default"` form.
const FALLBACK_MODELS: [&str; 4] = [
    "claude-fable-5-1",
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-sonnet-5-5",
];
const ANTHROPIC_FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const ANTHROPIC_DEFAULT_MAX_TOKENS: u32 = 64_000;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// A local server may have to load its model before it answers.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(300);
const BODY_TIMEOUT: Duration = Duration::from_secs(1800);
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_ERROR_CHARS: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AiError {
    #[error("the provider address is invalid")]
    InvalidUrl,
    #[error("external providers need https")]
    InsecureUrl,
    #[error("no model is set")]
    NoModel,
    #[error("sending needs the user's consent")]
    ConsentRequired,
    #[error("the provider cannot be reached: {0}")]
    Unreachable(String),
    #[error("the provider rejected the key")]
    Unauthorized,
    #[error("the provider does not know this model or path")]
    NotFound,
    #[error("the provider limits requests at the moment")]
    RateLimited,
    #[error("the provider reports status {status}: {message}")]
    Http { status: u16, message: String },
    #[error("the provider's answer cannot be read")]
    BadResponse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// `/v1/chat/completions`: Ollama, llama.cpp, LM Studio and many cloud services.
    OpenAiCompatible,
    /// `/v1/messages`.
    Anthropic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: String,
    /// Longest answer. None leaves it to the provider where that is possible.
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

/// Where project data ends up. Follows from the address, never from a setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Locality {
    Local,
    External,
}

/// Only this computer counts as local. A server elsewhere in the network does not.
/// External addresses must use https: keys and project data would otherwise
/// travel unencrypted.
pub fn classify(base_url: &str) -> Result<Locality, AiError> {
    let url = Url::parse(base_url.trim()).map_err(|_| AiError::InvalidUrl)?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(AiError::InvalidUrl);
    }
    let local = match url.host() {
        Some(Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        None => return Err(AiError::InvalidUrl),
    };
    if local {
        Ok(Locality::Local)
    } else if url.scheme() == "https" {
        Ok(Locality::External)
    } else {
        Err(AiError::InsecureUrl)
    }
}

/// `https://host:port`, the identity that consent and keys belong to.
pub fn origin(base_url: &str) -> Result<String, AiError> {
    let url = Url::parse(base_url.trim()).map_err(|_| AiError::InvalidUrl)?;
    Ok(url.origin().ascii_serialization())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

/// Everything that leaves the application, exactly as the user can inspect it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChatRequest {
    pub system: String,
    pub messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Finished,
    /// The answer hit the length limit and is incomplete.
    Truncated,
    /// The model or its provider declined to answer.
    Refused,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChatOutcome {
    pub text: String,
    pub stop: StopReason,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

/// One way of asking a model. The rest of the application only knows this.
pub trait AiProvider: Send + Sync {
    /// Sends a request. `on_text` receives the answer piece by piece.
    fn chat(
        &self,
        request: &ChatRequest,
        cancel: &AtomicBool,
        on_text: &mut dyn FnMut(&str),
    ) -> Result<ChatOutcome, AiError>;
}

pub struct HttpProvider {
    config: ProviderConfig,
    key: Option<String>,
    locality: Locality,
}

impl HttpProvider {
    pub fn new(config: ProviderConfig, key: Option<String>) -> Result<Self, AiError> {
        let locality = classify(&config.base_url)?;
        if config.model.trim().is_empty() {
            return Err(AiError::NoModel);
        }
        Ok(Self {
            config,
            key,
            locality,
        })
    }

    fn agent(&self, response: Duration, body: Duration) -> ureq::Agent {
        // No redirects: an answer from a local or trusted address must not be
        // able to send the project data and the key somewhere else. Local traffic
        // never goes through a proxy.
        let proxy = match self.locality {
            Locality::Local => None,
            Locality::External => ureq::Proxy::try_from_env(),
        };
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .proxy(proxy)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_recv_response(Some(response))
            .timeout_recv_body(Some(body))
            .build()
            .into()
    }

    fn url(&self, path: &str) -> String {
        let base = self.config.base_url.trim().trim_end_matches('/');
        let base = base.strip_suffix("/v1").unwrap_or(base);
        format!("{base}{path}")
    }

    fn headers(&self) -> Vec<(&'static str, String)> {
        let mut headers = Vec::new();
        match self.config.kind {
            ProviderKind::OpenAiCompatible => {
                if let Some(key) = &self.key {
                    headers.push(("authorization", format!("Bearer {key}")));
                }
            }
            ProviderKind::Anthropic => {
                headers.push(("anthropic-version", ANTHROPIC_VERSION.to_owned()));
                if let Some(key) = &self.key {
                    headers.push(("x-api-key", key.clone()));
                }
            }
        }
        headers
    }

    fn body(&self, request: &ChatRequest) -> Value {
        let messages = request.messages.iter().map(|message| {
            json!({
                "role": match message.role { Role::User => "user", Role::Assistant => "assistant" },
                "content": message.content,
            })
        });
        match self.config.kind {
            ProviderKind::OpenAiCompatible => {
                let mut all = vec![json!({"role": "system", "content": request.system})];
                all.extend(messages);
                let mut body = json!({
                    "model": self.config.model,
                    "stream": true,
                    "stream_options": {"include_usage": true},
                    "messages": all,
                });
                if let Some(limit) = self.config.max_tokens {
                    body["max_tokens"] = json!(limit);
                }
                body
            }
            ProviderKind::Anthropic => {
                let mut body = json!({
                    "model": self.config.model,
                    "max_tokens": self.config.max_tokens.unwrap_or(ANTHROPIC_DEFAULT_MAX_TOKENS),
                    "stream": true,
                    "system": request.system,
                    "messages": messages.collect::<Vec<_>>(),
                });
                if self.uses_fallbacks() {
                    body["fallbacks"] = json!("default");
                }
                body
            }
        }
    }

    fn uses_fallbacks(&self) -> bool {
        self.config.kind == ProviderKind::Anthropic
            && FALLBACK_MODELS.contains(&self.config.model.as_str())
            && origin(&self.config.base_url).as_deref() == Ok("https://api.anthropic.com")
    }

    fn get_json(&self, path: &str) -> Result<Value, AiError> {
        let agent = self.agent(PROBE_TIMEOUT, PROBE_TIMEOUT);
        let mut call = agent.get(self.url(path));
        for (name, value) in self.headers() {
            call = call.header(name, value);
        }
        let mut response = call
            .call()
            .map_err(|error| AiError::Unreachable(error.to_string()))?;
        let status = response.status().as_u16();
        let mut text = String::new();
        response
            .body_mut()
            .as_reader()
            .take(4 * 1024 * 1024)
            .read_to_string(&mut text)
            .map_err(|_| AiError::BadResponse)?;
        if !(200..300).contains(&status) {
            return Err(status_error(status, &text));
        }
        serde_json::from_str(&text).map_err(|_| AiError::BadResponse)
    }

    /// The models the provider offers. Sends no project data.
    pub fn models(&self) -> Result<Vec<String>, AiError> {
        let body = self.get_json("/v1/models")?;
        let list = body["data"].as_array().ok_or(AiError::BadResponse)?;
        Ok(list
            .iter()
            .filter_map(|model| model["id"].as_str().map(str::to_owned))
            .collect())
    }

    /// What a local server reports about loaded models. Ollama offers this,
    /// other servers do not, which is not an error.
    pub fn loaded_models(&self) -> Vec<LoadedModel> {
        if self.locality != Locality::Local {
            return Vec::new();
        }
        let Ok(body) = self.get_json("/api/ps") else {
            return Vec::new();
        };
        body["models"]
            .as_array()
            .map(|models| {
                models
                    .iter()
                    .filter_map(|model| {
                        Some(LoadedModel {
                            name: model["name"].as_str()?.to_owned(),
                            bytes: model["size"].as_u64()?,
                            gpu_bytes: model["size_vram"].as_u64().unwrap_or(0),
                            context_length: model["context_length"].as_u64(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoadedModel {
    pub name: String,
    pub bytes: u64,
    pub gpu_bytes: u64,
    /// How many tokens the model works with, as far as the server says.
    pub context_length: Option<u64>,
}

/// Local servers can cut a request that exceeds the model's context without any
/// message, and the model then answers from part of the data. Measured with
/// Ollama: at a context of 4096, every long request counts as about 2050 tokens.
/// So this looks for two signs: far fewer tokens counted than the text can hold,
/// or a count at that cut-off. A false alarm is possible, a silent loss is worse.
pub fn cut_suspected(sent_chars: usize, input_tokens: Option<u64>, context: Option<u64>) -> bool {
    let Some(tokens) = input_tokens else {
        return false;
    };
    // Even very regular text rarely packs more than about eight characters into a token.
    let too_few = sent_chars as u64 / 8 > tokens;
    let at_limit = context.is_some_and(|limit| tokens + 8 >= limit / 2);
    too_few || at_limit
}

/// Never includes request headers, so a key cannot reach a message this way.
fn status_error(status: u16, body: &str) -> AiError {
    match status {
        401 | 403 => AiError::Unauthorized,
        404 => AiError::NotFound,
        429 => AiError::RateLimited,
        _ => {
            let parsed: Option<Value> = serde_json::from_str(body).ok();
            let message = parsed
                .as_ref()
                .and_then(|value| {
                    value["error"]["message"]
                        .as_str()
                        .or_else(|| value["error"].as_str())
                        .or_else(|| value["message"].as_str())
                })
                .unwrap_or("");
            AiError::Http {
                status,
                message: message.chars().take(MAX_ERROR_CHARS).collect(),
            }
        }
    }
}

/// Collects what the event stream of one provider says.
#[derive(Default)]
struct StreamState {
    text: String,
    stop: Option<StopReason>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    done: bool,
}

fn read_openai_event(
    event: &Value,
    state: &mut StreamState,
    on_text: &mut dyn FnMut(&str),
) -> Result<(), AiError> {
    if !event["error"].is_null() {
        return Err(status_error(500, &event.to_string()));
    }
    if let Some(choice) = event["choices"].get(0) {
        if let Some(text) = choice["delta"]["content"].as_str() {
            state.text.push_str(text);
            on_text(text);
        }
        match choice["finish_reason"].as_str() {
            Some("length") => state.stop = Some(StopReason::Truncated),
            Some("content_filter") => state.stop = Some(StopReason::Refused),
            Some(_) => state.stop = Some(StopReason::Finished),
            None => {}
        }
    }
    if let Some(usage) = event.get("usage") {
        state.input_tokens = usage["prompt_tokens"].as_u64().or(state.input_tokens);
        state.output_tokens = usage["completion_tokens"].as_u64().or(state.output_tokens);
    }
    Ok(())
}

fn read_anthropic_event(
    event: &Value,
    state: &mut StreamState,
    on_text: &mut dyn FnMut(&str),
) -> Result<(), AiError> {
    match event["type"].as_str() {
        Some("content_block_delta") if event["delta"]["type"] == "text_delta" => {
            if let Some(text) = event["delta"]["text"].as_str() {
                state.text.push_str(text);
                on_text(text);
            }
        }
        Some("message_start") => {
            state.input_tokens = event["message"]["usage"]["input_tokens"].as_u64();
        }
        Some("message_delta") => {
            state.stop = match event["delta"]["stop_reason"].as_str() {
                Some("max_tokens") => Some(StopReason::Truncated),
                Some("refusal") => Some(StopReason::Refused),
                Some(_) => Some(StopReason::Finished),
                None => state.stop,
            };
            state.output_tokens = event["usage"]["output_tokens"]
                .as_u64()
                .or(state.output_tokens);
        }
        Some("message_stop") => state.done = true,
        Some("error") => return Err(status_error(500, &event.to_string())),
        _ => {}
    }
    Ok(())
}

impl AiProvider for HttpProvider {
    fn chat(
        &self,
        request: &ChatRequest,
        cancel: &AtomicBool,
        on_text: &mut dyn FnMut(&str),
    ) -> Result<ChatOutcome, AiError> {
        let path = match self.config.kind {
            ProviderKind::OpenAiCompatible => "/v1/chat/completions",
            ProviderKind::Anthropic => "/v1/messages",
        };
        let agent = self.agent(RESPONSE_TIMEOUT, BODY_TIMEOUT);
        let mut call = agent
            .post(self.url(path))
            .header("content-type", "application/json");
        for (name, value) in self.headers() {
            call = call.header(name, value);
        }
        if self.uses_fallbacks() {
            call = call.header("anthropic-beta", ANTHROPIC_FALLBACK_BETA);
        }
        let mut response = call
            .send(self.body(request).to_string())
            .map_err(|error| AiError::Unreachable(error.to_string()))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let mut text = String::new();
            let _ = response
                .body_mut()
                .as_reader()
                .take(64 * 1024)
                .read_to_string(&mut text);
            return Err(status_error(status, &text));
        }

        let mut state = StreamState::default();
        let mut lines = BufReader::new(response.body_mut().as_reader());
        let mut line = String::new();
        let mut cancelled = false;
        loop {
            if cancel.load(Ordering::Relaxed) {
                cancelled = true;
                break;
            }
            line.clear();
            let read = lines
                .read_line(&mut line)
                .map_err(|error| AiError::Unreachable(error.to_string()))?;
            if read == 0 {
                break;
            }
            let Some(data) = line.trim_end().strip_prefix("data:") else {
                continue;
            };
            let data = data.trim_start();
            if data == "[DONE]" {
                break;
            }
            let event: Value = serde_json::from_str(data).map_err(|_| AiError::BadResponse)?;
            match self.config.kind {
                ProviderKind::OpenAiCompatible => {
                    read_openai_event(&event, &mut state, on_text)?;
                }
                ProviderKind::Anthropic => read_anthropic_event(&event, &mut state, on_text)?,
            }
            if state.done {
                break;
            }
        }
        // A stream that ends without saying why ended early.
        let stop = if cancelled {
            StopReason::Cancelled
        } else {
            state.stop.ok_or(AiError::BadResponse)?
        };
        Ok(ChatOutcome {
            text: state.text,
            stop,
            input_tokens: state.input_tokens,
            output_tokens: state.output_tokens,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn only_this_computer_counts_as_local() {
        for url in [
            "http://localhost:11434",
            "http://LOCALHOST:8080/v1",
            "http://127.0.0.1:8080",
            "http://127.1.2.3",
            "http://[::1]:8080",
            "https://localhost",
        ] {
            assert_eq!(classify(url), Ok(Locality::Local), "{url}");
        }
        assert_eq!(classify("https://api.example.com"), Ok(Locality::External));
    }

    #[test]
    fn lookalikes_are_not_local() {
        for url in [
            "https://localhost.example.com",
            "https://127.0.0.1.example.com",
            "https://example.com/localhost",
            "https://192.168.1.5",
            "https://10.0.0.1:8080",
            "https://[fe80::1]",
        ] {
            assert_eq!(classify(url), Ok(Locality::External), "{url}");
        }
    }

    #[test]
    fn external_addresses_need_https() {
        for url in ["http://api.example.com", "http://192.168.1.5:11434"] {
            assert_eq!(classify(url), Err(AiError::InsecureUrl), "{url}");
        }
    }

    #[test]
    fn nonsense_addresses_are_refused() {
        for url in [
            "",
            "localhost:1234",
            "ftp://localhost",
            "file:///etc/passwd",
            "https://user:secret@localhost",
        ] {
            assert_eq!(classify(url), Err(AiError::InvalidUrl), "{url}");
        }
    }

    #[test]
    fn the_v1_suffix_may_be_given_or_not() {
        let provider = |url: &str| {
            HttpProvider::new(
                ProviderConfig {
                    kind: ProviderKind::OpenAiCompatible,
                    base_url: url.to_owned(),
                    model: "m".to_owned(),
                    max_tokens: None,
                },
                None,
            )
            .unwrap()
        };
        for url in [
            "http://localhost:1",
            "http://localhost:1/",
            "http://localhost:1/v1/",
        ] {
            assert_eq!(
                provider(url).url("/v1/models"),
                "http://localhost:1/v1/models"
            );
        }
    }

    #[test]
    fn a_model_is_required() {
        let config = ProviderConfig {
            kind: ProviderKind::Anthropic,
            base_url: "https://api.anthropic.com".to_owned(),
            model: " ".to_owned(),
            max_tokens: None,
        };
        assert!(matches!(
            HttpProvider::new(config, None),
            Err(AiError::NoModel)
        ));
    }

    #[test]
    fn fallbacks_are_only_requested_where_they_exist() {
        let provider = |url: &str, model: &str| {
            HttpProvider::new(
                ProviderConfig {
                    kind: ProviderKind::Anthropic,
                    base_url: url.to_owned(),
                    model: model.to_owned(),
                    max_tokens: None,
                },
                None,
            )
            .unwrap()
        };
        assert!(provider("https://api.anthropic.com", "claude-opus-5-5").uses_fallbacks());
        assert!(!provider("https://api.anthropic.com", "claude-haiku-5-5").uses_fallbacks());
        assert!(!provider("https://proxy.example.com", "claude-opus-5-5").uses_fallbacks());
    }

    #[test]
    fn a_cut_request_is_noticed() {
        // Measured with Ollama, context 4096.
        assert!(!cut_suspected(2_500, Some(1139), Some(4096)));
        assert!(cut_suspected(20_000, Some(2050), Some(4096)));
        assert!(cut_suspected(200_000, Some(2050), Some(4096)));
        // Without knowing the context, the ratio still shows a large cut.
        assert!(cut_suspected(200_000, Some(2050), None));
        assert!(!cut_suspected(2_000, Some(600), None));
        // Without a count there is nothing to say.
        assert!(!cut_suspected(200_000, None, Some(4096)));
    }

    #[test]
    fn error_messages_are_shortened() {
        let long = "x".repeat(5000);
        let body = json!({"error": {"message": long}}).to_string();
        let AiError::Http { status, message } = status_error(500, &body) else {
            panic!("expected an http error");
        };
        assert_eq!(status, 500);
        assert_eq!(message.chars().count(), MAX_ERROR_CHARS);
    }
}
