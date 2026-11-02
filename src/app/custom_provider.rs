//! Working copy of the Settings "custom providers" form.
//!
//! The persisted providers live in
//! [`CustomProviderStore`](crate::data::custom_providers::CustomProviderStore);
//! this is the transient editor shown inline in Settings, with `id == None`
//! while adding a new entry.

use crate::data::custom_providers::{CustomProvider, CustomProviderKind};

/// One editable text field of the custom-provider form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomProviderField {
    Name,
    ServerUrl,
    Username,
    Password,
}

/// State of the form's "test connection" probe.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ConnectionTest {
    #[default]
    Idle,
    Testing,
    Ok(String),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct CustomProviderForm {
    /// `Some(id)` while editing an existing provider, `None` while adding.
    pub id: Option<u64>,
    pub name: String,
    pub kind: CustomProviderKind,
    pub server_url: String,
    pub username: String,
    pub password: String,
    pub test: ConnectionTest,
    /// Request id of the in-flight test, so a late reply from a previous form
    /// cannot overwrite the current one.
    pub test_rid: u64,
}

impl Default for CustomProviderForm {
    fn default() -> Self {
        Self {
            id: None,
            name: String::new(),
            kind: CustomProviderKind::default(),
            server_url: String::new(),
            username: String::new(),
            password: String::new(),
            test: ConnectionTest::Idle,
            test_rid: 0,
        }
    }
}

impl CustomProviderForm {
    pub fn from_provider(provider: &CustomProvider) -> Self {
        Self {
            id: Some(provider.id),
            name: provider.name.clone(),
            kind: provider.kind,
            server_url: provider.server_url.clone(),
            username: provider.username.clone(),
            password: provider.password.clone(),
            test: ConnectionTest::Idle,
            test_rid: 0,
        }
    }

    pub fn set_field(&mut self, field: CustomProviderField, value: String) {
        match field {
            CustomProviderField::Name => self.name = value,
            CustomProviderField::ServerUrl => self.server_url = value,
            CustomProviderField::Username => self.username = value,
            CustomProviderField::Password => self.password = value,
        }
        self.clear_test();
    }

    pub fn set_kind(&mut self, kind: CustomProviderKind) {
        self.kind = kind;
        self.clear_test();
    }

    fn clear_test(&mut self) {
        self.test = ConnectionTest::Idle;
        self.test_rid = 0;
    }

    pub fn into_provider(self, id: u64) -> CustomProvider {
        CustomProvider {
            id,
            name: self.name.trim().to_string(),
            kind: self.kind,
            server_url: self.server_url.trim().to_string(),
            username: self.username.trim().to_string(),
            password: self.password,
        }
    }

    /// `None` when the required fields are blank.
    pub fn validated(&self) -> Option<()> {
        let filled = [&self.name, &self.server_url, &self.username]
            .iter()
            .all(|field| !field.trim().is_empty());
        filled.then_some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_fields_are_trimmed_and_checked() {
        let mut form = CustomProviderForm::default();
        assert!(form.validated().is_none());
        form.name = "  ".into();
        form.server_url = "http://localhost:4533".into();
        form.username = "admin".into();
        assert!(form.validated().is_none());
        form.name = " Home ".into();
        assert!(form.validated().is_some());

        let provider = form.into_provider(7);
        assert_eq!(provider.id, 7);
        assert_eq!(provider.name, "Home");
        assert_eq!(provider.server_url, "http://localhost:4533");
    }

    #[test]
    fn editing_clears_the_previous_test_result() {
        let mut form = CustomProviderForm {
            test: ConnectionTest::Ok("Navidrome".into()),
            ..CustomProviderForm::default()
        };
        form.set_field(CustomProviderField::ServerUrl, "http://other".into());
        assert_eq!(form.test, ConnectionTest::Idle);
    }

    #[test]
    fn changing_kind_clears_the_previous_test_result() {
        let mut form = CustomProviderForm {
            test: ConnectionTest::Failed("offline".into()),
            test_rid: 12,
            ..CustomProviderForm::default()
        };
        form.set_kind(CustomProviderKind::Subsonic);
        assert_eq!(form.test, ConnectionTest::Idle);
        assert_eq!(form.test_rid, 0);
    }
}
