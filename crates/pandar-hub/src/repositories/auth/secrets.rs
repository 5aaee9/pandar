pub(crate) fn generate_secret(prefix: &str) -> String {
    format!(
        "{prefix}{}_{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
