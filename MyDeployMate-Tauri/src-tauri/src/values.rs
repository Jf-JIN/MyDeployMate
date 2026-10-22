//! Where variable values that are not in the configuration come from on this computer.

use mdm_core::{EnvironmentId, ValueSource, VariableId};

/// Environment variables of this process. Saved credentials join in phase 5; until then a
/// sensitive variable stored there resolves as "需填写".
pub struct ProcessValues;

impl ValueSource for ProcessValues {
    fn env_var(&self, name: &str) -> Option<String> {
        if name.is_empty() || name.contains(['=', '\0']) {
            return None;
        }
        std::env::var(name).ok()
    }

    fn secret(&self, _variable: VariableId, _environment: Option<EnvironmentId>) -> Option<String> {
        None
    }
}
