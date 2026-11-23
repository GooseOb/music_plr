//! Working copy of the Settings "custom providers" form.
//!
//! The persisted providers live in
//! [`CustomProviderStore`](crate::data::custom_providers::CustomProviderStore);
//! this is the transient editor shown inline in Settings, with `id == None`
//! while adding a new entry.

use crate::data::custom_providers::{
    CustomProvider, CustomProviderConnection, CustomProviderKind, S3Settings,
};

/// One editable text field of the custom-provider form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomProviderField {
    Name,
    ServerUrl,
    Username,
    Password,
    Bucket,
    Region,
    Endpoint,
    AccessKey,
    SecretKey,
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

#[derive(Debug, Clone, Default)]
pub struct CustomProviderForm {
    /// `Some(id)` while editing an existing provider, `None` while adding.
    pub id: Option<u64>,
    /// The provider being edited: its connection variant is the one the form
    /// shows, so the HTTP and S3 fields can never coexist.
    pub draft: CustomProvider,
    pub test: ConnectionTest,
    /// Request id of the in-flight test, so a late reply from a previous form
    /// cannot overwrite the current one.
    pub test_rid: u64,
}

impl From<&CustomProvider> for CustomProviderForm {
    fn from(provider: &CustomProvider) -> Self {
        Self {
            id: Some(provider.id),
            draft: provider.clone(),
            ..Self::default()
        }
    }
}

impl CustomProviderForm {
    pub fn set_field(&mut self, field: CustomProviderField, value: String) {
        match field {
            CustomProviderField::Name => self.draft.name = value,
            CustomProviderField::ServerUrl => {
                if let Some(http) = self.draft.http_mut() {
                    http.server_url = value;
                }
            }
            CustomProviderField::Username => {
                if let Some(http) = self.draft.http_mut() {
                    http.username = value;
                }
            }
            CustomProviderField::Password => {
                if let Some(http) = self.draft.http_mut() {
                    http.password = value;
                }
            }
            CustomProviderField::Bucket => {
                if let Some(s3) = self.draft.s3_mut() {
                    s3.bucket = value;
                }
            }
            CustomProviderField::Region => {
                if let Some(s3) = self.draft.s3_mut() {
                    s3.region = value;
                }
            }
            CustomProviderField::Endpoint => {
                if let Some(s3) = self.draft.s3_mut() {
                    s3.endpoint = value;
                }
            }
            CustomProviderField::AccessKey => {
                if let Some(s3) = self.draft.s3_mut() {
                    s3.access_key = value;
                }
            }
            CustomProviderField::SecretKey => {
                if let Some(s3) = self.draft.s3_mut() {
                    s3.secret_key = value;
                }
            }
        }
        self.clear_test();
    }

    pub fn set_kind(&mut self, kind: CustomProviderKind) {
        if self.draft.kind() != kind {
            let http = self.draft.http().cloned().unwrap_or_default();
            self.draft.connection = match kind {
                CustomProviderKind::Subsonic => CustomProviderConnection::Subsonic(http),
                CustomProviderKind::Jellyfin => CustomProviderConnection::Jellyfin(http),
                CustomProviderKind::S3 => {
                    let s3 = S3Settings {
                        region: "us-east-1".to_string(),
                        ..S3Settings::default()
                    };
                    CustomProviderConnection::S3(s3)
                }
            };
        }
        self.clear_test();
    }

    pub fn set_path_style(&mut self, path_style: bool) {
        if let Some(s3) = self.draft.s3_mut() {
            s3.path_style = path_style;
        }
        self.clear_test();
    }

    pub fn set_public_bucket(&mut self, public_bucket: bool) {
        if let Some(s3) = self.draft.s3_mut() {
            s3.public_bucket = public_bucket;
        }
        self.clear_test();
    }

    fn clear_test(&mut self) {
        self.test = ConnectionTest::Idle;
        self.test_rid = 0;
    }

    pub fn into_provider(mut self, id: u64) -> CustomProvider {
        self.draft.id = id;
        self.draft.name = self.draft.name.trim().to_string();
        match &mut self.draft.connection {
            CustomProviderConnection::Subsonic(http) | CustomProviderConnection::Jellyfin(http) => {
                http.server_url = http.server_url.trim().to_string();
                http.username = http.username.trim().to_string();
                http.password = http.password.trim().to_string();
            }
            CustomProviderConnection::S3(s3) => {
                s3.endpoint = s3.endpoint.trim().to_string();
                s3.region = s3.region.trim().to_string();
                s3.bucket = s3.bucket.trim().to_string();
                s3.access_key = s3.access_key.trim().to_string();
                s3.secret_key = s3.secret_key.trim().to_string();
            }
        }
        self.draft
    }

    /// `None` when the fields this kind needs are blank.
    pub fn validated(&self) -> Option<()> {
        if self.draft.name.trim().is_empty() {
            return None;
        }
        match &self.draft.connection {
            CustomProviderConnection::S3(s3) => {
                if s3.bucket.trim().is_empty() {
                    return None;
                }
                let credentials = s3.public_bucket
                    || (!s3.access_key.trim().is_empty() && !s3.secret_key.trim().is_empty());
                credentials.then_some(())
            }
            CustomProviderConnection::Subsonic(http) | CustomProviderConnection::Jellyfin(http) => {
                (!http.server_url.trim().is_empty() && !http.username.trim().is_empty())
                    .then_some(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_fields_are_trimmed_and_checked() {
        let mut form = CustomProviderForm::default();
        assert!(form.validated().is_none());
        form.draft.name = "  ".into();
        form.draft.http_mut().unwrap().server_url = "http://localhost:4533".into();
        form.draft.http_mut().unwrap().username = "admin".into();
        assert!(form.validated().is_none());
        form.draft.name = " Home ".into();
        assert!(form.validated().is_some());

        let provider = form.into_provider(7);
        assert_eq!(provider.id, 7);
        assert_eq!(provider.name, "Home");
        assert_eq!(provider.kind(), CustomProviderKind::Subsonic);
        assert_eq!(
            provider.http().map(|http| http.server_url.as_str()),
            Some("http://localhost:4533")
        );
        assert!(provider.s3().is_none());
    }

    #[test]
    fn s3_needs_a_bucket_and_credentials_unless_public() {
        let mut form = CustomProviderForm::default();
        form.set_kind(CustomProviderKind::S3);
        form.draft.name = "Bucket".into();
        assert!(form.validated().is_none());

        form.draft.s3_mut().unwrap().bucket = " music ".into();
        assert!(form.validated().is_none());
        form.draft.s3_mut().unwrap().access_key = "key".into();
        form.draft.s3_mut().unwrap().secret_key = "secret".into();
        assert!(form.validated().is_some());

        form.set_public_bucket(true);
        form.draft.s3_mut().unwrap().access_key.clear();
        form.draft.s3_mut().unwrap().secret_key.clear();
        assert!(form.validated().is_some());

        let provider = form.into_provider(9);
        let s3 = provider.s3().unwrap();
        assert_eq!(s3.bucket, "music");
        assert_eq!(s3.region, "us-east-1");
        assert!(s3.public_bucket);
        assert!(provider.http().is_none());
    }

    #[test]
    fn switching_kind_swaps_the_connection() {
        let mut form = CustomProviderForm::default();
        form.draft.http_mut().unwrap().server_url = "http://home".into();

        form.set_kind(CustomProviderKind::S3);
        assert!(form.draft.http().is_none());
        assert!(form.draft.s3().is_some());

        form.set_kind(CustomProviderKind::Subsonic);
        assert!(form.draft.s3().is_none());
        assert!(form.draft.http().is_some());
    }

    #[test]
    fn switching_between_http_backends_keeps_the_server() {
        let mut form = CustomProviderForm::default();
        form.draft.http_mut().unwrap().server_url = "http://home".into();
        form.set_kind(CustomProviderKind::Jellyfin);
        assert_eq!(
            form.draft.http().map(|http| http.server_url.as_str()),
            Some("http://home")
        );
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
    fn s3_toggles_clear_the_previous_test_result() {
        let mut form = CustomProviderForm {
            test: ConnectionTest::Failed("offline".into()),
            test_rid: 12,
            ..CustomProviderForm::default()
        };
        form.set_path_style(true);
        assert_eq!(form.test, ConnectionTest::Idle);
        form.test = ConnectionTest::Ok("ok".into());
        form.set_public_bucket(true);
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
