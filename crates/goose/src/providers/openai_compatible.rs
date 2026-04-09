use anyhow::Error;
use async_stream::try_stream;
use futures::TryStreamExt;
use reqwest::header::HeaderMap;
use reqwest::{Response, StatusCode};
use serde_json::Value;
use std::time::{Duration, SystemTime};
use tokio::pin;
use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, LinesCodec};
use tokio_util::io::StreamReader;

use super::api_client::ApiClient;
use super::base::{MessageStream, Provider};
use super::errors::ProviderError;
use super::retry::ProviderRetry;
use super::utils::{ImageFormat, RequestLog};
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use crate::providers::formats::openai::{create_request, response_to_streaming_message};
use rmcp::model::Tool;

/// Caps sleeps from `Retry-After` or JSON hints so retries stay bounded (see `ProviderRetry`).
const MAX_RATE_LIMIT_RETRY_SECS: u64 = 900;
const MAX_RATE_LIMIT_RETRY_DELAY: Duration = Duration::from_secs(MAX_RATE_LIMIT_RETRY_SECS);
const MAX_RATE_LIMIT_RETRY_MS: u64 = MAX_RATE_LIMIT_RETRY_SECS.saturating_mul(1000);

fn cap_rate_limit_delay(d: Duration) -> Duration {
    d.min(MAX_RATE_LIMIT_RETRY_DELAY)
}

fn duration_from_retry_secs_sanitized(secs: u64) -> Duration {
    Duration::from_secs(secs.min(MAX_RATE_LIMIT_RETRY_SECS))
}

fn duration_from_retry_millis_sanitized(ms: u64) -> Duration {
    Duration::from_millis(ms.min(MAX_RATE_LIMIT_RETRY_MS))
}

/// Parses the HTTP `Retry-After` header: delay in seconds (integer) or an HTTP-date.
pub fn parse_retry_after_header(headers: &HeaderMap) -> Option<Duration> {
    let raw = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(secs) = s.parse::<u64>() {
        return Some(duration_from_retry_secs_sanitized(secs));
    }
    let target = httpdate::parse_http_date(s).ok()?;
    let now = SystemTime::now();
    let dur = target.duration_since(now).unwrap_or(Duration::ZERO);
    Some(cap_rate_limit_delay(dur))
}

fn parse_retry_delay_from_json(payload: Option<&Value>) -> Option<Duration> {
    let p = payload?;
    if let Some(ms) = p.get("retry_after_ms").and_then(|v| v.as_u64()) {
        return Some(duration_from_retry_millis_sanitized(ms));
    }
    if let Some(secs) = p.get("retry_after").and_then(|v| v.as_u64()) {
        return Some(duration_from_retry_secs_sanitized(secs));
    }
    let err = p.get("error")?;
    if let Some(ms) = err.get("retry_after_ms").and_then(|v| v.as_u64()) {
        return Some(duration_from_retry_millis_sanitized(ms));
    }
    if let Some(secs) = err.get("retry_after").and_then(|v| v.as_u64()) {
        return Some(duration_from_retry_secs_sanitized(secs));
    }
    None
}

/// Prefer the header when present (RFC 7231); otherwise use JSON fields some gateways include.
pub fn merge_rate_limit_retry_hints(
    header_delay: Option<Duration>,
    payload: Option<&Value>,
) -> Option<Duration> {
    header_delay.or_else(|| parse_retry_delay_from_json(payload))
}

pub struct OpenAiCompatibleProvider {
    name: String,
    /// Client targeted at the base URL (e.g. `https://api.x.ai/v1`)
    api_client: ApiClient,
    model: ModelConfig,
    /// Path prefix prepended to `chat/completions` (e.g. `"deployments/{name}/"` for Azure).
    completions_prefix: String,
}

impl OpenAiCompatibleProvider {
    pub fn new(
        name: String,
        api_client: ApiClient,
        model: ModelConfig,
        completions_prefix: String,
    ) -> Self {
        Self {
            name,
            api_client,
            model,
            completions_prefix,
        }
    }

    fn build_request(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
        for_streaming: bool,
    ) -> Result<Value, ProviderError> {
        create_request(
            model_config,
            system,
            messages,
            tools,
            &ImageFormat::OpenAi,
            for_streaming,
        )
        .map_err(|e| ProviderError::RequestFailed(format!("Failed to create request: {}", e)))
    }
}

#[async_trait::async_trait]
impl Provider for OpenAiCompatibleProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_model_config(&self) -> ModelConfig {
        self.model.clone()
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        let response = self
            .api_client
            .response_get(None, "models")
            .await
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;
        let json = handle_response_openai_compat(response).await?;

        if let Some(err_obj) = json.get("error") {
            let msg = err_obj
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(ProviderError::Authentication(msg.to_string()));
        }

        let arr = json.get("data").and_then(|v| v.as_array()).ok_or_else(|| {
            ProviderError::RequestFailed("Missing 'data' array in models response".to_string())
        })?;
        let mut models: Vec<String> = arr
            .iter()
            .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(str::to_string))
            .collect();
        models.sort();
        Ok(models)
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        session_id: &str,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let payload = self.build_request(model_config, system, messages, tools, true)?;
        let mut log = RequestLog::start(model_config, &payload)?;

        let completions_path = format!("{}chat/completions", self.completions_prefix);
        let response = self
            .with_retry(|| async {
                let resp = self
                    .api_client
                    .response_post(Some(session_id), &completions_path, &payload)
                    .await?;
                handle_status_openai_compat(resp).await
            })
            .await
            .inspect_err(|e| {
                let _ = log.error(e);
            })?;

        stream_openai_compat(response, log)
    }
}

fn check_context_length_exceeded(text: &str) -> bool {
    let check_phrases = [
        "too long",
        "context length",
        "context_length_exceeded",
        "reduce the length",
        "token count",
        "exceeds",
        "exceed context limit",
        "input length",
        "max_tokens",
        "decrease input length",
        "context limit",
        "maximum prompt length",
    ];
    let text_lower = text.to_lowercase();
    check_phrases
        .iter()
        .any(|phrase| text_lower.contains(phrase))
}

pub fn map_http_error_to_provider_error(
    status: StatusCode,
    payload: Option<Value>,
    retry_after_hint: Option<Duration>,
) -> ProviderError {
    let extract_message = || -> String {
        payload
            .as_ref()
            .and_then(|p| {
                p.get("error")
                    .and_then(|e| e.get("message"))
                    .or_else(|| p.get("message"))
                    .and_then(|m| m.as_str())
                    .map(String::from)
            })
            .unwrap_or_else(|| payload.as_ref().map(|p| p.to_string()).unwrap_or_default())
    };

    let error = match status {
        StatusCode::OK => unreachable!("Should not call this function with OK status"),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ProviderError::Authentication(format!(
            "Authentication failed. Status: {}. Response: {}",
            status,
            extract_message()
        )),
        StatusCode::NOT_FOUND => {
            ProviderError::RequestFailed(format!("Resource not found (404): {}", extract_message()))
        }
        StatusCode::PAYMENT_REQUIRED => ProviderError::CreditsExhausted {
            details: extract_message(),
            top_up_url: None,
        },
        StatusCode::PAYLOAD_TOO_LARGE => ProviderError::ContextLengthExceeded(extract_message()),
        StatusCode::BAD_REQUEST => {
            let payload_str = extract_message();
            if check_context_length_exceeded(&payload_str) {
                ProviderError::ContextLengthExceeded(payload_str)
            } else {
                ProviderError::RequestFailed(format!("Bad request (400): {}", payload_str))
            }
        }
        StatusCode::TOO_MANY_REQUESTS => ProviderError::RateLimitExceeded {
            details: extract_message(),
            retry_delay: merge_rate_limit_retry_hints(retry_after_hint, payload.as_ref()),
        },
        _ if status.is_server_error() => {
            ProviderError::ServerError(format!("Server error ({}): {}", status, extract_message()))
        }
        _ => ProviderError::RequestFailed(format!(
            "Request failed with status {}: {}",
            status,
            extract_message()
        )),
    };

    if !status.is_success() {
        tracing::warn!(
            "Provider request failed with status: {}. Payload: {:?}. Returning error: {:?}",
            status,
            payload,
            error
        );
    }

    error
}

pub async fn handle_status_openai_compat(response: Response) -> Result<Response, ProviderError> {
    let status = response.status();
    if !status.is_success() {
        let retry_after_hint = parse_retry_after_header(response.headers());
        let body = response.text().await.unwrap_or_default();
        let payload = serde_json::from_str::<Value>(&body).ok();
        return Err(map_http_error_to_provider_error(
            status,
            payload,
            retry_after_hint,
        ));
    }
    Ok(response)
}

pub async fn handle_response_openai_compat(response: Response) -> Result<Value, ProviderError> {
    let response = handle_status_openai_compat(response).await?;

    response.json::<Value>().await.map_err(|e| {
        ProviderError::RequestFailed(format!("Response body is not valid JSON: {}", e))
    })
}

pub fn stream_openai_compat(
    response: Response,
    mut log: RequestLog,
) -> Result<MessageStream, ProviderError> {
    let stream = response.bytes_stream().map_err(std::io::Error::other);

    Ok(Box::pin(try_stream! {
        let stream_reader = StreamReader::new(stream);
        let framed = FramedRead::new(stream_reader, LinesCodec::new())
            .map_err(Error::from);

        let message_stream = response_to_streaming_message(framed);
        pin!(message_stream);
        while let Some(message) = message_stream.next().await {
            let (message, usage) = message.map_err(|e|
                ProviderError::RequestFailed(format!("Stream decode error: {}", e))
            )?;
            log.write(&message, usage.as_ref().map(|f| f.usage).as_ref())?;
            yield (message, usage);
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use test_case::test_case;

    #[test_case(
        StatusCode::PAYMENT_REQUIRED,
        Some(json!({"error": {"message": "Insufficient credits to complete this request"}})),
        "CreditsExhausted"
        ; "402 with payload"
    )]
    #[test_case(
        StatusCode::PAYMENT_REQUIRED,
        None,
        "CreditsExhausted"
        ; "402 without payload"
    )]
    #[test_case(
        StatusCode::TOO_MANY_REQUESTS,
        Some(json!({"error": {"message": "Rate limit exceeded"}})),
        "RateLimitExceeded"
        ; "429 rate limit"
    )]
    #[test_case(
        StatusCode::UNAUTHORIZED,
        None,
        "Authentication"
        ; "401 unauthorized"
    )]
    #[test_case(
        StatusCode::BAD_REQUEST,
        Some(json!({"error": {"message": "This request exceeds the maximum context length"}})),
        "ContextLengthExceeded"
        ; "400 context length"
    )]
    #[test_case(
        StatusCode::INTERNAL_SERVER_ERROR,
        None,
        "ServerError"
        ; "500 server error"
    )]
    fn http_status_maps_to_expected_error(
        status: StatusCode,
        payload: Option<Value>,
        expected_variant: &str,
    ) {
        let err = map_http_error_to_provider_error(status, payload, None);
        let actual = err.telemetry_type();
        let expected_telemetry = match expected_variant {
            "CreditsExhausted" => "credits_exhausted",
            "RateLimitExceeded" => "rate_limit",
            "Authentication" => "auth",
            "ContextLengthExceeded" => "context_length",
            "ServerError" => "server",
            other => panic!("Unknown variant: {other}"),
        };
        assert_eq!(
            actual, expected_telemetry,
            "Expected {expected_variant}, got error: {err:?}"
        );
    }

    #[test]
    fn rate_limit_includes_json_retry_after() {
        let err = map_http_error_to_provider_error(
            StatusCode::TOO_MANY_REQUESTS,
            Some(json!({"error": {"message": "slow down", "retry_after": 42}})),
            None,
        );
        assert!(
            matches!(err, ProviderError::RateLimitExceeded { retry_delay: Some(d), .. } if d == Duration::from_secs(42)),
            "unexpected err: {err:?}"
        );
    }

    #[test]
    fn rate_limit_prefers_retry_after_header_over_json() {
        let err = map_http_error_to_provider_error(
            StatusCode::TOO_MANY_REQUESTS,
            Some(json!({"error": {"message": "slow", "retry_after": 10}})),
            Some(Duration::from_secs(99)),
        );
        assert!(
            matches!(err, ProviderError::RateLimitExceeded { retry_delay: Some(d), .. } if d == Duration::from_secs(99)),
            "unexpected err: {err:?}"
        );
    }

    #[test]
    fn parse_retry_after_seconds_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("7"),
        );
        assert_eq!(
            parse_retry_after_header(&headers),
            Some(Duration::from_secs(7))
        );
    }

    #[test]
    fn rate_limit_json_extreme_retry_after_does_not_panic() {
        let err = map_http_error_to_provider_error(
            StatusCode::TOO_MANY_REQUESTS,
            Some(json!({"retry_after": u64::MAX})),
            None,
        );
        assert!(
            matches!(err, ProviderError::RateLimitExceeded { retry_delay: Some(d), .. } if d == MAX_RATE_LIMIT_RETRY_DELAY),
            "unexpected err: {err:?}"
        );
    }
}
