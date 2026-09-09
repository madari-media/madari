//! Profile-authorized Trakt secrets and cached imports, separate from public state.
use super::*;
use crate::trakt::{Client, Credentials, Data, Tokens};

#[derive(Clone, Serialize, Deserialize)]
struct Account {
    credentials: Credentials,
    tokens: Tokens,
    #[serde(default)]
    connection_id: String,
}

#[derive(Clone)]
pub struct PlaybackGrant {
    profile_id: String,
    connection_id: String,
}
fn account(db: &Database, session: &ProfileSession) -> Result<Option<Account>> {
    authorized(db, session, false)?;
    db.connection
        .query_row(
            "SELECT account FROM profile_trakt WHERE profile_id=?1",
            [&session.profile.id],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(storage_error)?
        .map(|s| serde_json::from_str(&s).map_err(storage_error))
        .transpose()
}
fn write_account(db: &mut Database, session: &ProfileSession, account: &Account) -> Result<()> {
    authorized(db, session, false)?;
    db.connection.execute("INSERT INTO profile_trakt(profile_id,account,data) VALUES(?1,?2,NULL) ON CONFLICT(profile_id) DO UPDATE SET account=excluded.account,data=NULL",
        params![session.profile.id,serde_json::to_string(account).map_err(storage_error)?]).map_err(storage_error)?;
    Ok(())
}
// Finish saving a token rotation even if the user switches profiles during HTTP.
// The operation was authorized before refresh; compare the exact prior account so
// this cannot recreate a disconnected account or overwrite a newer connection.
fn rotate_account(db: &mut Database, id: &str, previous: &Account, next: &Account) -> Result<()> {
    let changed = db
        .connection
        .execute(
            "UPDATE profile_trakt SET account=?1 WHERE profile_id=?2 AND json_extract(account,'$.tokens.refresh_token')=?3 AND COALESCE(json_extract(account,'$.connection_id'),'')=?4",
            params![
                serde_json::to_string(next).map_err(storage_error)?,
                id,
                previous.tokens.refresh_token,
                previous.connection_id
            ],
        )
        .map_err(storage_error)?;
    if changed != 1 {
        return Err(Error::new(
            ErrorCode::Conflict,
            "Trakt connection changed. Try syncing again.",
        ));
    }
    Ok(())
}
impl Profiles {
    pub async fn trakt_playback(&self, session: ProfileSession) -> Result<Option<PlaybackGrant>> {
        self.run(move |db| {
            Ok(account(db, &session)?.map(|a| PlaybackGrant {
                profile_id: session.profile.id,
                connection_id: a.connection_id,
            }))
        })
        .await
    }
    pub async fn scrobble_trakt(
        &self,
        grant: PlaybackGrant,
        event: crate::trakt::Scrobble,
    ) -> Result<Option<crate::trakt::Outcome>> {
        let _guard = self.trakt_gate.lock().await;
        let id = grant.profile_id.clone();
        let connection_id = grant.connection_id.clone();
        let Some(mut saved) = self
            .run(move |db| {
                let value = db
                    .connection
                    .query_row(
                        "SELECT account FROM profile_trakt WHERE profile_id=?1",
                        [id],
                        |r| r.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(storage_error)?;
                let saved = value
                    .map(|s| serde_json::from_str::<Account>(&s).map_err(storage_error))
                    .transpose()?;
                Ok(saved.filter(|a| a.connection_id == connection_id))
            })
            .await?
        else {
            return Ok(None);
        };
        let client = Client::new(saved.credentials.clone())?;
        if saved.tokens.expires_soon() {
            let previous = saved.clone();
            saved.tokens = client.refresh(&saved.tokens).await?;
            let updated = saved.clone();
            let id = grant.profile_id.clone();
            self.run(move |db| rotate_account(db, &id, &previous, &updated))
                .await?;
        }
        let result = client.scrobble(&saved.tokens, &event).await;
        let result = if result
            .as_ref()
            .is_err_and(|e| e.code == ErrorCode::Forbidden)
        {
            let previous = saved.clone();
            saved.tokens = client.refresh(&saved.tokens).await?;
            let updated = saved.clone();
            let id = grant.profile_id.clone();
            self.run(move |db| rotate_account(db, &id, &previous, &updated))
                .await?;
            client.scrobble(&saved.tokens, &event).await
        } else {
            result
        };
        if matches!(result, Ok(crate::trakt::Outcome::Watched)) {
            let id = grant.profile_id;
            // The next import must reflect Trakt's watched/watchlist changes.
            let _ = self.run(move |db| {
                db.connection.execute(
                    "UPDATE profile_trakt SET data=json_set(data,'$.synced_at',0) WHERE profile_id=?1 AND data IS NOT NULL",
                    [id],
                ).map_err(storage_error)?;
                Ok(())
            }).await;
        }
        result.map(Some)
    }
    pub async fn trakt_connected(&self, session: ProfileSession) -> Result<bool> {
        self.run(move |db| Ok(account(db, &session)?.is_some()))
            .await
    }
    pub async fn trakt_data(&self, session: ProfileSession) -> Result<Option<Data>> {
        self.run(move |db| {
            authorized(db, &session, false)?;
            let data = db
                .connection
                .query_row(
                    "SELECT data FROM profile_trakt WHERE profile_id=?1",
                    [&session.profile.id],
                    |r| r.get::<_, Option<String>>(0),
                )
                .optional()
                .map_err(storage_error)?
                .flatten();
            data.map(|s| serde_json::from_str(&s).map_err(storage_error))
                .transpose()
        })
        .await
    }
    pub async fn connect_trakt(
        &self,
        session: ProfileSession,
        client: Client,
        tokens: Tokens,
    ) -> Result<()> {
        let _guard = self.trakt_gate.lock().await;
        self.run(move |db| {
            authorized(db, &session, true)?;
            // The account and cleared cache change in one SQLite statement.
            write_account(
                db,
                &session,
                &Account {
                    credentials: client.credentials(),
                    tokens,
                    connection_id: Uuid::new_v4().to_string(),
                },
            )
        })
        .await
    }
    /// Serialize refresh + import + disconnect: refresh tokens are single-use.
    pub async fn sync_trakt(
        &self,
        session: ProfileSession,
        only_if_stale: bool,
    ) -> Result<Option<Data>> {
        let _guard = self.trakt_gate.lock().await;
        if only_if_stale
            && let Some(data) = self.trakt_data(session.clone()).await?
            && data.artwork_version >= 1
            && now().saturating_sub(data.synced_at as i64) < 15 * 60
        {
            return Ok(Some(data));
        }
        let s = session.clone();
        let Some(mut account) = self.run(move |db| account(db, &s)).await? else {
            return Ok(None);
        };
        let client = Client::new(account.credentials.clone())?;
        if account.tokens.expires_soon() {
            let previous = account.clone();
            account.tokens = client.refresh(&account.tokens).await?;
            let id = session.profile.id.clone();
            let updated = account.clone();
            self.run(move |db| rotate_account(db, &id, &previous, &updated))
                .await?;
        }
        let data = match client.pull(&account.tokens).await {
            Err(e) if e.code == ErrorCode::Forbidden => {
                let previous = account.clone();
                account.tokens = client.refresh(&account.tokens).await?;
                let id = session.profile.id.clone();
                let updated = account.clone();
                self.run(move |db| rotate_account(db, &id, &previous, &updated))
                    .await?;
                client.pull(&account.tokens).await?
            }
            other => other?,
        };
        let saved = data.clone();
        self.run(move |db| {
            authorized(db, &session, false)?;
            db.connection
                .execute(
                    "UPDATE profile_trakt SET data=?1 WHERE profile_id=?2",
                    params![
                        serde_json::to_string(&saved).map_err(storage_error)?,
                        session.profile.id
                    ],
                )
                .map_err(storage_error)?;
            Ok(())
        })
        .await?;
        Ok(Some(data))
    }
    /// Disconnect locally even when offline. The boolean reports remote revocation.
    pub async fn disconnect_trakt(&self, session: ProfileSession) -> Result<bool> {
        let _guard = self.trakt_gate.lock().await;
        let saved = self
            .run(move |db| {
                authorized(db, &session, true)?;
                let saved = account(db, &session)?;
                db.connection
                    .execute(
                        "DELETE FROM profile_trakt WHERE profile_id=?1",
                        [&session.profile.id],
                    )
                    .map_err(storage_error)?;
                Ok(saved)
            })
            .await?;
        if let Some(account) = saved {
            return Ok(Client::new(account.credentials)?
                .revoke(&account.tokens)
                .await
                .is_ok());
        }
        Ok(true)
    }
    /// Import watched flags once metadata supplies real movie/episode IDs.
    /// Prepend history so an ongoing local rewatch remains the latest progress.
    pub async fn apply_trakt_history(
        &self,
        session: ProfileSession,
        key: madari_model::ItemKey,
        meta: madari_model::Meta,
    ) -> Result<()> {
        let Some(data) = self.trakt_data(session.clone()).await? else {
            return Ok(());
        };
        self.run(move |db| {
            let mut snapshot = load_snapshot(db, &session)?;
            let mut imported = data.completed_progress(&key, &meta, &snapshot.progress);
            if imported.is_empty() {
                return Ok(());
            }
            imported.append(&mut snapshot.progress);
            snapshot.progress = imported;
            snapshot.addons.clear();
            snapshot.revision = snapshot
                .revision
                .checked_add(1)
                .filter(|r| *r <= i64::MAX as u64)
                .ok_or_else(|| storage_error("revision exhausted"))?;
            let tx = db.connection.transaction().map_err(storage_error)?;
            tx.execute(
                "UPDATE profile_state SET data=?1 WHERE profile_id=?2",
                params![
                    serde_json::to_string(&snapshot).map_err(storage_error)?,
                    session.profile.id
                ],
            )
            .map_err(storage_error)?;
            tx.execute(
                "UPDATE profile_meta SET revision=?1 WHERE id=1",
                [snapshot.revision],
            )
            .map_err(storage_error)?;
            tx.commit().map_err(storage_error)
        })
        .await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cached_accounts_are_profile_scoped_and_not_exported() {
        let dir = tempfile::tempdir().unwrap();
        let profiles = Profiles::open(dir.path().join("profiles.sqlite"))
            .await
            .unwrap();
        let p = profiles
            .create(None, "One".into(), false, String::new())
            .await
            .unwrap();
        let s = profiles.unlock(p.id.clone(), String::new()).await.unwrap();
        profiles
            .authorize_settings(s.clone(), String::new())
            .await
            .unwrap();
        let client = Client::new(Credentials {
            client_id: "id".into(),
            client_secret: "secret-marker".into(),
            redirect_uri: "urn:ietf:wg:oauth:2.0:oob".into(),
        })
        .unwrap();
        profiles
            .connect_trakt(
                s.clone(),
                client,
                Tokens {
                    access_token: "access-marker".into(),
                    refresh_token: "refresh-marker".into(),
                    created_at: 0,
                    expires_in: 1,
                },
            )
            .await
            .unwrap();
        let old_grant = profiles.trakt_playback(s.clone()).await.unwrap().unwrap();
        assert!(profiles.trakt_connected(s.clone()).await.unwrap());
        let public =
            serde_json::to_string(&profiles.core(s.clone()).snapshot().await.unwrap()).unwrap();
        assert!(!public.contains("marker"));
        let p2 = profiles
            .create(Some(s.clone()), "Two".into(), false, String::new())
            .await
            .unwrap();
        let copy = s.clone();
        let previous = profiles
            .run(move |db| Ok(account(db, &copy)?.unwrap()))
            .await
            .unwrap();
        profiles.leave(s.clone(), String::new()).await.unwrap();
        let s2 = profiles.unlock(p2.id, String::new()).await.unwrap();
        assert!(!profiles.trakt_connected(s2).await.unwrap());
        assert!(profiles.trakt_connected(s.clone()).await.is_err());
        let id = p.id.clone();
        let mut updated = previous.clone();
        updated.tokens.refresh_token = "rotated-marker".into();
        let old = previous.clone();
        let new = updated.clone();
        let profile_id = id.clone();
        profiles
            .run(move |db| rotate_account(db, &profile_id, &old, &new))
            .await
            .unwrap();
        // Replaying a stale rotation must never overwrite newer tokens.
        assert!(
            profiles
                .run(move |db| rotate_account(db, &id, &previous, &updated))
                .await
                .is_err()
        );

        let s = profiles.unlock(p.id, String::new()).await.unwrap();
        assert!(profiles.trakt_connected(s.clone()).await.unwrap());
        let check = s.clone();
        profiles
            .run(move |db| {
                assert_eq!(
                    account(db, &check)?.unwrap().tokens.refresh_token,
                    "rotated-marker"
                );
                Ok(())
            })
            .await
            .unwrap();
        profiles
            .authorize_settings(s.clone(), String::new())
            .await
            .unwrap();
        let copy = s.clone();
        profiles
            .run(move |db| {
                let mut next = account(db, &copy)?.unwrap();
                next.connection_id = Uuid::new_v4().to_string();
                write_account(db, &copy, &next)
            })
            .await
            .unwrap();
        let key = madari_model::ItemKey {
            installation_id: "addon".into(),
            content_type: "movie".into(),
            item_id: "tt123".into(),
        };
        let event = crate::trakt::Scrobble {
            media: crate::trakt::Media::from_video(&key, "tt123", &[]).unwrap(),
            action: crate::trakt::Action::Stop,
            progress: 99.0,
        };
        assert!(
            profiles
                .scrobble_trakt(old_grant, event)
                .await
                .unwrap()
                .is_none()
        );
    }
}
