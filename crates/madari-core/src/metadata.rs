use super::*;

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
pub struct ResolvedMetadata {
    pub meta: Meta,
    pub field_providers: std::collections::BTreeMap<String, String>,
}
impl Core {
    pub async fn metadata(&self, key: &ItemKey, preview: Option<Meta>) -> Result<Meta> {
        Ok(self.resolve_metadata(key, preview).await?.meta)
    }

    /// Query every enabled provider declaring support for this item. Prefer the
    /// originating provider, then installation order. Only matching identities
    /// contribute missing fields; conflicting populated values are not overwritten.
    pub async fn resolve_metadata(
        &self,
        key: &ItemKey,
        preview: Option<Meta>,
    ) -> Result<ResolvedMetadata> {
        Ok(self.resolve_metadata_inner(key, preview, false).await?.1)
    }

    /// Resolve an external identity through a real addon response. A poster
    /// preview or old preview-only cache is not proof that an addon matched.
    pub async fn resolve_addon_metadata(
        &self,
        key: &ItemKey,
        preview: Option<Meta>,
    ) -> Result<(ItemKey, ResolvedMetadata)> {
        self.resolve_metadata_inner(key, preview, true).await
    }

    async fn resolve_metadata_inner(
        &self,
        key: &ItemKey,
        preview: Option<Meta>,
        require_addon: bool,
    ) -> Result<(ItemKey, ResolvedMetadata)> {
        let request = ResourceRequest {
            resource: Resource::Meta,
            content_type: key.content_type.clone(),
            id: key.item_id.clone(),
            extra: Default::default(),
        };
        let results = self.query_all(request).await?;
        let mut matching: Vec<_> = results
            .into_iter()
            .filter_map(|r| match r.result {
                Ok(ResourceData::Meta(Some(meta)))
                    if meta.id == key.item_id && meta.content_type == key.content_type =>
                {
                    Some((r.installation_id, meta))
                }
                _ => None,
            })
            .collect();
        matching.sort_by_key(|(id, _)| id != &key.installation_id);
        if require_addon && matching.is_empty() {
            return Err(Error::new(
                ErrorCode::NotFound,
                format!(
                    "No enabled metadata addon returned {} details for {}.",
                    key.content_type, key.item_id
                ),
            ));
        }
        let resolved_key = if require_addon {
            ItemKey {
                installation_id: matching[0].0.clone(),
                ..key.clone()
            }
        } else {
            key.clone()
        };
        let key = &resolved_key;

        let snapshot = self.storage.load().await?;
        let sources: Vec<_> = snapshot
            .addons
            .iter()
            .filter(|a| a.enabled)
            .map(|a| {
                (
                    a.installation_id.clone(),
                    a.manifest_url.clone(),
                    a.allow_local,
                )
            })
            .collect();
        let cached = self
            .storage
            .cached_metadata()
            .await
            .unwrap_or_default()
            .into_iter()
            .find(|c| c.key == *key && c.sources == sources);
        let mut metas = matching;
        if let Some(preview) =
            preview.filter(|m| m.id == key.item_id && m.content_type == key.content_type)
        {
            metas.push((key.installation_id.clone(), preview));
        }
        if metas.is_empty()
            && let Some(cached) = &cached
        {
            return Ok((key.clone(), cached.resolved.clone()));
        }
        let mut iter = metas.into_iter();
        let (provider, mut selected) = iter.next().ok_or_else(|| {
            Error::new(
                ErrorCode::NotFound,
                "None of the enabled addons returned details for this title.",
            )
        })?;
        let mut field_providers = selected
            .extra
            .keys()
            .map(|k| (k.clone(), provider.clone()))
            .collect();
        for (provider, fallback) in iter {
            fill_missing(&mut selected, fallback, &provider, &mut field_providers);
        }
        if let Some(cached) = &cached {
            let from_cache: Vec<_> = cached
                .resolved
                .meta
                .extra
                .keys()
                .filter(|field| selected.extra.get(*field).is_none_or(empty))
                .cloned()
                .collect();
            fill_missing(
                &mut selected,
                cached.resolved.meta.clone(),
                &key.installation_id,
                &mut field_providers,
            );
            for field in from_cache {
                if let Some(provider) = cached.resolved.field_providers.get(&field) {
                    field_providers.insert(field, provider.clone());
                }
            }
        }
        let resolved = ResolvedMetadata {
            meta: selected,
            field_providers,
        };
        let _ = self
            .storage
            .cache_metadata(vec![crate::CachedMetadata {
                key: key.clone(),
                resolved: resolved.clone(),
                sources,
                fetched_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            }])
            .await;
        Ok((key.clone(), resolved))
    }
}
fn empty(value: &serde_json::Value) -> bool {
    value.is_null()
        || value.as_str().is_some_and(|v| v.trim().is_empty())
        || value.as_array().is_some_and(Vec::is_empty)
        || value.as_object().is_some_and(|v| v.is_empty())
}
pub(super) fn fill_missing(
    selected: &mut Meta,
    fallback: Meta,
    provider: &str,
    providers: &mut std::collections::BTreeMap<String, String>,
) {
    if selected.name.trim().is_empty() {
        selected.name = fallback.name;
    }
    if selected.videos.is_empty() {
        selected.videos = fallback.videos;
    }
    for (key, value) in fallback.extra {
        if !empty(&value) && selected.extra.get(&key).is_none_or(empty) {
            providers.insert(key.clone(), provider.into());
            selected.extra.insert(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct FixtureHttp;
    #[async_trait]
    impl Http for FixtureHttp {
        async fn get_json(&self, url: Url, _: bool) -> Result<serde_json::Value> {
            match url.host_str().unwrap() {
                "broken.example" => Err(Error::new(ErrorCode::Network, "unavailable")),
                "catalog.example" => Ok(
                    serde_json::json!({"meta":{"id":"tt1","type":"movie","name":"Primary","poster":"","description":"Primary description"}}),
                ),
                "art.example" => Ok(
                    serde_json::json!({"meta":{"id":"tt1","type":"movie","name":"Other title","poster":"https://img.example/poster.jpg","background":"https://img.example/back.jpg","description":"Conflicting description"}}),
                ),
                _ => Ok(
                    serde_json::json!({"meta":{"id":"wrong-id","type":"movie","name":"Wrong","cast":["Must not merge"]}}),
                ),
            }
        }
    }
    struct FixtureStore(Snapshot);
    #[async_trait]
    impl Storage for FixtureStore {
        async fn load(&self) -> Result<Snapshot> {
            Ok(self.0.clone())
        }
        async fn compare_and_swap(&self, _: u64, _: Snapshot) -> Result<()> {
            unreachable!()
        }
    }
    #[tokio::test]
    async fn all_supported_providers_fill_missing_metadata_without_overwriting_or_crossing_ids() {
        let manifest:Manifest=serde_json::from_value(serde_json::json!({"id":"fixture","name":"Fixture","version":"1","resources":["meta"],"types":["movie"],"catalogs":[]})).unwrap();
        let addons = ["broken", "art", "catalog", "wrong"]
            .into_iter()
            .map(|id| Installation {
                installation_id: id.into(),
                manifest_url: format!("https://{id}.example/manifest.json")
                    .parse()
                    .unwrap(),
                manifest: manifest.clone(),
                enabled: true,
                allow_local: false,
            })
            .collect();
        let core = Core::new(
            Arc::new(FixtureHttp),
            Arc::new(FixtureStore(Snapshot {
                addons,
                ..Default::default()
            })),
        );
        let key = ItemKey {
            installation_id: "catalog".into(),
            content_type: "movie".into(),
            item_id: "tt1".into(),
        };
        let meta = core.metadata(&key, None).await.unwrap();
        assert_eq!(meta.name, "Primary");
        assert_eq!(meta.extra["description"], "Primary description");
        assert_eq!(meta.extra["poster"], "https://img.example/poster.jpg");
        assert!(meta.extra.contains_key("background"));
        assert!(!meta.extra.contains_key("cast"));
        let key = ItemKey {
            installation_id: "broken".into(),
            ..key
        };
        assert_eq!(core.metadata(&key, None).await.unwrap().name, "Other title");
    }
}
