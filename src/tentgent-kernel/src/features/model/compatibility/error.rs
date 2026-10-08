/// Validation errors contain fixed field names/codes, never rejected payloads.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CompatibilityError {
    #[error("invalid compatibility field: {0}")]
    InvalidField(&'static str),
    #[error("unsupported compatibility identity version: {0}")]
    UnsupportedIdentity(u16),
    #[error("unsupported compatibility proof schema: {0}")]
    UnsupportedSchema(u16),
    #[error("compatibility proof location does not match its tuple")]
    LocationMismatch,
    #[error("compatibility tuple could not be canonically serialized")]
    CanonicalSerialization,
    #[error("compatibility proof source cannot establish this observation")]
    InvalidObservationSource,
}
