//! Per-provider backends.
//!
//! Each module implements the provider-specific search/resolve/download logic
//! and returns [`crate::types::Track`]s carrying that provider's id. All four
//! providers are dispatched from [`crate::provider`].

pub mod jamendo;
pub mod musicbrainz;
pub mod soundcloud;
pub mod youtube;
