use super::*;
use std::collections::{BTreeMap, HashSet};

/// Plan addon-declared ID catalogs in recency order, then sort IDs for cacheable URLs.
pub(crate) fn id_catalog_requests<'a>(
    snapshot: &'a Snapshot,
    keys: &[ItemKey],
    extra: &str,
) -> Vec<(&'a Installation, ResourceRequest)> {
    let mut requests = Vec::new();
    for addon in snapshot.addons.iter().filter(|a| a.enabled) {
        for catalog in &addon.manifest.catalogs {
            let Some(property) = catalog.extra.iter().find(|p| p.name == extra) else {
                continue;
            };
            let mut seen = HashSet::new();
            let mut ids: Vec<_> = keys
                .iter()
                .filter(|key| {
                    key.content_type == catalog.content_type
                        && addon.manifest.types.contains(&key.content_type)
                        && addon.manifest.id_prefixes.as_ref().is_none_or(|prefixes| {
                            prefixes.is_empty()
                                || prefixes.iter().any(|p| key.item_id.starts_with(p))
                        })
                        && seen.insert(key.item_id.clone())
                })
                .take(property.options_limit.unwrap_or(1).min(100))
                .map(|key| key.item_id.clone())
                .collect();
            if ids.is_empty() {
                continue;
            }
            ids.sort();
            let request = ResourceRequest {
                resource: Resource::Catalog,
                content_type: catalog.content_type.clone(),
                id: catalog.id.clone(),
                extra: BTreeMap::from([(extra.into(), ids.join(","))]),
            };
            if addon.manifest.supports(&request) {
                requests.push((addon, request));
            }
        }
    }
    requests
}

#[derive(Clone, Default)]
pub struct CalendarData {
    pub titles: Vec<(ItemKey, ResolvedMetadata)>,
    pub supported: bool,
    pub warnings: Vec<String>,
}
impl Core {
    /// The calendar follows titles explicitly saved to the profile's library.
    /// Results are retained by the UI while navigating months, not re-fetched per day.
    pub async fn calendar(&self) -> Result<CalendarData> {
        let snapshot = self.storage.load().await?;
        let keys: Vec<_> = snapshot
            .library
            .iter()
            .rev()
            .map(|item| item.key.clone())
            .collect();
        let mut data = CalendarData {
            supported: snapshot.addons.iter().any(|a| {
                a.enabled
                    && a.manifest
                        .catalogs
                        .iter()
                        .any(|c| c.extra.iter().any(|e| e.name == "calendarVideosIds"))
            }),
            ..Default::default()
        };
        for (addon, request) in id_catalog_requests(&snapshot, &keys, "calendarVideosIds") {
            match self.query_addon(addon, &request).await {
                Ok(ResourceData::Catalog(metas)) => {
                    for meta in metas {
                        let Some(key) = keys.iter().find(|key| {
                            key.item_id == meta.id && key.content_type == meta.content_type
                        }) else {
                            continue;
                        };
                        if let Some((_, existing)) =
                            data.titles.iter_mut().find(|(found, _)| found == key)
                        {
                            for video in &meta.videos {
                                if !existing.meta.videos.iter().any(|v| v.id == video.id) {
                                    existing.meta.videos.push(video.clone());
                                }
                            }
                            metadata::fill_missing(
                                &mut existing.meta,
                                meta,
                                &addon.installation_id,
                                &mut existing.field_providers,
                            );
                        } else {
                            data.titles.push((
                                key.clone(),
                                ResolvedMetadata {
                                    field_providers: meta
                                        .extra
                                        .keys()
                                        .map(|field| (field.clone(), addon.installation_id.clone()))
                                        .collect(),
                                    meta,
                                },
                            ));
                        }
                    }
                }
                Err(error) => data
                    .warnings
                    .push(format!("{}: {:?}", addon.manifest.name, error.code)),
                _ => (),
            }
        }
        Ok(data)
    }
}
