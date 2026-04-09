use super::errors::ProviderError;
use crate::providers::base::Provider;
use async_trait::async_trait;
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;

pub const DEFAULT_MAX_RETRIES: usize = 3;
pub const DEFAULT_INITIAL_RETRY_INTERVAL_MS: u64 = 1000;
pub const DEFAULT_BACKOFF_MULTIPLIER: f64 = 2.0;
pub const DEFAULT_MAX_RETRY_INTERVAL_MS: u64 = 30_000;

/// Optional tuning for `ProviderRetry` (HTTP retries on rate limits, 5xx, etc.).
///
/// - `GOOSE_PROVIDER_MAX_RETRIES` — max retry attempts (default: [`DEFAULT_MAX_RETRIES`]).
/// - `GOOSE_PROVIDER_INITIAL_RETRY_INTERVAL_MS` — first backoff (default: [`DEFAULT_INITIAL_RETRY_INTERVAL_MS`]).
/// - `GOOSE_PROVIDER_BACKOFF_MULTIPLIER` — exponential factor (default: [`DEFAULT_BACKOFF_MULTIPLIER`]).
/// - `GOOSE_PROVIDER_MAX_RETRY_INTERVAL_MS` — backoff cap in ms (default: [`DEFAULT_MAX_RETRY_INTERVAL_MS`]).
///
/// Invalid or empty values fall back to the defaults above. `GOOSE_PROVIDER_SKIP_BACKOFF` still
/// disables sleeping between attempts when set to `true`.

#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub(crate) max_retries: usize,
    /// Initial interval between retries in milliseconds
    pub(crate) initial_interval_ms: u64,
    /// Multiplier for backoff (exponential)
    pub(crate) backoff_multiplier: f64,
    /// Maximum interval between retries in milliseconds
    pub(crate) max_interval_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self::from_env_or_default()
    }
}

fn env_parse<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(default)
}

impl RetryConfig {
    pub fn from_env_or_default() -> Self {
        Self {
            max_retries: env_parse("GOOSE_PROVIDER_MAX_RETRIES", DEFAULT_MAX_RETRIES),
            initial_interval_ms: env_parse(
                "GOOSE_PROVIDER_INITIAL_RETRY_INTERVAL_MS",
                DEFAULT_INITIAL_RETRY_INTERVAL_MS,
            ),
            backoff_multiplier: env_parse(
                "GOOSE_PROVIDER_BACKOFF_MULTIPLIER",
                DEFAULT_BACKOFF_MULTIPLIER,
            ),
            max_interval_ms: env_parse(
                "GOOSE_PROVIDER_MAX_RETRY_INTERVAL_MS",
                DEFAULT_MAX_RETRY_INTERVAL_MS,
            ),
        }
    }

    pub fn new(
        max_retries: usize,
        initial_interval_ms: u64,
        backoff_multiplier: f64,
        max_interval_ms: u64,
    ) -> Self {
        Self {
            max_retries,
            initial_interval_ms,
            backoff_multiplier,
            max_interval_ms,
        }
    }

    pub fn max_retries(&self) -> usize {
        self.max_retries
    }

    pub fn delay_for_attempt(&self, attempt: usize) -> Duration {
        if attempt == 0 {
            return Duration::from_millis(0);
        }

        let exponent = (attempt - 1) as u32;
        let base_delay_ms = (self.initial_interval_ms as f64
            * self.backoff_multiplier.powi(exponent as i32)) as u64;

        let capped_delay_ms = std::cmp::min(base_delay_ms, self.max_interval_ms);

        let jitter_factor_to_avoid_thundering_herd = 0.8 + (rand::random::<f64>() * 0.4);
        let jitter_delay_ms =
            (capped_delay_ms as f64 * jitter_factor_to_avoid_thundering_herd) as u64;

        Duration::from_millis(jitter_delay_ms)
    }
}

pub fn should_retry(error: &ProviderError) -> bool {
    matches!(
        error,
        ProviderError::RateLimitExceeded { .. }
            | ProviderError::ServerError(_)
            | ProviderError::RequestFailed(_)
    )
}

pub async fn retry_operation<F, Fut, T>(
    config: &RetryConfig,
    operation: F,
) -> Result<T, ProviderError>
where
    F: Fn() -> Fut + Send,
    Fut: Future<Output = Result<T, ProviderError>> + Send,
    T: Send,
{
    let mut attempts = 0;

    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(error) => {
                if should_retry(&error) && attempts < config.max_retries {
                    attempts += 1;
                    tracing::warn!(
                        "Request failed, retrying ({}/{}): {:?}",
                        attempts,
                        config.max_retries,
                        error
                    );

                    let delay = match &error {
                        ProviderError::RateLimitExceeded {
                            retry_delay: Some(d),
                            ..
                        } => *d,
                        _ => config.delay_for_attempt(attempts),
                    };

                    sleep(delay).await;
                    continue;
                }
                return Err(error);
            }
        }
    }
}

/// Trait for retry functionality to keep Provider dyn-compatible
#[async_trait]
pub trait ProviderRetry {
    fn retry_config(&self) -> RetryConfig {
        RetryConfig::default()
    }

    async fn with_retry<F, Fut, T>(&self, operation: F) -> Result<T, ProviderError>
    where
        F: Fn() -> Fut + Send,
        Fut: Future<Output = Result<T, ProviderError>> + Send,
        T: Send,
    {
        self.with_retry_config(operation, self.retry_config()).await
    }

    async fn with_retry_config<F, Fut, T>(
        &self,
        operation: F,
        config: RetryConfig,
    ) -> Result<T, ProviderError>
    where
        F: Fn() -> Fut + Send,
        Fut: Future<Output = Result<T, ProviderError>> + Send,
        T: Send,
    {
        let mut attempts = 0;

        loop {
            return match operation().await {
                Ok(result) => Ok(result),
                Err(error) => {
                    if should_retry(&error) && attempts < config.max_retries {
                        attempts += 1;
                        tracing::warn!(
                            "Request failed, retrying ({}/{}): {:?}",
                            attempts,
                            config.max_retries,
                            error
                        );

                        let delay = match &error {
                            ProviderError::RateLimitExceeded {
                                retry_delay: Some(provider_delay),
                                ..
                            } => *provider_delay,
                            _ => config.delay_for_attempt(attempts),
                        };

                        let skip_backoff = std::env::var("GOOSE_PROVIDER_SKIP_BACKOFF")
                            .unwrap_or_default()
                            .parse::<bool>()
                            .unwrap_or(false);

                        if skip_backoff {
                            tracing::info!("Skipping backoff due to GOOSE_PROVIDER_SKIP_BACKOFF");
                        } else {
                            tracing::info!("Backing off for {:?} before retry", delay);
                            sleep(delay).await;
                        }
                        continue;
                    }

                    Err(error)
                }
            };
        }
    }
}

impl<P: Provider> ProviderRetry for P {
    fn retry_config(&self) -> RetryConfig {
        Provider::retry_config(self)
    }
}
