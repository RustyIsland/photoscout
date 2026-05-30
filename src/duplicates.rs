use crate::model::{PhotoId, PhotoRecord};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default, Clone)]
pub struct DuplicateIndex {
    groups_by_hash: HashMap<String, Vec<PhotoId>>,
    duplicate_ids: HashSet<PhotoId>,
}

impl DuplicateIndex {
    pub fn rebuild(photos: &[PhotoRecord]) -> Self {
        let mut groups_by_hash: HashMap<String, Vec<PhotoId>> = HashMap::new();

        for photo in photos {
            if let Some(hash) = &photo.content_hash {
                groups_by_hash.entry(hash.clone()).or_default().push(photo.id);
            }
        }

        groups_by_hash.retain(|_, ids| ids.len() > 1);

        let duplicate_ids = groups_by_hash
            .values()
            .flat_map(|ids| ids.iter().copied())
            .collect();

        Self {
            groups_by_hash,
            duplicate_ids,
        }
    }

    pub fn duplicate_group_count(&self) -> usize {
        self.groups_by_hash.len()
    }

    pub fn duplicate_photo_count(&self) -> usize {
        self.duplicate_ids.len()
    }

    pub fn is_duplicate(&self, photo_id: PhotoId) -> bool {
        self.duplicate_ids.contains(&photo_id)
    }

    pub fn group_for(&self, photo: &PhotoRecord) -> Option<&Vec<PhotoId>> {
        let hash = photo.content_hash.as_ref()?;
        self.groups_by_hash.get(hash)
    }

    pub fn groups(&self) -> impl Iterator<Item = &Vec<PhotoId>> {
        self.groups_by_hash.values()
    }
}
