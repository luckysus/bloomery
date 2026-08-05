pub const LOCAL_WORKSPACE_ID: &str = "local";

#[derive(Debug, Default)]
pub struct LocalIdentity;

impl LocalIdentity {
    pub fn workspace_id(&self) -> &'static str {
        LOCAL_WORKSPACE_ID
    }

    #[cfg(test)]
    pub fn credential(&self) -> Option<&str> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::LocalIdentity;

    #[test]
    fn local_identity_is_stable() {
        assert_eq!(LocalIdentity.workspace_id(), "local");
    }

    #[test]
    fn local_identity_has_no_token() {
        assert_eq!(LocalIdentity.credential(), None);
    }
}
