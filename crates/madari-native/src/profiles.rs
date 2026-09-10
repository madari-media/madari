//! Linux profile sessions, PIN verification and transactional shared-addon storage.
//! Each profile gets a Core storage view; configured installations remain linked.
pub mod avatars;
mod trakt;
use crate::{NativeHttp, storage_error};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use async_trait::async_trait;
use madari_core::{Core, Installation, Snapshot, Storage};
use madari_model::{Error, ErrorCode, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
pub use trakt::PlaybackGrant;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub kids: bool,
    pub pin_protected: bool,
    pub guardian_id: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
}

#[derive(Clone)]
pub struct ProfileSession {
    pub profile: Profile,
    token: String,
}

struct Active {
    id: String,
    token: String,
    editable: bool,
}
struct Database {
    connection: Connection,
    active: Option<Active>,
}
#[derive(Clone)]
pub struct Profiles {
    database: Arc<Mutex<Database>>,
    trakt_gate: Arc<tokio::sync::Mutex<()>>,
}

fn forbidden(message: &str) -> Error {
    Error::new(ErrorCode::Forbidden, message)
}
fn invalid(message: &str) -> Error {
    Error::new(ErrorCode::InvalidInput, message)
}
fn conflict(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::Conflict, message)
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64
}

fn profile(conn: &Connection, id: &str) -> Result<Profile> {
    conn.query_row(
        "SELECT id,name,kids,pin_hash IS NOT NULL,guardian_id,avatar FROM profiles WHERE id=?1",
        [id],
        |r| {
            Ok(Profile {
                id: r.get(0)?,
                name: r.get(1)?,
                kids: r.get(2)?,
                pin_protected: r.get(3)?,
                guardian_id: r.get(4)?,
                avatar: r.get(5)?,
            })
        },
    )
    .optional()
    .map_err(storage_error)?
    .ok_or_else(|| Error::new(ErrorCode::NotFound, "profile not found"))
}

fn name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 40 || value.chars().any(char::is_control) {
        return Err(invalid(
            "profile names must contain 1–40 visible characters",
        ));
    }
    Ok(value.to_owned())
}

fn hash_pin(pin: &str) -> Result<String> {
    if !(4..=8).contains(&pin.len()) || !pin.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid("use a PIN containing 4–8 digits"));
    }
    let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes()).map_err(storage_error)?;
    Argon2::default()
        .hash_password(pin.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(storage_error)
}

fn verify_pin(conn: &Connection, id: &str, pin: &str) -> Result<()> {
    let (hash, failed, until): (Option<String>, i64, i64) = conn
        .query_row(
            "SELECT pin_hash,failed_attempts,locked_until FROM profiles WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(storage_error)?;
    if until > now() {
        return Err(forbidden(
            "too many incorrect PINs; wait one minute before trying again",
        ));
    }
    let Some(hash) = hash else {
        return Ok(());
    };
    let parsed = PasswordHash::new(&hash).map_err(storage_error)?;
    let valid = pin.len() <= 8
        && Argon2::default()
            .verify_password(pin.as_bytes(), &parsed)
            .is_ok();
    if !valid {
        let attempts = if until > 0 { 1 } else { failed + 1 };
        let locked = if attempts >= 5 { now() + 60 } else { 0 };
        conn.execute(
            "UPDATE profiles SET failed_attempts=?1,locked_until=?2 WHERE id=?3",
            params![attempts, locked, id],
        )
        .map_err(storage_error)?;
        return Err(forbidden("incorrect PIN"));
    }
    conn.execute(
        "UPDATE profiles SET failed_attempts=0,locked_until=0 WHERE id=?1",
        [id],
    )
    .map_err(storage_error)?;
    Ok(())
}

fn authorized(db: &Database, session: &ProfileSession, editing: bool) -> Result<Profile> {
    let active = db
        .active
        .as_ref()
        .filter(|a| a.token == session.token && a.id == session.profile.id)
        .ok_or_else(|| forbidden("profile session has ended; open the profile again"))?;
    let locked: Option<String> = db
        .connection
        .query_row("SELECT active_kids FROM profile_meta WHERE id=1", [], |r| {
            r.get(0)
        })
        .map_err(storage_error)?;
    if locked.as_ref().is_some_and(|id| id != &active.id) {
        return Err(forbidden("leave kids mode using the guardian PIN first"));
    }
    if editing && !active.editable {
        return Err(forbidden(
            "unlock profile settings before changing addons or profiles",
        ));
    }
    profile(&db.connection, &active.id)
}

fn guardian(_conn: &Connection, p: &Profile) -> Result<String> {
    if p.kids {
        p.guardian_id
            .clone()
            .ok_or_else(|| forbidden("kids profile has no guardian"))
    } else {
        Ok(p.id.clone())
    }
}

impl Profiles {
    pub async fn open(path: PathBuf) -> Result<Self> {
        let database = tokio::task::spawn_blocking(move || {
            let conn = Connection::open(path).map_err(storage_error)?;
            let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).map_err(storage_error)?;
            if version > 2 { return Err(storage_error("profile database was created by a newer version of Madari")); }
            conn.busy_timeout(std::time::Duration::from_secs(5)).map_err(storage_error)?;
            conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;
                CREATE TABLE IF NOT EXISTS profile_meta(id INTEGER PRIMARY KEY CHECK(id=1),revision INTEGER NOT NULL,active_kids TEXT);
                INSERT OR IGNORE INTO profile_meta VALUES(1,0,NULL);
                CREATE TABLE IF NOT EXISTS profiles(id TEXT PRIMARY KEY,name TEXT NOT NULL,kids INTEGER NOT NULL,pin_hash TEXT,guardian_id TEXT REFERENCES profiles(id),failed_attempts INTEGER NOT NULL DEFAULT 0,locked_until INTEGER NOT NULL DEFAULT 0);
                CREATE TABLE IF NOT EXISTS profile_trakt(profile_id TEXT PRIMARY KEY REFERENCES profiles(id) ON DELETE CASCADE,account TEXT NOT NULL,data TEXT);
                CREATE TABLE IF NOT EXISTS profile_state(profile_id TEXT PRIMARY KEY REFERENCES profiles(id),data TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS shared_addons(id TEXT PRIMARY KEY,data TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS profile_addons(profile_id TEXT REFERENCES profiles(id),addon_id TEXT REFERENCES shared_addons(id),position INTEGER NOT NULL,enabled INTEGER NOT NULL,PRIMARY KEY(profile_id,addon_id));
                CREATE TABLE IF NOT EXISTS profile_metadata_cache(profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,cache_key TEXT NOT NULL,fetched_at INTEGER NOT NULL,data TEXT NOT NULL,PRIMARY KEY(profile_id,cache_key));
            ").map_err(storage_error)?;
            if version < 2 {
                // Recheck under the write lock: two clients can open the same store together.
                conn.execute_batch("BEGIN IMMEDIATE;").map_err(storage_error)?;
                let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).map_err(storage_error)?;
                if version < 2 {
                    conn.execute_batch("ALTER TABLE profiles ADD COLUMN avatar TEXT;
                        PRAGMA user_version=2;").map_err(storage_error)?;
                }
                conn.execute_batch("COMMIT;").map_err(storage_error)?;
            }
            Ok(Database { connection: conn, active: None })
        }).await.map_err(storage_error)??;
        Ok(Self {
            database: Arc::new(Mutex::new(database)),
            trakt_gate: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    async fn run<T: Send + 'static>(
        &self,
        action: impl FnOnce(&mut Database) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let db = self.database.clone();
        tokio::task::spawn_blocking(move || action(&mut *db.lock().map_err(storage_error)?))
            .await
            .map_err(storage_error)?
    }

    pub async fn list(&self) -> Result<Vec<Profile>> {
        self.run(|db| {
            let mut stmt = db
                .connection
                .prepare("SELECT id FROM profiles ORDER BY rowid")
                .map_err(storage_error)?;
            let ids = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(storage_error)?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(storage_error)?;
            ids.iter().map(|id| profile(&db.connection, id)).collect()
        })
        .await
    }

    pub async fn active_kids(&self) -> Result<Option<Profile>> {
        self.run(|db| {
            let id: Option<String> = db
                .connection
                .query_row("SELECT active_kids FROM profile_meta WHERE id=1", [], |r| {
                    r.get(0)
                })
                .map_err(storage_error)?;
            id.map(|id| profile(&db.connection, &id)).transpose()
        })
        .await
    }

    /// First profile bootstraps the device. Further profiles need adult settings access.
    pub async fn create(
        &self,
        session: Option<ProfileSession>,
        profile_name: String,
        kids: bool,
        pin: String,
    ) -> Result<Profile> {
        self.create_with_avatar(session, profile_name, kids, pin, None)
            .await
    }

    pub async fn create_with_avatar(
        &self,
        session: Option<ProfileSession>,
        profile_name: String,
        kids: bool,
        pin: String,
        avatar: Option<String>,
    ) -> Result<Profile> {
        self.run(move |db| {
            let name = name(&profile_name)?;
            let avatar = avatars::validate(avatar)?;
            let count: i64 = db
                .connection
                .query_row("SELECT count(*) FROM profiles", [], |r| r.get(0))
                .map_err(storage_error)?;
            if count >= 12 {
                return Err(invalid("this device supports up to 12 profiles"));
            }
            let actor = match session {
                Some(ref s) => Some(authorized(db, s, true)?),
                None if count == 0 && !kids => None,
                _ => {
                    return Err(forbidden(
                        "open a regular profile and unlock settings first",
                    ));
                }
            };
            if actor.as_ref().is_some_and(|p| p.kids) {
                return Err(forbidden("create profiles from a regular profile"));
            }
            let guardian_id = if kids {
                let adult = actor.filter(|p| p.pin_protected).ok_or_else(|| {
                    forbidden("set a PIN on the regular profile before creating a kids profile")
                })?;
                Some(adult.id)
            } else {
                None
            };
            let hash = if !kids && !pin.is_empty() {
                Some(hash_pin(&pin)?)
            } else {
                None
            };
            let id = Uuid::new_v4().to_string();
            let tx = db.connection.transaction().map_err(storage_error)?;
            tx.execute(
                "INSERT INTO profiles(id,name,kids,pin_hash,guardian_id,avatar) VALUES(?1,?2,?3,?4,?5,?6)",
                params![id, name, kids, hash, guardian_id, avatar],
            )
            .map_err(storage_error)?;
            tx.execute(
                "INSERT INTO profile_state VALUES(?1,?2)",
                params![
                    id,
                    serde_json::to_string(&Snapshot::default()).map_err(storage_error)?
                ],
            )
            .map_err(storage_error)?;
            tx.execute("UPDATE profile_meta SET revision=revision+1 WHERE id=1", [])
                .map_err(storage_error)?;
            tx.commit().map_err(storage_error)?;
            profile(&db.connection, &id)
        })
        .await
    }

    pub async fn unlock(&self, id: String, pin: String) -> Result<ProfileSession> {
        self.run(move |db| {
            let locked: Option<String> = db
                .connection
                .query_row("SELECT active_kids FROM profile_meta WHERE id=1", [], |r| {
                    r.get(0)
                })
                .map_err(storage_error)?;
            if locked.as_ref().is_some_and(|kid| kid != &id) {
                return Err(forbidden("leave kids mode using the guardian PIN first"));
            }
            let p = profile(&db.connection, &id)?;
            if !p.kids {
                verify_pin(&db.connection, &id, &pin)?;
            }
            if p.kids {
                db.connection
                    .execute("UPDATE profile_meta SET active_kids=?1 WHERE id=1", [&id])
                    .map_err(storage_error)?;
            }
            let token = Uuid::new_v4().to_string();
            db.active = Some(Active {
                id,
                token: token.clone(),
                editable: false,
            });
            Ok(ProfileSession { profile: p, token })
        })
        .await
    }

    /// Open an administrative session without changing the device's active kids mode.
    /// Call on a separate Profiles connection so the watching session stays intact.
    pub async fn unlock_management(&self, id: String, pin: String) -> Result<ProfileSession> {
        self.run(move |db| {
            let locked: Option<String> = db
                .connection
                .query_row("SELECT active_kids FROM profile_meta WHERE id=1", [], |r| {
                    r.get(0)
                })
                .map_err(storage_error)?;
            if locked.as_ref().is_some_and(|kid| kid != &id) {
                return Err(forbidden(
                    "Leave kids mode on the TV using the guardian PIN first",
                ));
            }
            let p = profile(&db.connection, &id)?;
            verify_pin(&db.connection, &guardian(&db.connection, &p)?, &pin)?;
            let token = Uuid::new_v4().to_string();
            db.active = Some(Active {
                id,
                token: token.clone(),
                editable: true,
            });
            Ok(ProfileSession { profile: p, token })
        })
        .await
    }

    pub async fn authorize_settings(&self, session: ProfileSession, pin: String) -> Result<()> {
        self.run(move |db| {
            let p = authorized(db, &session, false)?;
            verify_pin(&db.connection, &guardian(&db.connection, &p)?, &pin)?;
            db.active.as_mut().expect("authorized session").editable = true;
            Ok(())
        })
        .await
    }

    pub async fn lock_settings(&self, session: ProfileSession) -> Result<()> {
        self.run(move |db| {
            authorized(db, &session, false)?;
            db.active.as_mut().expect("session").editable = false;
            Ok(())
        })
        .await
    }

    pub async fn leave(&self, session: ProfileSession, pin: String) -> Result<()> {
        self.run(move |db| {
            let p = authorized(db, &session, false)?;
            if p.kids {
                verify_pin(&db.connection, &guardian(&db.connection, &p)?, &pin)?;
            }
            db.connection
                .execute("UPDATE profile_meta SET active_kids=NULL WHERE id=1", [])
                .map_err(storage_error)?;
            db.active = None;
            Ok(())
        })
        .await
    }

    /// A nonempty new PIN replaces the current one. Guardian PIN removal is not offered.
    pub async fn update(
        &self,
        session: ProfileSession,
        profile_name: String,
        new_pin: String,
    ) -> Result<Profile> {
        self.update_fields(session, profile_name, new_pin, None)
            .await
    }

    /// Explicitly set an image, or return to initials with None. Legacy update keeps it.
    pub async fn update_with_avatar(
        &self,
        session: ProfileSession,
        profile_name: String,
        new_pin: String,
        avatar: Option<String>,
    ) -> Result<Profile> {
        self.update_fields(session, profile_name, new_pin, Some(avatar))
            .await
    }

    async fn update_fields(
        &self,
        session: ProfileSession,
        profile_name: String,
        new_pin: String,
        avatar: Option<Option<String>>,
    ) -> Result<Profile> {
        self.run(move |db| {
            let p = authorized(db, &session, true)?;
            let name = name(&profile_name)?;
            let avatar = match avatar {
                Some(value) => avatars::validate(value)?,
                None => p.avatar,
            };
            if p.kids && !new_pin.is_empty() {
                return Err(invalid("kids profiles use their guardian's PIN"));
            }
            let hash = if new_pin.is_empty() {
                None
            } else {
                Some(hash_pin(&new_pin)?)
            };
            let tx = db.connection.transaction().map_err(storage_error)?;
            tx.execute(
                "UPDATE profiles SET name=?1,avatar=?2,pin_hash=COALESCE(?3,pin_hash),
                 failed_attempts=CASE WHEN ?3 IS NULL THEN failed_attempts ELSE 0 END,
                 locked_until=CASE WHEN ?3 IS NULL THEN locked_until ELSE 0 END WHERE id=?4",
                params![name, avatar, hash, p.id],
            )
            .map_err(storage_error)?;
            tx.execute("UPDATE profile_meta SET revision=revision+1 WHERE id=1", [])
                .map_err(storage_error)?;
            tx.commit().map_err(storage_error)?;
            profile(&db.connection, &p.id)
        })
        .await
    }

    /// Deletes a profile and everything that belongs to it.
    ///
    /// Destructive, so it takes the same authorization as editing: the caller must
    /// hold a session with settings unlocked, and a kids profile additionally needs
    /// the guardian's PIN.
    ///
    /// A profile that guards another is refused rather than cascaded. A kids profile
    /// without its guardian could not be left, authorized or deleted afterwards, so
    /// the guardians are named and the caller decides.
    pub async fn delete(&self, session: ProfileSession, pin: String) -> Result<()> {
        self.run(move |db| {
            let p = authorized(db, &session, true)?;
            if p.kids {
                verify_pin(&db.connection, &guardian(&db.connection, &p)?, &pin)?;
            }
            // Scoped so the statement's borrow of the connection ends before the
            // transaction below needs it mutably.
            let dependents: Vec<String> = {
                let mut statement = db
                    .connection
                    .prepare("SELECT name FROM profiles WHERE guardian_id=?1 ORDER BY name")
                    .map_err(storage_error)?;
                let rows = statement
                    .query_map([&p.id], |r| r.get(0))
                    .map_err(storage_error)?;
                rows.collect::<std::result::Result<_, _>>()
                    .map_err(storage_error)?
            };
            if !dependents.is_empty() {
                return Err(conflict(format!(
                    "this profile guards {}. Delete those profiles first.",
                    dependents.join(", ")
                )));
            }
            let tx = db.connection.transaction().map_err(storage_error)?;
            // profile_trakt and profile_metadata_cache cascade on delete; these two do
            // not, so they are removed explicitly.
            tx.execute("DELETE FROM profile_state WHERE profile_id=?1", [&p.id])
                .map_err(storage_error)?;
            tx.execute("DELETE FROM profile_addons WHERE profile_id=?1", [&p.id])
                .map_err(storage_error)?;
            tx.execute("DELETE FROM profiles WHERE id=?1", [&p.id])
                .map_err(storage_error)?;
            // An installation no profile links to any more has no owner left.
            tx.execute(
                "DELETE FROM shared_addons WHERE NOT EXISTS(
                     SELECT 1 FROM profile_addons WHERE addon_id=shared_addons.id)",
                [],
            )
            .map_err(storage_error)?;
            // Kids mode cannot outlive the profile it was pinned to.
            tx.execute(
                "UPDATE profile_meta SET active_kids=NULL WHERE id=1 AND active_kids=?1",
                [&p.id],
            )
            .map_err(storage_error)?;
            tx.execute("UPDATE profile_meta SET revision=revision+1 WHERE id=1", [])
                .map_err(storage_error)?;
            tx.commit().map_err(storage_error)?;
            if db.active.as_ref().is_some_and(|a| a.id == p.id) {
                db.active = None;
            }
            Ok(())
        })
        .await
    }

    pub async fn share(
        &self,
        session: ProfileSession,
        addon_id: String,
        target_id: String,
        target_pin: String,
    ) -> Result<()> {
        self.run(move |db| {
            let p = authorized(db,&session,true)?;
            let target = profile(&db.connection,&target_id)?;
            let approver = guardian(&db.connection,&target)?;
            if approver != p.id { verify_pin(&db.connection,&approver,&target_pin)?; }
            let exists: bool = db.connection.query_row("SELECT EXISTS(SELECT 1 FROM profile_addons WHERE profile_id=?1 AND addon_id=?2)",params![p.id,addon_id],|r|r.get(0)).map_err(storage_error)?;
            if !exists { return Err(Error::new(ErrorCode::NotFound,"addon is not installed in this profile")); }
            let tx = db.connection.transaction().map_err(storage_error)?;
            tx.execute("INSERT OR IGNORE INTO profile_addons(profile_id,addon_id,position,enabled) SELECT ?1,?2,COALESCE(MAX(position)+1,0),1 FROM profile_addons WHERE profile_id=?1",params![target_id,addon_id]).map_err(storage_error)?;
            tx.execute("UPDATE profile_meta SET revision=revision+1 WHERE id=1",[]).map_err(storage_error)?;
            tx.commit().map_err(storage_error)?; Ok(())
        }).await
    }

    pub async fn linked_profiles(
        &self,
        session: ProfileSession,
        addon_id: String,
    ) -> Result<Vec<String>> {
        self.run(move |db| {
            authorized(db,&session,false)?;
            let mut stmt=db.connection.prepare("SELECT p.name FROM profiles p JOIN profile_addons a ON p.id=a.profile_id WHERE a.addon_id=?1 ORDER BY p.rowid").map_err(storage_error)?;
            stmt.query_map([addon_id],|r|r.get(0)).map_err(storage_error)?.collect::<std::result::Result<Vec<_>,_>>().map_err(storage_error)
        }).await
    }

    pub fn core(&self, session: ProfileSession) -> Arc<Core> {
        Arc::new(Core::new(
            Arc::new(NativeHttp::default()),
            Arc::new(ProfileStorage {
                profiles: self.clone(),
                session,
            }),
        ))
    }
}

struct ProfileStorage {
    profiles: Profiles,
    session: ProfileSession,
}

fn load_snapshot(db: &Database, session: &ProfileSession) -> Result<Snapshot> {
    authorized(db, session, false)?;
    let data: String = db
        .connection
        .query_row(
            "SELECT data FROM profile_state WHERE profile_id=?1",
            [&session.profile.id],
            |r| r.get(0),
        )
        .map_err(storage_error)?;
    let mut snapshot: Snapshot = serde_json::from_str(&data).map_err(storage_error)?;
    snapshot.revision = db
        .connection
        .query_row("SELECT revision FROM profile_meta WHERE id=1", [], |r| {
            r.get(0)
        })
        .map_err(storage_error)?;
    let mut stmt=db.connection.prepare("SELECT s.data,a.enabled FROM shared_addons s JOIN profile_addons a ON s.id=a.addon_id WHERE a.profile_id=?1 ORDER BY a.position,a.addon_id").map_err(storage_error)?;
    let rows = stmt
        .query_map([&session.profile.id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))
        })
        .map_err(storage_error)?;
    snapshot.addons = rows
        .map(|row| {
            let (data, enabled) = row.map_err(storage_error)?;
            let mut addon: Installation = serde_json::from_str(&data).map_err(storage_error)?;
            addon.enabled = enabled;
            Ok(addon)
        })
        .collect::<Result<_>>()?;
    Ok(snapshot)
}

#[async_trait]
impl Storage for ProfileStorage {
    async fn cached_metadata(&self) -> Result<Vec<madari_core::CachedMetadata>> {
        let session = self.session.clone();
        self.profiles.run(move |db| {
            authorized(db, &session, false)?;
            let mut query = db.connection.prepare("SELECT data FROM profile_metadata_cache WHERE profile_id=?1 ORDER BY fetched_at").map_err(storage_error)?;
            let rows = query.query_map([&session.profile.id], |r| r.get::<_, String>(0)).map_err(storage_error)?;
            let mut entries = Vec::new();
            for row in rows {
                if let Ok(entry) = serde_json::from_str(&row.map_err(storage_error)?) { entries.push(entry); }
            }
            Ok(entries)
        }).await
    }
    async fn cache_metadata(&self, entries: Vec<madari_core::CachedMetadata>) -> Result<()> {
        let session = self.session.clone();
        self.profiles.run(move |db| {
            authorized(db, &session, false)?;
            let tx = db.connection.transaction().map_err(storage_error)?;
            for entry in entries {
                tx.execute("INSERT INTO profile_metadata_cache(profile_id,cache_key,fetched_at,data) VALUES(?1,?2,?3,?4) ON CONFLICT(profile_id,cache_key) DO UPDATE SET fetched_at=excluded.fetched_at,data=excluded.data",
                    params![session.profile.id, serde_json::to_string(&entry.key).map_err(storage_error)?, entry.fetched_at, serde_json::to_string(&entry).map_err(storage_error)?]).map_err(storage_error)?;
            }
            tx.execute("DELETE FROM profile_metadata_cache WHERE profile_id=?1 AND cache_key NOT IN (SELECT cache_key FROM profile_metadata_cache WHERE profile_id=?1 ORDER BY fetched_at DESC LIMIT 100)", [&session.profile.id]).map_err(storage_error)?;
            tx.commit().map_err(storage_error)
        }).await
    }
    async fn load(&self) -> Result<Snapshot> {
        let session = self.session.clone();
        self.profiles
            .run(move |db| load_snapshot(db, &session))
            .await
    }
    async fn compare_and_swap(&self, expected: u64, mut next: Snapshot) -> Result<()> {
        let session = self.session.clone();
        self.profiles.run(move |db| {
            let previous=load_snapshot(db,&session)?;
            if next.revision != expected.checked_add(1).ok_or_else(||invalid("revision exhausted"))? || next.revision > i64::MAX as u64 { return Err(invalid("invalid snapshot revision")); }
            if previous.revision != expected { return Err(Error::new(ErrorCode::Conflict,"profile state changed")); }
            let addons_changed=serde_json::to_value(&previous.addons).map_err(storage_error)? != serde_json::to_value(&next.addons).map_err(storage_error)?;
            if addons_changed || previous.playback_preferences != next.playback_preferences { authorized(db,&session,true)?; }
            let mut ids=HashSet::new();
            if next.addons.iter().any(|a|!ids.insert(&a.installation_id)) { return Err(invalid("duplicate addon installation")); }
            let tx=db.connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(storage_error)?;
            let changed=tx.execute("UPDATE profile_meta SET revision=?1 WHERE id=1 AND revision=?2",params![next.revision,expected]).map_err(storage_error)?;
            if changed != 1 { return Err(Error::new(ErrorCode::Conflict,"profile state changed")); }
            if addons_changed {
                let old_ids:HashSet<_>=previous.addons.iter().map(|a|&a.installation_id).collect();
                for (position,addon) in next.addons.iter().enumerate() {
                    // New IDs may not attach an existing installation by guessing its ID.
                    let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM shared_addons WHERE id=?1)",[&addon.installation_id],|r|r.get(0)).map_err(storage_error)?;
                    if exists && !old_ids.contains(&addon.installation_id) { return Err(forbidden("use the sharing action to link an existing installation")); }
                    let mut shared=addon.clone(); shared.enabled=true;
                    tx.execute("INSERT INTO shared_addons VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",params![addon.installation_id,serde_json::to_string(&shared).map_err(storage_error)?]).map_err(storage_error)?;
                    tx.execute("INSERT INTO profile_addons VALUES(?1,?2,?3,?4) ON CONFLICT(profile_id,addon_id) DO UPDATE SET position=excluded.position,enabled=excluded.enabled",params![session.profile.id,addon.installation_id,position,addon.enabled]).map_err(storage_error)?;
                }
                for addon in &previous.addons {
                    if !ids.contains(&addon.installation_id) { tx.execute("DELETE FROM profile_addons WHERE profile_id=?1 AND addon_id=?2",params![session.profile.id,addon.installation_id]).map_err(storage_error)?; }
                }
                tx.execute("DELETE FROM shared_addons WHERE NOT EXISTS(SELECT 1 FROM profile_addons WHERE addon_id=shared_addons.id)",[]).map_err(storage_error)?;
            }
            next.addons.clear();
            tx.execute("UPDATE profile_state SET data=?1 WHERE profile_id=?2",params![serde_json::to_string(&next).map_err(storage_error)?,session.profile.id]).map_err(storage_error)?;
            tx.commit().map_err(storage_error)?; Ok(())
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use madari_model::{ItemKey, LibraryEntry, Progress};

    /// Deletion is destructive, so the tests cover what it must refuse as much as what
    /// it removes.
    #[tokio::test]
    async fn deleting_a_profile_removes_its_state_and_refuses_guardians() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.sqlite");
        let p = Profiles::open(path.clone()).await.unwrap();
        let adult = p
            .create_with_avatar(
                None,
                "Parent".into(),
                false,
                "1234".into(),
                Some("Fox.webp".into()),
            )
            .await
            .unwrap();
        let session = p.unlock(adult.id.clone(), "1234".into()).await.unwrap();
        p.authorize_settings(session.clone(), "1234".into())
            .await
            .unwrap();
        let kid = p
            .create_with_avatar(Some(session.clone()), "Kid".into(), true, "".into(), None)
            .await
            .unwrap();

        // A guardian cannot be removed while a kids profile still points at it.
        let error = p.delete(session.clone(), "1234".into()).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::Conflict);
        assert!(error.message.contains("Kid"), "{}", error.message);

        // The kids profile is deleted on the guardian's authority.
        let kid_session = p.unlock(kid.id.clone(), "".into()).await.unwrap();
        assert!(
            p.delete(kid_session.clone(), "0000".into()).await.is_err(),
            "the wrong guardian PIN must not delete a kids profile"
        );
        p.authorize_settings(kid_session.clone(), "1234".into())
            .await
            .unwrap();
        p.delete(kid_session, "1234".into()).await.unwrap();
        assert_eq!(p.list().await.unwrap().len(), 1);

        // Deleting a profile ends its session, so the guardian is opened again
        // before it can be removed. Its stored state goes with it.
        let session = p.unlock(adult.id.clone(), "1234".into()).await.unwrap();
        p.authorize_settings(session.clone(), "1234".into())
            .await
            .unwrap();
        p.delete(session, "1234".into()).await.unwrap();
        assert!(p.list().await.unwrap().is_empty());
        // Read the file directly: the profile's stored state is what must be gone, not
        // just its row in `profiles`.
        let connection = rusqlite::Connection::open(&path).unwrap();
        let remaining: i64 = connection
            .query_row("SELECT COUNT(*) FROM profile_state", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
    }

    #[tokio::test]
    async fn avatar_updates_are_authorized_atomic_and_persistent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.sqlite");
        let p = Profiles::open(path.clone()).await.unwrap();
        assert!(
            p.create_with_avatar(
                None,
                "Bad".into(),
                false,
                "".into(),
                Some("../Fox.webp".into())
            )
            .await
            .is_err()
        );
        assert!(p.list().await.unwrap().is_empty());
        let adult = p
            .create_with_avatar(
                None,
                "Parent".into(),
                false,
                "1234".into(),
                Some("Fox.webp".into()),
            )
            .await
            .unwrap();
        let session = p.unlock(adult.id.clone(), "1234".into()).await.unwrap();
        assert!(
            p.update_with_avatar(session.clone(), "Changed".into(), "".into(), None)
                .await
                .is_err()
        );
        p.authorize_settings(session.clone(), "1234".into())
            .await
            .unwrap();
        assert!(
            p.update_with_avatar(
                session.clone(),
                "Wrong".into(),
                "5678".into(),
                Some("unknown.webp".into())
            )
            .await
            .is_err()
        );
        assert!(
            p.update_with_avatar(
                session.clone(),
                "Wrong".into(),
                "bad".into(),
                Some("Duck.webp".into())
            )
            .await
            .is_err()
        );
        let current = p.list().await.unwrap().remove(0);
        assert_eq!(current.name, "Parent");
        assert_eq!(current.avatar.as_deref(), Some("Fox.webp"));
        let current = p
            .update(session.clone(), "Renamed".into(), "".into())
            .await
            .unwrap();
        assert_eq!(current.avatar.as_deref(), Some("Fox.webp"));
        let kid = p
            .create_with_avatar(
                Some(session.clone()),
                "Kid".into(),
                true,
                "".into(),
                Some("Robot.webp".into()),
            )
            .await
            .unwrap();
        p.update_with_avatar(
            session.clone(),
            "Renamed".into(),
            "".into(),
            Some("Black Cat.webp".into()),
        )
        .await
        .unwrap();
        p.leave(session, "".into()).await.unwrap();
        drop(p);
        let p = Profiles::open(path).await.unwrap();
        let profiles = p.list().await.unwrap();
        assert_eq!(profiles[0].avatar.as_deref(), Some("Black Cat.webp"));
        assert_eq!(profiles[1].avatar.as_deref(), Some("Robot.webp"));
        assert_eq!(profiles[1].guardian_id.as_deref(), Some(adult.id.as_str()));
        let session = p.unlock(adult.id, "1234".into()).await.unwrap();
        p.authorize_settings(session.clone(), "1234".into())
            .await
            .unwrap();
        assert!(
            p.update_with_avatar(session, "Renamed".into(), "".into(), None)
                .await
                .unwrap()
                .avatar
                .is_none()
        );
        let child = p.unlock(kid.id, "".into()).await.unwrap();
        assert!(
            p.update_with_avatar(child.clone(), "Kid".into(), "".into(), None)
                .await
                .is_err()
        );
        p.authorize_settings(child.clone(), "1234".into())
            .await
            .unwrap();
        assert!(
            p.update_with_avatar(child, "Kid".into(), "".into(), None)
                .await
                .unwrap()
                .avatar
                .is_none()
        );
    }

    #[tokio::test]
    async fn version_one_migration_preserves_profile_pin_and_storage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.sqlite");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE profiles(id TEXT PRIMARY KEY,name TEXT NOT NULL,kids INTEGER NOT NULL,pin_hash TEXT,guardian_id TEXT REFERENCES profiles(id),failed_attempts INTEGER NOT NULL DEFAULT 0,locked_until INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE profile_state(profile_id TEXT PRIMARY KEY REFERENCES profiles(id),data TEXT NOT NULL);
            PRAGMA user_version=1;").unwrap();
        conn.execute(
            "INSERT INTO profiles(id,name,kids,pin_hash) VALUES('legacy','Existing',0,?1)",
            [hash_pin("1234").unwrap()],
        )
        .unwrap();
        let snapshot = serde_json::to_string(&Snapshot::default()).unwrap();
        conn.execute("INSERT INTO profile_state VALUES('legacy',?1)", [&snapshot])
            .unwrap();
        drop(conn);
        let p = Profiles::open(path.clone()).await.unwrap();
        let profiles = p.list().await.unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "Existing");
        assert!(profiles[0].avatar.is_none());
        assert!(p.unlock("legacy".into(), "0000".into()).await.is_err());
        let session = p.unlock("legacy".into(), "1234".into()).await.unwrap();
        assert_eq!(p.core(session).snapshot().await.unwrap().revision, 0);
        let conn = Connection::open(path).unwrap();
        let stored: String = conn
            .query_row(
                "SELECT data FROM profile_state WHERE profile_id='legacy'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored, snapshot);
        assert_eq!(
            conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
    }

    fn addon() -> Installation {
        Installation {
            installation_id: "shared-test".into(), manifest_url: "https://example.com/manifest.json".parse().unwrap(),
            manifest: serde_json::from_value(serde_json::json!({"id":"test","name":"Original","version":"1.0.0","resources":["catalog"],"types":["movie"],"catalogs":[]})).unwrap(),
            enabled: true, allow_local: false,
        }
    }
    fn storage(p: &Profiles, s: &ProfileSession) -> ProfileStorage {
        ProfileStorage {
            profiles: p.clone(),
            session: s.clone(),
        }
    }
    async fn add(p: &Profiles, s: &ProfileSession) -> Result<()> {
        let store = storage(p, s);
        let mut next = store.load().await?;
        let revision = next.revision;
        next.revision += 1;
        next.addons.push(addon());
        store.compare_and_swap(revision, next).await
    }

    #[tokio::test]
    async fn pin_and_kids_policy_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.sqlite");
        let profiles = Profiles::open(path.clone()).await.unwrap();
        assert!(
            profiles
                .create(None, "Kid".into(), true, String::new())
                .await
                .is_err()
        );
        let adult = profiles
            .create(None, "Parent".into(), false, "1234".into())
            .await
            .unwrap();
        assert!(
            profiles
                .unlock(adult.id.clone(), "0000".into())
                .await
                .is_err()
        );
        let session = profiles
            .unlock(adult.id.clone(), "1234".into())
            .await
            .unwrap();
        assert!(
            profiles
                .create(Some(session.clone()), "Kid".into(), true, String::new())
                .await
                .is_err()
        );
        assert!(add(&profiles, &session).await.is_err());
        profiles
            .authorize_settings(session.clone(), "1234".into())
            .await
            .unwrap();
        let kid = profiles
            .create(Some(session.clone()), "Kid".into(), true, String::new())
            .await
            .unwrap();
        add(&profiles, &session).await.unwrap();
        profiles
            .share(
                session.clone(),
                "shared-test".into(),
                kid.id.clone(),
                String::new(),
            )
            .await
            .unwrap();
        profiles
            .leave(session.clone(), String::new())
            .await
            .unwrap();
        assert!(profiles.core(session).snapshot().await.is_err());
        let child = profiles
            .unlock(kid.id.clone(), String::new())
            .await
            .unwrap();
        assert!(
            profiles
                .core(child.clone())
                .remove_addon("shared-test")
                .await
                .is_err()
        );
        assert!(
            profiles
                .share(
                    child.clone(),
                    "shared-test".into(),
                    adult.id.clone(),
                    "1234".into()
                )
                .await
                .is_err()
        );
        assert!(profiles.leave(child.clone(), "0000".into()).await.is_err());
        assert!(
            profiles
                .unlock(adult.id.clone(), "1234".into())
                .await
                .is_err()
        );
        drop(profiles);
        let profiles = Profiles::open(path.clone()).await.unwrap();
        assert_eq!(profiles.active_kids().await.unwrap().unwrap().id, kid.id);
        assert!(
            profiles
                .unlock(adult.id.clone(), "1234".into())
                .await
                .is_err()
        );
        let child = profiles.unlock(kid.id, String::new()).await.unwrap();
        profiles
            .authorize_settings(child.clone(), "1234".into())
            .await
            .unwrap();
        profiles
            .core(child.clone())
            .set_enabled("shared-test", false)
            .await
            .unwrap();
        profiles.lock_settings(child.clone()).await.unwrap();
        assert!(
            profiles
                .core(child.clone())
                .remove_addon("shared-test")
                .await
                .is_err()
        );
        profiles.leave(child, "1234".into()).await.unwrap();
        let session = profiles
            .unlock(adult.id.clone(), "1234".into())
            .await
            .unwrap();
        assert!(profiles.core(session).snapshot().await.unwrap().addons[0].enabled);
        let db = Connection::open(path).unwrap();
        let hash: String = db
            .query_row(
                "SELECT pin_hash FROM profiles WHERE id=?1",
                [adult.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert_ne!(hash, "1234");
    }

    #[tokio::test]
    async fn linked_configuration_and_viewing_state_are_scoped_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.sqlite");
        let profiles = Profiles::open(path.clone()).await.unwrap();
        let a = profiles
            .create(None, "A".into(), false, String::new())
            .await
            .unwrap();
        let sa = profiles.unlock(a.id.clone(), String::new()).await.unwrap();
        profiles
            .authorize_settings(sa.clone(), String::new())
            .await
            .unwrap();
        let b = profiles
            .create(Some(sa.clone()), "B".into(), false, "5678".into())
            .await
            .unwrap();
        add(&profiles, &sa).await.unwrap();
        assert!(
            profiles
                .share(
                    sa.clone(),
                    "shared-test".into(),
                    b.id.clone(),
                    "0000".into()
                )
                .await
                .is_err()
        );
        profiles
            .share(
                sa.clone(),
                "shared-test".into(),
                b.id.clone(),
                "5678".into(),
            )
            .await
            .unwrap();
        let key = ItemKey {
            installation_id: "shared-test".into(),
            content_type: "movie".into(),
            item_id: "1".into(),
        };
        let core = profiles.core(sa.clone());
        let playback_preferences = madari_model::PlaybackPreferences {
            subtitle_sdh: madari_model::TrackPreference::Prefer,
            subtitle_forced: madari_model::TrackPreference::Avoid,
            audio_commentary: madari_model::TrackPreference::Avoid,
            audio_languages: vec!["fr".into(), "en".into()],
            subtitle_languages: vec!["en".into(), "hi".into()],
            subtitles_enabled: false,
            ..Default::default()
        };
        core.set_playback_preferences(playback_preferences.clone())
            .await
            .unwrap();
        core.save_item(LibraryEntry {
            metadata: Some(serde_json::from_value(serde_json::json!({"id":"1", "type":"movie", "name":"Saved movie", "poster":"https://images.example/movie.jpg"})).unwrap()),
            key: key.clone(),
            title: "Saved only for A".into(),
        })
        .await
        .unwrap();
        core.record_progress(Progress {
            metadata: Some(serde_json::from_value(serde_json::json!({"id":"1", "type":"movie", "name":"Saved movie", "poster":"https://images.example/movie.jpg"})).unwrap()),
            binge_group: Some("torrentio|1080p|WEBRip|x264".into()),
            source_provider: Some("shared-test".into()),
            key,
            video_id: "v1".into(),
            position_ms: 42000,
            duration_ms: Some(100000),
            completed: false,
        })
        .await
        .unwrap();
        let store = storage(&profiles, &sa);
        let mut next = store.load().await.unwrap();
        let revision = next.revision;
        next.revision += 1;
        next.addons[0].manifest.name = "Updated shared configuration".into();
        next.addons[0].manifest_url = "https://example.com/new/manifest.json".parse().unwrap();
        next.addons[0].enabled = false;
        store.compare_and_swap(revision, next).await.unwrap();
        profiles.leave(sa, String::new()).await.unwrap();
        let sb = profiles.unlock(b.id.clone(), "5678".into()).await.unwrap();
        let snap = storage(&profiles, &sb).load().await.unwrap();
        assert!(snap.library.is_empty());
        assert!(snap.progress.is_empty());
        assert_eq!(
            snap.playback_preferences,
            madari_model::PlaybackPreferences::default()
        );
        assert!(snap.addons[0].enabled);
        assert_eq!(snap.addons[0].manifest.name, "Updated shared configuration");
        assert!(snap.addons[0].manifest_url.path().starts_with("/new/"));
        profiles
            .authorize_settings(sb.clone(), "5678".into())
            .await
            .unwrap();
        profiles
            .core(sb.clone())
            .remove_addon("shared-test")
            .await
            .unwrap();
        profiles.leave(sb, String::new()).await.unwrap();
        drop(profiles);
        let profiles = Profiles::open(path).await.unwrap();
        let sa = profiles.unlock(a.id, String::new()).await.unwrap();
        let snap = profiles.core(sa).snapshot().await.unwrap();
        assert_eq!(snap.addons.len(), 1);
        assert_eq!(snap.library.len(), 1);
        assert_eq!(snap.progress[0].position_ms, 42000);
        assert_eq!(
            snap.progress[0].metadata.as_ref().unwrap().name,
            "Saved movie"
        );
        assert_eq!(
            snap.library[0].metadata.as_ref().unwrap().extra["poster"],
            "https://images.example/movie.jpg"
        );
        assert_eq!(snap.playback_preferences, playback_preferences);
        assert_eq!(
            snap.progress[0].binge_group.as_deref(),
            Some("torrentio|1080p|WEBRip|x264")
        );
        assert_eq!(
            snap.progress[0].source_provider.as_deref(),
            Some("shared-test")
        );
    }

    #[tokio::test]
    async fn profile_metadata_cache_persists_isolated_without_changing_revision() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.sqlite");
        let profiles = Profiles::open(path.clone()).await.unwrap();
        let a = profiles
            .create(None, "A".into(), false, String::new())
            .await
            .unwrap();
        let session = profiles.unlock(a.id.clone(), String::new()).await.unwrap();
        profiles
            .authorize_settings(session.clone(), String::new())
            .await
            .unwrap();
        let b = profiles
            .create(Some(session.clone()), "B".into(), false, String::new())
            .await
            .unwrap();
        let store = storage(&profiles, &session);
        let revision = store.load().await.unwrap().revision;
        store
            .cache_metadata(vec![madari_core::CachedMetadata {
                key: ItemKey {
                    installation_id: "addon".into(),
                    content_type: "movie".into(),
                    item_id: "tt1".into(),
                },
                resolved: madari_core::ResolvedMetadata {
                    meta: serde_json::from_value(
                        serde_json::json!({"id":"tt1", "type":"movie", "name":"Cached movie"}),
                    )
                    .unwrap(),
                    field_providers: Default::default(),
                },
                fetched_at: 10,
                sources: Vec::new(),
            }])
            .await
            .unwrap();
        assert_eq!(store.load().await.unwrap().revision, revision);
        profiles.leave(session, String::new()).await.unwrap();
        let other = profiles.unlock(b.id, String::new()).await.unwrap();
        assert!(
            storage(&profiles, &other)
                .cached_metadata()
                .await
                .unwrap()
                .is_empty()
        );
        profiles.leave(other, String::new()).await.unwrap();
        drop(store);
        drop(profiles);
        let reopened = Profiles::open(path).await.unwrap();
        let session = reopened.unlock(a.id, String::new()).await.unwrap();
        let cached = storage(&reopened, &session)
            .cached_metadata()
            .await
            .unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].resolved.meta.name, "Cached movie");
    }

    #[tokio::test]
    async fn pin_throttle_is_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.sqlite");
        let profiles = Profiles::open(path.clone()).await.unwrap();
        let adult = profiles
            .create(None, "Adult".into(), false, "1234".into())
            .await
            .unwrap();
        for _ in 0..5 {
            assert!(
                profiles
                    .unlock(adult.id.clone(), "9999".into())
                    .await
                    .is_err()
            );
        }
        drop(profiles);
        let profiles = Profiles::open(path).await.unwrap();
        let error = profiles
            .unlock(adult.id, "1234".into())
            .await
            .err()
            .unwrap();
        assert!(error.message.contains("wait one minute"));
    }
}
