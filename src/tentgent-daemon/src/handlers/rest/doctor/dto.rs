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
}
