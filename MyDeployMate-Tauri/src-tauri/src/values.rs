//! Where variable values that are not in the configuration come from on this computer.

use mdm_core::{EnvironmentId, ValueSource, VariableId};
use mdm_store::SecretStore;
use secrecy::ExposeSecret;

/// Environment variables of this process and sensitive values in Credential Manager (ARC-10).
/// Without a secret store every sensitive variable resolves as "需填写".
pub struct ProcessValues<'a> {
    pub secrets: Option<&'a SecretStore>,
}

impl ValueSource for ProcessValues<'_> {
    fn env_var(&self, name: &str) -> Option<String> {
        if name.is_empty() || name.contains(['=', '\0']) {
            return None;
        }
        std::env::var(name).ok()
    }

    fn secret(&self, variable: VariableId, environment: Option<EnvironmentId>) -> Option<String> {
        let value = self.secrets?.variable(variable, environment).ok()??;
        Some(value.expose_secret().to_owned())
    }
}
