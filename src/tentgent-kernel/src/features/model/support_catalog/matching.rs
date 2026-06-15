//! Source identity matching for support catalog entries.

use super::super::domain::ModelMetadata;
use super::domain::ModelSupportCatalogEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ModelCatalogMatchKind {
    None,
    Pattern,
    Exact,
}

pub(super) fn entry_match_kind(
    entry: &ModelSupportCatalogEntry,
    metadata: &ModelMetadata,
) -> ModelCatalogMatchKind {
    if entry.source_kind != metadata.source_kind {
        return ModelCatalogMatchKind::None;
    }

    let Some(source_repo) = metadata.source_repo.as_deref() else {
        return ModelCatalogMatchKind::None;
    };

    if entry
        .source_repos
        .iter()
        .any(|repo| normalized_eq(repo, source_repo))
    {
        return ModelCatalogMatchKind::Exact;
    }

    if entry
        .source_repo_patterns
        .iter()
        .any(|pattern| wildcard_matches(pattern, source_repo))
    {
        return ModelCatalogMatchKind::Pattern;
    }

    ModelCatalogMatchKind::None
}

fn normalized_eq(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    let value = value.to_ascii_lowercase();

    if pattern == "*" {
        return true;
    }

    if !pattern.contains('*') {
        return pattern == value;
    }

    let mut remainder = value.as_str();
    let mut parts = pattern.split('*').peekable();
    let starts_with_wildcard = pattern.starts_with('*');
    let ends_with_wildcard = pattern.ends_with('*');

    if let Some(first) = parts.next() {
        if !first.is_empty() {
            if !remainder.starts_with(first) {
                return false;
            }
            remainder = &remainder[first.len()..];
        } else if !starts_with_wildcard {
            return false;
        }
    }

    let mut last_part = "";
    for part in parts {
        if part.is_empty() {
            continue;
        }

        let Some(index) = remainder.find(part) else {
            return false;
        };
        remainder = &remainder[index + part.len()..];
        last_part = part;
    }

    ends_with_wildcard || last_part.is_empty() || remainder.is_empty()
}
