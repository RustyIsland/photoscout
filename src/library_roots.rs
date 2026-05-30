use crate::model::LibraryRoot;
use crate::path_utils::canonical_or_original;

pub fn prune_nested_roots(roots: &[LibraryRoot]) -> Vec<LibraryRoot> {
    let mut normalized: Vec<LibraryRoot> = roots
        .iter()
        .filter(|root| root.enabled)
        .cloned()
        .map(|mut root| {
            root.path = canonical_or_original(&root.path);
            root
        })
        .collect();

    normalized.sort_by_key(|root| root.path.components().count());

    let mut kept: Vec<LibraryRoot> = Vec::new();
    'outer: for root in normalized {
        for existing in &kept {
            if root.path.starts_with(&existing.path) {
                tracing::debug!("skipping nested library root");
                continue 'outer;
            }
        }
        kept.push(root);
    }

    kept
}
