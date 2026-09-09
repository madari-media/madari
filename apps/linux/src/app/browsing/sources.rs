use super::*;
use madari_addon::Catalog;
use madari_model::{
    Error, ErrorCode, ItemKey, Meta, PlaybackDelivery, PlayerCapabilities, PreparePlaybackRequest,
    Resource, ResourceData, ResourceRequest, Stream,
};
use madari_native::companion::{Companion, NoCompanion};
use std::collections::BTreeMap;

impl Ui {
    fn page(self: &Rc<Self>, title: &str) {
        self.clear();
        self.content.append(&self.back_button());
        self.content.append(&label(title, "title-1"));
    }
    pub(in crate::app) fn catalog_options(self: &Rc<Self>, id: String, catalog: Catalog) {
        self.catalog(id, catalog, BTreeMap::new());
    }
    pub(in crate::app) fn catalog_filters(
        self: &Rc<Self>,
        id: String,
        catalog: Catalog,
        search: Option<String>,
    ) {
        let extras: Vec<_> = catalog
            .extra
            .iter()
            .filter(|e| e.name != "skip")
            .cloned()
            .collect();
        if extras.is_empty() {
            self.catalog(id, catalog, BTreeMap::new());
            return;
        }
        let fields = form();
        let mut inputs = Vec::new();
        for extra in extras {
            fields.append(&label(
                &format!(
                    "{}{}",
                    extra.name,
                    if extra.is_required { " (required)" } else { "" }
                ),
                "heading",
            ));
            let field = entry(&extra.name, false);
            if extra.name == "search"
                && let Some(search) = &search
            {
                field.set_text(search);
            }
            if let Some(options) = &extra.options {
                fields.append(&label(
                    &format!("Choices: {}", options.join(", ")),
                    "dim-label",
                ));
            }
            fields.append(&field);
            inputs.push((extra, field));
        }
        self.dialog(
            "Catalog filters",
            "Search and filters are supplied by this addon. Leave optional fields empty to browse.",
            &fields,
            "Browse",
            move |ui| {
                let mut values = BTreeMap::new();
                for (extra, field) in &inputs {
                    let value = field.text().trim().to_string();
                    if (extra.is_required && value.is_empty())
                        || (!value.is_empty()
                            && extra.options.as_ref().is_some_and(|o| !o.contains(&value)))
                    {
                        ui.toast.add_toast(adw::Toast::new(&format!(
                            "Enter a valid value for {}.",
                            extra.name
                        )));
                        return;
                    }
                    if !value.is_empty() {
                        values.insert(extra.name.clone(), value);
                    }
                }
                ui.catalog(id.clone(), catalog.clone(), values);
            },
        );
    }
    fn catalog(self: &Rc<Self>, id: String, catalog: Catalog, extra: BTreeMap<String, String>) {
        self.page(catalog.name.as_deref().unwrap_or(&catalog.id));
        let filter_id = id.clone();
        let filter_catalog = catalog.clone();
        if catalog.extra.iter().any(|e| e.name != "skip") {
            self.content.append(&self.button("Filters", move |ui| {
                ui.catalog_filters(filter_id.clone(), filter_catalog.clone(), None)
            }));
        }
        if catalog
            .extra
            .iter()
            .any(|e| e.is_required && e.name != "skip" && !extra.contains_key(&e.name))
        {
            self.content.append(&label(
                "Choose the required filters to load this collection.",
                "dim-label",
            ));
            return;
        }
        let section = form();
        self.content.append(&section);
        self.catalog_grid(&section, id, catalog, extra);
    }
    pub(in crate::app) fn details(self: &Rc<Self>, key: ItemKey, preview: Option<Meta>) {
        let core = self.core.borrow().as_ref().unwrap().clone();
        let lookup = key.clone();
        let profiles = self.profiles.clone();
        let session = self.session.borrow().as_ref().unwrap().clone();
        self.loading_page(true);
        let retry_key = key.clone();
        let retry_preview = preview.clone();
        *self.page_retry.borrow_mut() = Some(Rc::new(move |ui| {
            ui.details(retry_key.clone(), retry_preview.clone());
        }));
        self.run(
            async move {
                let meta = core.resolve_metadata(&lookup, preview).await?;
                profiles
                    .apply_trakt_history(session, lookup.clone(), meta.meta.clone())
                    .await?;
                let snapshot = core.snapshot().await?;
                let saved = snapshot.library.iter().any(|i| i.key == lookup);
                Ok((meta, saved, snapshot.progress))
            },
            move |ui, (resolved, saved, progress)| {
                ui.remember_metadata(&resolved);
                ui.detail_view(key, resolved.meta, saved, progress)
            },
        );
    }

    pub(in crate::app) fn sources(self: &Rc<Self>, key: ItemKey, video: String) {
        let core = self.core.borrow().as_ref().unwrap().clone();
        let request = ResourceRequest {
            resource: Resource::Stream,
            content_type: key.content_type.clone(),
            id: video.clone(),
            extra: BTreeMap::new(),
        };
        let lookup = key.clone();
        let preview = self
            .playback_metadata
            .borrow()
            .get(&(key.content_type.clone(), key.item_id.clone()))
            .cloned();
        self.run(
            async move {
                let resolved = if preview.is_none() {
                    core.resolve_metadata(&lookup, None).await.ok()
                } else {
                    None
                };
                Ok((
                    core.query_all(request).await?,
                    core.snapshot().await?,
                    resolved,
                ))
            },
            move |ui, (results, snapshot, resolved)| {
                if let Some(resolved) = resolved {
                    ui.remember_metadata(&resolved);
                }
                ui.show_sources(key, video, results, snapshot);
            },
        );
    }
    fn show_sources(
        self: &Rc<Self>,
        key: ItemKey,
        video: String,
        results: Vec<madari_model::ProviderResult>,
        snapshot: PublicSnapshot,
    ) {
        self.page("Choose a source");
        let retry_key = key.clone();
        let retry_video = video.clone();
        self.content
            .append(&self.button("Reload sources", move |ui| {
                ui.sources(retry_key.clone(), retry_video.clone());
            }));
        let filters = form();
        self.content.append(&filters);
        let mut groups = Vec::new();
        let mut count = 0;
        for result in results {
            let provider = snapshot
                .addons
                .iter()
                .find(|a| a.installation_id == result.installation_id)
                .map(|a| a.manifest.name.as_str())
                .unwrap_or("Addon");
            let group = form();
            let mut group_count = 0;
            match result.result {
                Ok(ResourceData::Stream(streams)) => {
                    if streams.is_empty() {
                        continue;
                    }
                    group.append(&label(provider, "title-2"));
                    group_count = streams.len();
                    let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
                    list.add_css_class("source-list");
                    for stream in streams {
                        count += 1;
                        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                        let s = stream.clone();
                        let k = key.clone();
                        let v = video.clone();
                        let source_provider = result.installation_id.clone();
                        let play = self.button("Play source", move |ui| {
                            ui.start_source(
                                k.clone(),
                                v.clone(),
                                s.clone(),
                                source_provider.clone(),
                                None,
                                false,
                            );
                        });
                        play.remove_css_class("compact-action");
                        play.add_css_class("source-card");
                        play.set_halign(gtk::Align::Fill);
                        play.set_hexpand(true);
                        play.set_child(Some(&crate::app::browsing::stream_list::content(
                            &stream, false,
                        )));
                        row.append(&play);
                        if self
                            .companion
                            .borrow()
                            .as_ref()
                            .is_some_and(|c| !c.is_internal())
                            && (stream.info_hash.is_some()
                                || stream
                                    .url
                                    .as_ref()
                                    .is_some_and(|u| u.starts_with("magnet:")))
                        {
                            let s = stream.clone();
                            let k = key.clone();
                            let v = video.clone();
                            let source_provider = result.installation_id.clone();
                            let transcode = self.button("Transcode", move |ui| {
                                ui.start_source(
                                    k.clone(),
                                    v.clone(),
                                    s.clone(),
                                    source_provider.clone(),
                                    None,
                                    true,
                                );
                            });
                            transcode.set_icon_name("applications-multimedia-symbolic");
                            transcode.set_tooltip_text(Some("Play with FFmpeg transcoding"));
                            row.append(&transcode);
                        }
                        list.append(&row);
                    }
                    group.append(&list);
                }
                Err(e) if e.code != ErrorCode::UnsupportedResource => {
                    group.append(&label(&format!("{provider}: {}", e.message), "dim-label"))
                }
                _ => (),
            }
            if group.first_child().is_some() {
                self.content.append(&group);
                groups.push((
                    result.installation_id,
                    provider.to_owned(),
                    group_count,
                    group.upcast(),
                ));
            }
        }
        if !groups.is_empty() {
            filters.append(&crate::app::browsing::stream_list::addon_filter(groups));
        }
        if count == 0 {
            self.content.append(&label(
                "No playable sources were returned. Check your profile’s addons.",
                "body",
            ));
        }
    }
    pub(in crate::app) fn companion_settings(
        self: &Rc<Self>,
        status: glib::WeakRef<adw::ActionRow>,
    ) {
        let fields = form();
        let mode = gtk::DropDown::from_strings(&["Built-in", "External server"]);
        let external = self
            .companion
            .borrow()
            .as_ref()
            .is_some_and(|c| !c.is_internal());
        mode.set_selected(u32::from(external));
        mode.update_property(&[gtk::accessible::Property::Label("Media server mode")]);
        fields.append(&mode);
        let origin = entry("http://localhost:11470", false);
        origin.set_text(
            self.companion
                .borrow()
                .as_ref()
                .filter(|c| !c.is_internal())
                .map_or("http://localhost:11470", |c| c.origin()),
        );
        let key = entry("Companion API key", false);
        key.set_visibility(false);
        key.set_max_length(1024);
        let connection = form();
        connection.append(&origin);
        connection.append(&key);
        connection.set_visible(external);
        let weak = connection.downgrade();
        mode.connect_selected_notify(move |mode| {
            if let Some(connection) = weak.upgrade() {
                connection.set_visible(mode.selected() == 1);
            }
        });
        fields.append(&connection);
        self.dialog("Media server","Built-in playback runs inside the app with no listening ports. To use an external server, enter its address and API key below. External credentials last for this session.",&fields,"Save",move |ui|{
            if mode.selected() == 0 {
                *ui.companion.borrow_mut() = Some(ui.internal_companion.clone());
                if let Some(row) = status.upgrade() { row.set_subtitle(ui.internal_companion.origin()); }
                ui.toast.add_toast(adw::Toast::new("Using built-in media playback."));
                return;
            }
            let status = status.clone();
            let result=Companion::new(origin.text().trim(),key.text().to_string());
            match result {
                Ok(companion)=>ui.run(async move {companion.check().await?;Ok(companion)},move |ui,c|{if let Some(row) = status.upgrade() { row.set_subtitle(c.origin()); } *ui.companion.borrow_mut()=Some(c);ui.toast.add_toast(adw::Toast::new("Companion connected."));}),
                Err(e)=>ui.toast.add_toast(adw::Toast::new(&e.message)),
            }
        });
    }
    pub(in crate::app) fn start_source(
        self: &Rc<Self>,
        key: ItemKey,
        video: String,
        source: Stream,
        provider: String,
        previous: Option<crate::app::playback::PlaybackContext>,
        transcode: bool,
    ) {
        if self.playback_active.get() {
            *self.after_playback.borrow_mut() = Some(Box::new(move |ui| {
                ui.start_source(key, video, source, provider, previous, transcode);
            }));
            if let Some(stop) = self.player_stop.borrow_mut().take() {
                let _ = stop.send(());
            }
            return;
        }
        let mut context = previous.unwrap_or_default();
        context.group = madari_core::binge_group(&source).map(str::to_string);
        context.provider = provider.clone();
        let transcode = transcode
            && self
                .companion
                .borrow()
                .as_ref()
                .is_some_and(|c| !c.is_internal());
        context.transcode = transcode;
        context.current_source = crate::app::playback::fingerprint(&provider, &source);
        context
            .attempted
            .insert(crate::app::playback::fingerprint(&provider, &source));
        if let Some(meta) = self
            .playback_metadata
            .borrow()
            .get(&(key.content_type.clone(), key.item_id.clone()))
        {
            context.episodes = meta.videos.clone();
            context.metadata = Some(meta.preview());
        }
        let title = self
            .playback_titles
            .borrow()
            .get(&(key.content_type.clone(), key.item_id.clone()))
            .cloned()
            .unwrap_or_else(|| "Now playing".into());
        let title = context
            .episodes
            .iter()
            .find(|episode| episode.id == video)
            .map(|episode| {
                format!(
                    "{title} · {}",
                    crate::app::browsing::episodes::episode_title(episode)
                )
            })
            .unwrap_or(title);
        let core = self.core.borrow().as_ref().unwrap().clone();
        let companion = self.companion.borrow().clone();
        let k = key.clone();
        let v = video.clone();
        let retry_context = context.clone();
        let preparation = async move {
            let snapshot = core.snapshot().await?;
            context.progress = snapshot.progress;
            context.preferences = snapshot.playback_preferences;
            let capabilities = PlayerCapabilities {
                url_schemes: vec!["http".into(), "https".into()],
                companion: companion.is_some(),
                request_headers: true,
                ..Default::default()
            };
            let request = PreparePlaybackRequest {
                source,
                capabilities,
                key: Some(k.clone()),
                video_id: Some(v),
                file_index: None,
            };
            let prepared = if let Some(c) = &companion {
                core.prepare_playback(request, c).await?
            } else {
                core.prepare_playback(request, &NoCompanion).await?
            };
            let mut token = None;
            let (url, headers, resume_ms, offset_ms) = match prepared.delivery {
                PlaybackDelivery::Direct {
                    url,
                    request_headers,
                } => (url, request_headers, prepared.plan.resume_ms, 0),
                PlaybackDelivery::Torrent { media, .. } => {
                    token = Some(media.token.clone());
                    let c = companion.as_ref().unwrap();
                    if transcode && !c.is_internal() {
                        (
                            c.media_url(&media.transcode_path)?,
                            BTreeMap::new(),
                            0,
                            media.transcode_start_ms,
                        )
                    } else {
                        (
                            c.media_url(&media.direct_path)?,
                            BTreeMap::new(),
                            prepared.plan.resume_ms,
                            0,
                        )
                    }
                }
                _ => {
                    return Err(Error::new(
                        ErrorCode::Media,
                        if companion.is_none() {
                            "This source needs a companion or an unsupported player feature. Configure a companion in Settings for torrents."
                        } else {
                            "This source type is not supported by the Linux player. Choose another source."
                        },
                    ));
                }
            };
            let _ = core.set_continue_hidden(&k, false).await;
            Ok((
                crate::app::playback::Target {
                    title,
                    context,
                    url,
                    headers,
                    resume_ms,
                    offset_ms,
                    subtitles: prepared.plan.source.subtitles,
                },
                companion,
                token,
            ))
        };
        self.run(
            async move { Ok(preparation.await) },
            move |ui, result| match result {
                Ok((target, companion, token)) => {
                    ui.launch_player(key, video, target, companion, token)
                }
                Err(e) => {
                    ui.toast.add_toast(adw::Toast::new(&e.message));
                    ui.continue_playback(key, video, retry_context, false);
                }
            },
        );
    }
    pub(in crate::app) fn continue_playback(
        self: &Rc<Self>,
        key: ItemKey,
        video: String,
        mut context: crate::app::playback::PlaybackContext,
        new_episode: bool,
    ) {
        if new_episode {
            context.attempted.clear();
        }
        let Some(group) = context.group.clone() else {
            self.sources(key, video);
            return;
        };
        if context.attempted.len() >= 3 {
            self.sources(key, video);
            return;
        }
        let core = self.core.borrow().as_ref().unwrap().clone();
        let request = ResourceRequest {
            resource: Resource::Stream,
            content_type: key.content_type.clone(),
            id: video.clone(),
            extra: Default::default(),
        };
        let preferred = context.provider.clone();
        let attempted = context.attempted.clone();
        if self.button_loading.borrow().is_none() {
            self.loading_page(false);
        }
        self.run(
            async move {
                let results = core.query_all(request).await?;
                let choice = matching_source(&results, &group, &preferred, &attempted);
                Ok((choice, results, core.snapshot().await?))
            },
            move |ui, (choice, results, snapshot)| {
                if let Some((provider, stream)) = choice {
                    let transcode = context.transcode;
                    ui.start_source(key, video, stream, provider, Some(context), transcode);
                } else {
                    ui.show_sources(key, video, results, snapshot);
                }
            },
        );
    }
    fn launch_player(
        self: &Rc<Self>,
        key: ItemKey,
        video: String,
        target: crate::app::playback::Target,
        companion: Option<Companion>,
        token: Option<String>,
    ) {
        self.embedded_player(key, video, target, companion, token);
    }
}

// Match only the addon's exact continuity identifier, retaining provider order
// within the preferred-provider tier. Keep the results intact for the picker.
fn matching_source(
    results: &[madari_model::ProviderResult],
    group: &str,
    preferred: &str,
    attempted: &std::collections::HashSet<String>,
) -> Option<(String, Stream)> {
    results
        .iter()
        .flat_map(|result| {
            let streams = match &result.result {
                Ok(ResourceData::Stream(streams)) => streams.as_slice(),
                _ => &[],
            };
            streams
                .iter()
                .map(move |stream| (&result.installation_id, stream))
        })
        .filter(|(provider, stream)| {
            madari_core::same_binge_group(stream, group)
                && !attempted.contains(&crate::app::playback::fingerprint(provider, stream))
        })
        .min_by_key(|(provider, _)| provider.as_str() != preferred)
        .map(|(provider, stream)| (provider.clone(), stream.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(id: &str, group: &str) -> madari_model::ProviderResult {
        madari_model::ProviderResult {
            installation_id: id.into(),
            result: Ok(ResourceData::Stream(vec![
                serde_json::from_value(
                    serde_json::json!({"url": format!("https://example.com/{id}"),
                    "behaviorHints": {"bingeGroup": group}}),
                )
                .unwrap(),
            ])),
        }
    }

    #[test]
    fn continuation_prefers_saved_provider_and_skips_failed_sources() {
        let results = vec![provider("other", "exact"), provider("saved", "exact")];
        let mut attempted = std::collections::HashSet::new();
        let (id, stream) = matching_source(&results, "exact", "saved", &attempted).unwrap();
        assert_eq!(id, "saved");
        attempted.insert(crate::app::playback::fingerprint(&id, &stream));
        let (id, stream) = matching_source(&results, "exact", "saved", &attempted).unwrap();
        assert_eq!(id, "other");
        attempted.insert(crate::app::playback::fingerprint(&id, &stream));
        assert!(matching_source(&results, "exact", "saved", &attempted).is_none());
    }

    #[test]
    fn unmatched_or_missing_group_leaves_streams_for_picker() {
        let results = vec![
            provider("saved", "quality-higher"),
            provider("other", "quality"),
        ];
        let attempted = std::collections::HashSet::new();
        for group in ["", "Quality", "missing"] {
            assert!(matching_source(&results, group, "saved", &attempted).is_none());
        }
        assert_eq!(
            matching_source(&results, "quality", "saved", &attempted)
                .unwrap()
                .0,
            "other"
        );
        assert!(results.iter().all(|result| matches!(&result.result,
            Ok(ResourceData::Stream(streams)) if streams.len() == 1)));
    }
}
