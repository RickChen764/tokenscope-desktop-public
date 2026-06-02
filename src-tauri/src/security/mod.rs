#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SecretRef {
    value: String,
}

#[allow(dead_code)]
impl SecretRef {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}
