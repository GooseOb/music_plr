//! Per-provider backends for non-YouTube music sources.
//!
//! Each module implements the provider-specific search/resolve/download logic
//! and returns [`crate::types::Track`]s carrying that provider's id. `YouTube`
//! itself lives in `crate::youtube` and is dispatched from `crate::provider`.

pub mod jamendo;
pub mod musicbrainz;
pub mod soundcloud;
