use std::collections::BTreeMap;

use super::manifest::ManifestDocument;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatasetDiffStatus {
    Added,
    Removed,
    Modified,
    Unchanged,
}

impl DatasetDiffStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Modified => "modified",
            Self::Unchanged => "unchanged",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DatasetDiffFile {
    pub status: DatasetDiffStatus,
    pub relative_path: String,
    pub left_size_bytes: Option<u64>,
    pub right_size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct DatasetDiffSummary {
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
    pub unchanged: usize,
    pub left_total_bytes: u64,
    pub right_total_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct DatasetManifestDiff {
    pub summary: DatasetDiffSummary,
    pub files: Vec<DatasetDiffFile>,
}

pub fn diff_manifests(left: &ManifestDocument, right: &ManifestDocument) -> DatasetManifestDiff {
    let left_by_path = left
        .files
        .iter()
        .map(|entry| (entry.relative_path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let right_by_path = right
        .files
        .iter()
        .map(|entry| (entry.relative_path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let mut paths = left_by_path.keys().copied().collect::<Vec<_>>();

    for path in right_by_path.keys() {
        if !left_by_path.contains_key(path) {
            paths.push(path);
        }
    }

    paths.sort_unstable();

    let mut summary = DatasetDiffSummary {
        left_total_bytes: left.total_bytes(),
        right_total_bytes: right.total_bytes(),
        ..DatasetDiffSummary::default()
    };
    let mut files = Vec::new();

    for path in paths {
        let left_entry = left_by_path.get(path).copied();
        let right_entry = right_by_path.get(path).copied();
        let status = match (left_entry, right_entry) {
            (None, Some(_)) => DatasetDiffStatus::Added,
            (Some(_), None) => DatasetDiffStatus::Removed,
            (Some(left_entry), Some(right_entry))
                if left_entry.sha256 == right_entry.sha256
                    && left_entry.size_bytes == right_entry.size_bytes =>
            {
                DatasetDiffStatus::Unchanged
            }
            (Some(_), Some(_)) => DatasetDiffStatus::Modified,
            (None, None) => continue,
        };

        match status {
            DatasetDiffStatus::Added => summary.added += 1,
            DatasetDiffStatus::Removed => summary.removed += 1,
            DatasetDiffStatus::Modified => summary.modified += 1,
            DatasetDiffStatus::Unchanged => summary.unchanged += 1,
        }

        files.push(DatasetDiffFile {
            status,
            relative_path: path.to_string(),
            left_size_bytes: left_entry.map(|entry| entry.size_bytes),
            right_size_bytes: right_entry.map(|entry| entry.size_bytes),
        });
    }

    DatasetManifestDiff { summary, files }
}
