//! Shared fetch-state wrapper: a slot is either loading, holding its
//! content, or carrying the error message from a failed fetch.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LoadState<T, E = String> {
    Ready(T),
    Failed(E),
    #[default]
    Loading,
}

impl<T, E> LoadState<T, E> {
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }
}
