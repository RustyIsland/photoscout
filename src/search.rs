use crate::duplicates::DuplicateIndex;
use crate::model::{PhotoRecord, SearchQuery, SortMode};

pub fn filter_indices(
    photos: &[PhotoRecord],
    duplicates: &DuplicateIndex,
    query: &SearchQuery,
) -> Vec<usize> {
    let needle = query.text.trim().to_ascii_lowercase();

    let mut indices: Vec<usize> = photos
        .iter()
        .enumerate()
        .filter(|(_, photo)| {
            if let Some(root_id) = query.root_filter {
                if photo.root_id != root_id {
                    return false;
                }
            }

            if query.duplicates_only && !duplicates.is_duplicate(photo.id) {
                return false;
            }

            if needle.is_empty() {
                return true;
            }

            let path_text = photo.path.display().to_string().to_ascii_lowercase();
            let file_text = photo.file_name.to_ascii_lowercase();
            file_text.contains(&needle) || path_text.contains(&needle)
        })
        .map(|(index, _)| index)
        .collect();

    match query.sort_mode {
        SortMode::NameAsc => indices.sort_by_key(|&index| photos[index].file_name.to_ascii_lowercase()),
        SortMode::SizeLargest => indices.sort_by_key(|&index| std::cmp::Reverse(photos[index].size_bytes)),
        SortMode::RootThenName => indices.sort_by_key(|&index| {
            (
                photos[index].root_id.0,
                photos[index].relative_path.display().to_string().to_ascii_lowercase(),
            )
        }),
    }

    indices
}
