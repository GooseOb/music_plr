//! Shared HTTP plumbing for the lyrics providers: one agent with connect +
//! overall timeouts so a dead provider can't hang a background thread
//! indefinitely, plus a JSON GET that treats 404 as "no record".

use anyhow::{Context, Result};

pub(super) const USER_AGENT: &str = "goosemusic/0.1 (https://github.com/gooseob/music_plr)";

pub(super) fn agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::config::Config::builder()
            .timeout_connect(Some(std::time::Duration::from_secs(15)))
            .timeout_global(Some(std::time::Duration::from_secs(15)))
            .build()
            .new_agent()
    })
}

pub(super) fn get_json_opt<T: serde::de::DeserializeOwned>(
    url: &str,
    what: &str,
) -> Result<Option<T>> {
    match agent().get(url).header("User-Agent", USER_AGENT).call() {
        Ok(mut r) => {
            Ok(Some(r.body_mut().read_json().with_context(|| {
                format!("{what} response was not valid JSON")
            })?))
        }
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(e) => Err(e).with_context(|| format!("{what} request failed")),
    }
}
