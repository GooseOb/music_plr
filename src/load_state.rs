//! Shared fetch-state wrapper: a slot is either loading, holding its
//! content, or carrying the error message from a failed fetch.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LoadState<T, E = String> {
    Ready(T),
    Failed(E),
    Loading,
}

impl<T> LoadState<T> {
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }
}

impl<T: Default> Default for LoadState<T> {
    fn default() -> Self {
        Self::Ready(T::default())
    }
}
