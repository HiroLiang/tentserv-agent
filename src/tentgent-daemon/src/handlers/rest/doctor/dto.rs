use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DoctorResponse {
    pub status: String,
    pub summary: DoctorSummaryItem,
    pub checks: Vec<DoctorCheckItem>,
}

#[derive(Debug, Serialize)]
pub struct DoctorSummaryItem {
    pub pass: usize,
    pub warn: usize,
    pub fail: usize,
    pub skipped: usize,
}

#[derive(Debug, Serialize)]
pub struct DoctorCheckItem {
    pub name: String,
    pub category: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Deprecated legacy readable message field. Prefer `description` for new
    /// clients when present.
    pub detail: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<DoctorCheckDetailItem>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<DoctorNextActionItem>,
}

#[derive(Debug, Serialize)]
pub struct DoctorCheckDetailItem {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DoctorNextActionItem {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}
