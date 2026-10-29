//! Best-effort classification of provider failures, so the UI can say what
//! went wrong in the user's language.
//!
//! The text of a failure almost always comes from someone else — a web API
//! message, an HTTP status line, yt-dlp's stderr — and cannot be translated.
//! What *is* translatable is the kind of failure, so every error carries one
//! and the UI shows a localized sentence for it (see
//! [`crate::i18n::Strings::error_reason`]). The original text stays in
//! [`ProviderError::detail`]: it goes to the log, and it is what the user sees
//! when the kind is [`ErrorKind::Other`] because nothing else would name the
//! cause.

use std::io::ErrorKind as IoErrorKind;

/// What kind of failure a provider call hit. Each variant except `Other` has
/// one localized sentence; `Other` shows the provider's own message instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// The request never completed: offline, DNS failure, timeout, reset.
    Network,
    /// The service is throttling requests.
    RateLimited,
    /// Access was refused: sign-in wall, bot check, bad credentials.
    Denied,
    /// The track/artist/page doesn't exist (anymore) on that provider.
    Unavailable,
    /// A helper binary (yt-dlp, Python 3) is missing or won't run.
    ToolMissing,
    /// The answer didn't parse: unexpected HTML/JSON, truncated body.
    Parse,
    /// Anything unrecognized; the provider's own message is shown.
    Other,
}

/// A provider failure: its [`ErrorKind`] plus the original message.
#[derive(Debug, Clone)]
pub struct ProviderError {
    pub kind: ErrorKind,
    pub detail: String,
}

impl ProviderError {
    pub fn new(kind: ErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    /// Classify a failure that only survives as text: a third-party client
    /// error that was flattened to `String` before it reached us.
    pub fn from_text(detail: impl Into<String>) -> Self {
        let detail = detail.into();
        let kind = classify_text(&detail);
        Self { kind, detail }
    }
}

impl From<anyhow::Error> for ProviderError {
    fn from(err: anyhow::Error) -> Self {
        let kind = classify_error(&err);
        Self {
            kind,
            detail: format!("{err:#}"),
        }
    }
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

fn classify_error(err: &anyhow::Error) -> ErrorKind {
    if let Some(http) = err.downcast_ref::<ureq::Error>() {
        return match http {
            ureq::Error::StatusCode(code) => classify_status(*code),
            ureq::Error::Timeout(_) | ureq::Error::HostNotFound | ureq::Error::Io(_) => {
                ErrorKind::Network
            }
            _ => ErrorKind::Other,
        };
    }
    if let Some(io) = err.downcast_ref::<std::io::Error>() {
        return match io.kind() {
            IoErrorKind::NotFound => ErrorKind::ToolMissing,
            IoErrorKind::PermissionDenied => ErrorKind::Denied,
            IoErrorKind::TimedOut
            | IoErrorKind::ConnectionRefused
            | IoErrorKind::ConnectionReset
            | IoErrorKind::ConnectionAborted
            | IoErrorKind::NotConnected
            | IoErrorKind::BrokenPipe
            | IoErrorKind::UnexpectedEof
            | IoErrorKind::HostUnreachable
            | IoErrorKind::NetworkUnreachable => ErrorKind::Network,
            _ => ErrorKind::Other,
        };
    }
    if err.downcast_ref::<serde_json::Error>().is_some() {
        return ErrorKind::Parse;
    }
    classify_text(&format!("{err:#}"))
}

fn classify_status(code: u16) -> ErrorKind {
    match code {
        401 | 403 => ErrorKind::Denied,
        404 | 410 => ErrorKind::Unavailable,
        429 => ErrorKind::RateLimited,
        500..=599 => ErrorKind::Network,
        _ => ErrorKind::Other,
    }
}

/// Third-party messages that survive only as text (yt-dlp's stderr, an API's
/// error body, a `String`-returning helper). Checked in order: a missing tool
/// outranks everything, "network" outranks "unavailable" so that a `503
/// Service Unavailable` reads as a server problem rather than as missing
/// content, and a bare "not found" is left for last among the causes.
const MARKERS: &[(ErrorKind, &[&str])] = &[
    (
        ErrorKind::ToolMissing,
        &[
            "not installed",
            "command not found",
            "no such file or directory",
            "python 3 not found",
            "no module named",
            "modulenotfounderror",
        ],
    ),
    (
        ErrorKind::Denied,
        &[
            "sign in to confirm",
            "not a bot",
            "requires authentication",
            "requires signing in",
            "login required",
            "unauthorized",
            "forbidden",
            "http error 401",
            "http error 403",
            "status(401)",
            "status(403)",
        ],
    ),
    (
        ErrorKind::RateLimited,
        &[
            "too many requests",
            "rate limit",
            "rate-limit",
            "http error 429",
            "status(429)",
        ],
    ),
    (
        ErrorKind::Network,
        &[
            "timed out",
            "timeout",
            "connection refused",
            "connection reset",
            "failed to connect",
            "host not found",
            "name resolution",
            "temporary failure",
            "network",
            "dns",
            "http error 5",
            "status(5",
            "service unavailable",
        ],
    ),
    (
        ErrorKind::Unavailable,
        &[
            "video unavailable",
            "not available",
            "no longer available",
            "private video",
            "has been removed",
            "does not exist",
            "could not find",
            "not found",
            "http error 404",
            "status(404)",
        ],
    ),
    (
        ErrorKind::Parse,
        &[
            "expected value",
            "expecting value",
            "invalid json",
            "json decode",
            "jsondecodeerror",
            "parse error",
            "unexpected end of",
            "trailing characters",
        ],
    ),
];

fn classify_text(text: &str) -> ErrorKind {
    let lowered = text.to_lowercase();
    MARKERS
        .iter()
        .find(|(_, markers)| markers.iter().any(|m| lowered.contains(m)))
        .map_or(ErrorKind::Other, |(kind, _)| *kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_http_status_codes() {
        assert_eq!(classify_status(403), ErrorKind::Denied);
        assert_eq!(classify_status(404), ErrorKind::Unavailable);
        assert_eq!(classify_status(429), ErrorKind::RateLimited);
        assert_eq!(classify_status(503), ErrorKind::Network);
        assert_eq!(classify_status(400), ErrorKind::Other);
    }

    #[test]
    fn classifies_tool_output() {
        let cases = [
            (
                "ERROR: [youtube] abc: Video unavailable",
                ErrorKind::Unavailable,
            ),
            (
                "ERROR: Sign in to confirm you're not a bot",
                ErrorKind::Denied,
            ),
            (
                "ERROR: [youtube] abc: Video unavailable. This video has been removed",
                ErrorKind::Unavailable,
            ),
            (
                "This video is not available in your country",
                ErrorKind::Unavailable,
            ),
            (
                "ERROR: HTTP Error 429: Too Many Requests",
                ErrorKind::RateLimited,
            ),
            (
                "ERROR: HTTP Error 503: Service Unavailable",
                ErrorKind::Network,
            ),
            (
                "unable to download webpage: Temporary failure in name resolution",
                ErrorKind::Network,
            ),
            ("yt-dlp is not installed", ErrorKind::ToolMissing),
            (
                "ytmusicapi search failed: ModuleNotFoundError: No module named 'ytmusicapi'",
                ErrorKind::ToolMissing,
            ),
            (
                "error sending request: connection refused",
                ErrorKind::Network,
            ),
            ("expected value at line 1 column 1", ErrorKind::Parse),
            (
                "ytmusicapi search failed: json.decoder.JSONDecodeError",
                ErrorKind::Parse,
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(ProviderError::from_text(text).kind, expected, "{text}");
        }
    }

    #[test]
    fn keeps_unrecognized_text_verbatim() {
        let err = ProviderError::from_text("some provider SDK did something odd");
        assert_eq!(err.kind, ErrorKind::Other);
        assert_eq!(err.detail, "some provider SDK did something odd");
    }

    #[test]
    fn reports_the_whole_context_chain() {
        let err: anyhow::Error =
            anyhow::anyhow!("connection refused").context("request to https://example.test failed");
        assert_eq!(err.to_string(), "request to https://example.test failed");
        assert_eq!(
            ProviderError::from(err).detail,
            "request to https://example.test failed: connection refused"
        );
    }
}
