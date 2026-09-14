//! The production identity boundary is a private, secret-authenticated OIDC proxy.
use crate::*;
use axum::{extract::Request, middleware::Next};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Mutex,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct Config {
    pub production: bool,
    pub proxy_secret: Option<String>,
    pub identity_namespace: String,
    pub origins: Vec<String>,
    pub requests_per_minute: u32,
    pub max_saved_plans: i64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            production: false,
            proxy_secret: None,
            identity_namespace: "development".into(),
            origins: vec![
                "http://127.0.0.1:8081".into(),
                "http://localhost:8081".into(),
                "http://127.0.0.1:8080".into(),
                "http://localhost:8080".into(),
                "http://dioxus.index".into(),
                "https://dioxus.index".into(),
            ],
            requests_per_minute: 10_000,
            max_saved_plans: 1000,
        }
    }
}
impl Config {
    pub fn from_env() -> Result<Self, String> {
        Self::from_values(|key| std::env::var(key).ok())
    }
    pub fn from_values(value: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let mode = value("APP_ENV").unwrap_or_else(|| "development".into());
        if !["development", "production"].contains(&mode.as_str()) {
            return Err("APP_ENV must be development or production.".into());
        }
        let mut c = Self {
            production: mode == "production",
            ..Self::default()
        };
        if c.production {
            let secret =
                value("PROXY_AUTH_SECRET").ok_or("PROXY_AUTH_SECRET is required in production.")?;
            if secret.len() < 32
                || secret.contains("replace")
                || !secret
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
            {
                return Err(
                    "PROXY_AUTH_SECRET must contain at least 32 random URL-safe characters.".into(),
                );
            }
            c.proxy_secret = Some(secret);
            c.identity_namespace = value("OIDC_ISSUER_URL")
                .filter(|s| s.starts_with("https://") && s.len() < 512)
                .ok_or("Set the HTTPS OIDC_ISSUER_URL.")?;
            let origin =
                value("PUBLIC_ORIGIN").ok_or("PUBLIC_ORIGIN is required in production.")?;
            let uri: axum::http::Uri = origin
                .parse()
                .map_err(|_| "PUBLIC_ORIGIN must be an HTTPS origin.")?;
            if uri.scheme_str() != Some("https")
                || uri.authority().is_none()
                || uri.authority().is_some_and(|a| a.as_str().contains('@'))
                || uri.path_and_query().is_some_and(|p| p.as_str() != "/")
                || origin.ends_with('/')
            {
                return Err("PUBLIC_ORIGIN must be an HTTPS origin without credentials, a path or trailing slash.".into());
            }
            c.origins = vec![origin];
            c.requests_per_minute = 120;
            c.max_saved_plans = 100;
            if value("ALLOW_DEMO_DATA").as_deref() != Some("true") {
                return Err("This catalogue contains unverified demo data. Production mode requires explicit ALLOW_DEMO_DATA=true for a labelled demo deployment; review and replace the dataset before a nutrition product launch.".into());
            }
        }
        if let Some(n) = value("REQUESTS_PER_MINUTE") {
            c.requests_per_minute = n
                .parse()
                .ok()
                .filter(|n| *n > 0 && *n <= 10_000)
                .ok_or("REQUESTS_PER_MINUTE must be 1–10000.")?;
        }
        if let Some(n) = value("MAX_SAVED_PLANS") {
            c.max_saved_plans = n
                .parse()
                .ok()
                .filter(|n| *n > 0 && *n <= 1000)
                .ok_or("MAX_SAVED_PLANS must be 1–1000.")?;
        }
        Ok(c)
    }
    pub fn validate_bind(&self, bind: SocketAddr) -> Result<(), String> {
        if !self.production && !bind.ip().is_loopback() {
            return Err("Development mode must bind to loopback. Configure production authentication before exposing the server.".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct Principal(pub String);
pub struct Runtime {
    pub config: Config,
    windows: Mutex<HashMap<String, (Instant, u32)>>,
    pub slots: Arc<Semaphore>,
}
impl Runtime {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            windows: Mutex::new(HashMap::new()),
            slots: Arc::new(Semaphore::new(16)),
        }
    }
    fn allow(&self, owner: &str) -> bool {
        let Ok(mut windows) = self.windows.lock() else {
            return false;
        };
        let now = Instant::now();
        windows.retain(|_, (start, _)| now.duration_since(*start) < Duration::from_secs(60));
        if !windows.contains_key(owner) && windows.len() >= 4096 {
            return false;
        }
        let entry = windows.entry(owner.into()).or_insert((now, 0));
        if entry.1 >= self.config.requests_per_minute {
            return false;
        }
        entry.1 += 1;
        true
    }
    fn principal(&self, headers: &axum::http::HeaderMap) -> Result<Principal, Error> {
        if !self.config.production {
            return Ok(Principal("development".into()));
        }
        let supplied = headers
            .get("x-proxy-secret")
            .map(|h| h.as_bytes())
            .unwrap_or_default();
        let secret = self.config.proxy_secret.as_deref().unwrap_or_default();
        if secret.is_empty()
            || supplied.len() != secret.len()
            || !bool::from(supplied.ct_eq(secret.as_bytes()))
        {
            return Err(Error(
                StatusCode::UNAUTHORIZED,
                "Sign in to access your plans.".into(),
            ));
        }
        let user = headers
            .get("x-forwarded-user")
            .and_then(|h| h.to_str().ok())
            .filter(|s| !s.trim().is_empty() && s.len() <= 256)
            .ok_or_else(|| {
                Error(
                    StatusCode::UNAUTHORIZED,
                    "An authenticated user identity is required.".into(),
                )
            })?;
        let mut hash = Sha256::new();
        hash.update(self.config.identity_namespace.as_bytes());
        hash.update([0]);
        hash.update(user.as_bytes());
        Ok(Principal(format!("user-{:x}", hash.finalize())))
    }
}

pub async fn guard(State(s): State<AppState>, mut request: Request, next: Next) -> Response {
    let start = Instant::now();
    let request_id = Uuid::new_v4().to_string();
    let path = request.uri().path().to_string();
    let method = request.method().clone();
    let route = request
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "static".into());
    let api = path.starts_with("/api/");
    let protected = api && !["/api/health", "/api/live", "/api/ready"].contains(&path.as_str());
    let config = &s.security.config;
    let result: Result<Option<tokio::sync::OwnedSemaphorePermit>, Error> = (|| {
        if request.uri().to_string().len() > 4096 {
            return Err(Error(
                StatusCode::URI_TOO_LONG,
                "Request URL is too long.".into(),
            ));
        }
        if protected {
            let principal = s.security.principal(request.headers())?;
            if config.production && (path == "/api/plans" || path.starts_with("/api/plans/")) {
                return Err(Error(StatusCode::NOT_FOUND, "Endpoint not found.".into()));
            }
            if config.production && ![Method::GET, Method::HEAD, Method::OPTIONS].contains(&method)
            {
                let origin = request
                    .headers()
                    .get("origin")
                    .and_then(|h| h.to_str().ok());
                if !origin.is_some_and(|o| config.origins.iter().any(|allowed| allowed == o)) {
                    return Err(Error(
                        StatusCode::FORBIDDEN,
                        "Request origin is not allowed.".into(),
                    ));
                }
            }
            if !s.security.allow(&principal.0) {
                return Err(Error(
                    StatusCode::TOO_MANY_REQUESTS,
                    "Too many requests. Please wait a minute.".into(),
                ));
            }
            request.extensions_mut().insert(principal);
        }
        if api {
            Ok(Some(s.security.slots.clone().try_acquire_owned().map_err(
                |_| {
                    Error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "The service is busy. Please retry shortly.".into(),
                    )
                },
            )?))
        } else {
            Ok(None)
        }
    })();
    let mut response = match result {
        Ok(_permit) => match tokio::time::timeout(Duration::from_secs(30), next.run(request)).await
        {
            Ok(r) => r,
            Err(_) => Error(
                StatusCode::GATEWAY_TIMEOUT,
                "The request timed out. Please retry.".into(),
            )
            .into_response(),
        },
        Err(e) => e.into_response(),
    };
    let status = response.status();
    let h = response.headers_mut();
    h.insert(
        "x-request-id",
        HeaderValue::from_str(&request_id).expect("UUID header"),
    );
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert("x-frame-options", HeaderValue::from_static("DENY"));
    h.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    h.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    // Dioxus document::eval needs unsafe-eval; inline scripts and external origins remain blocked.
    h.insert("content-security-policy",HeaderValue::from_static("default-src 'self'; script-src 'self' 'unsafe-eval' 'wasm-unsafe-eval'; style-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'"));
    h.insert(
        "cache-control",
        HeaderValue::from_static(if api || !path.starts_with("/assets/") {
            "no-store"
        } else {
            "public, max-age=31536000, immutable"
        }),
    );
    if config.production {
        h.insert(
            "strict-transport-security",
            HeaderValue::from_static("max-age=31536000"),
        );
    }
    if [
        StatusCode::TOO_MANY_REQUESTS,
        StatusCode::SERVICE_UNAVAILABLE,
    ]
    .contains(&status)
    {
        h.insert("retry-after", HeaderValue::from_static("60"));
    }
    if api {
        tracing::info!(%request_id,%method,%route,status=status.as_u16(),elapsed_ms=start.elapsed().as_millis() as u64,"API request");
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn production_configuration_fails_closed() {
        assert!(Config::from_values(|k| (k == "APP_ENV").then(|| "production".into())).is_err());
        assert!(
            Config::default()
                .validate_bind("0.0.0.0:8081".parse().unwrap())
                .is_err()
        );
        assert!(
            Config::default()
                .validate_bind("127.0.0.1:8081".parse().unwrap())
                .is_ok()
        );
    }
    #[test]
    fn identity_requires_secret_and_is_namespaced() {
        let runtime = Runtime::new(Config {
            production: true,
            proxy_secret: Some("a".repeat(40)),
            identity_namespace: "https://issuer.example".into(),
            ..Config::default()
        });
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-user", HeaderValue::from_static("alice"));
        assert!(runtime.principal(&headers).is_err());
        headers.insert(
            "x-proxy-secret",
            HeaderValue::from_str(&"a".repeat(40)).unwrap(),
        );
        let alice = runtime.principal(&headers).unwrap().0;
        headers.insert("x-forwarded-user", HeaderValue::from_static("bob"));
        assert_ne!(alice, runtime.principal(&headers).unwrap().0);
        assert!(!alice.contains("alice"));
    }
    #[test]
    fn request_budgets_are_per_user() {
        let runtime = Runtime::new(Config {
            requests_per_minute: 2,
            ..Config::default()
        });
        assert!(runtime.allow("alice"));
        assert!(runtime.allow("alice"));
        assert!(!runtime.allow("alice"));
        assert!(runtime.allow("bob"));
    }
}
