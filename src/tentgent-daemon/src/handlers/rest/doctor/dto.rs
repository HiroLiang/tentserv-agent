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
    pub status: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<DoctorNextActionItem>,
}

#[derive(Debug, Serialize)]
pub struct DoctorNextActionItem {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}
