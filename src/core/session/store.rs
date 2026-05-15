use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use heed::types::Bytes;
use heed::{BoxedError, BytesDecode, BytesEncode, Database, Env, EnvOpenOptions};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use tokio::sync::Mutex;

use crate::core::auth::models::{Principal, PrincipalRole};
use crate::core::session::models::SessionRecord;
use crate::error::AppError;
use crate::store::json_file;

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn get(&self, token: &str) -> Result<Option<SessionRecord>, AppError>;
    async fn put(&self, token: &str, record: SessionRecord) -> Result<(), AppError>;
    async fn delete(&self, token: &str) -> Result<(), AppError>;
}

#[derive(Clone)]
pub struct JsonSessionStore {
    path: Option<PathBuf>,
    state: Arc<Mutex<JsonSessionState>>,
}

#[derive(Default)]
struct JsonSessionState {
    loaded: bool,
    sessions: BTreeMap<String, SessionRecord>,
}

impl JsonSessionStore {
    pub fn persistent(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            state: Arc::new(Mutex::new(JsonSessionState::default())),
        }
    }

    pub fn memory() -> Self {
        Self {
            path: None,
            state: Arc::new(Mutex::new(JsonSessionState {
                loaded: true,
                sessions: BTreeMap::new(),
            })),
        }
    }

    async fn load_if_needed(&self, state: &mut JsonSessionState) -> Result<(), AppError> {
        if state.loaded {
            return Ok(());
        }
        state.sessions = match &self.path {
            Some(path) => json_file::read_map(path).await?,
            None => BTreeMap::new(),
        };
        let now = time::OffsetDateTime::now_utc();
        state.sessions.retain(|_, record| !record.is_expired(now));
        state.loaded = true;
        Ok(())
    }

    async fn save(&self, state: &JsonSessionState) -> Result<(), AppError> {
        if let Some(path) = &self.path {
            json_file::write_pretty(path, &state.sessions).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl SessionStore for JsonSessionStore {
    async fn get(&self, token: &str) -> Result<Option<SessionRecord>, AppError> {
        let mut state = self.state.lock().await;
        self.load_if_needed(&mut state).await?;
        Ok(state.sessions.get(token).cloned())
    }

    async fn put(&self, token: &str, record: SessionRecord) -> Result<(), AppError> {
        let mut state = self.state.lock().await;
        self.load_if_needed(&mut state).await?;
        state.sessions.insert(token.to_string(), record);
        self.save(&state).await
    }

    async fn delete(&self, token: &str) -> Result<(), AppError> {
        let mut state = self.state.lock().await;
        self.load_if_needed(&mut state).await?;
        if state.sessions.remove(token).is_some() {
            self.save(&state).await?;
        }
        Ok(())
    }
}

pub struct LmdbSessionStore {
    env: Env,
    db: Database<Bytes, SessionRecordCodec>,
    write_lock: Arc<Mutex<()>>,
}

struct SessionRecordCodec;

const SESSION_RECORD_MAGIC: &[u8] = b"AMS2";

impl<'a> BytesEncode<'a> for SessionRecordCodec {
    type EItem = SessionRecord;

    fn bytes_encode(item: &'a Self::EItem) -> Result<Cow<'a, [u8]>, BoxedError> {
        let payload = bincode::serialize(&StoredSessionRecord::from_record(item))?;
        let mut bytes = Vec::with_capacity(SESSION_RECORD_MAGIC.len() + payload.len());
        bytes.extend_from_slice(SESSION_RECORD_MAGIC);
        bytes.extend_from_slice(&payload);
        Ok(Cow::Owned(bytes))
    }
}

impl<'a> BytesDecode<'a> for SessionRecordCodec {
    type DItem = SessionRecord;

    fn bytes_decode(bytes: &'a [u8]) -> Result<Self::DItem, BoxedError> {
        if let Some(payload) = bytes.strip_prefix(SESSION_RECORD_MAGIC) {
            let stored: StoredSessionRecord = bincode::deserialize(payload)?;
            return stored.into_record();
        }
        Ok(serde_json::from_slice(bytes)?)
    }
}

#[derive(Serialize, Deserialize)]
struct StoredSessionRecord {
    principal: StoredPrincipal,
    created_at_nanos: Option<i128>,
    updated_at_nanos: Option<i128>,
    expires_at_nanos: Option<i128>,
}

impl StoredSessionRecord {
    fn from_record(record: &SessionRecord) -> Self {
        Self {
            principal: StoredPrincipal::from_principal(&record.principal),
            created_at_nanos: record.created_at.map(OffsetDateTime::unix_timestamp_nanos),
            updated_at_nanos: record.updated_at.map(OffsetDateTime::unix_timestamp_nanos),
            expires_at_nanos: record.expires_at.map(OffsetDateTime::unix_timestamp_nanos),
        }
    }

    fn into_record(self) -> Result<SessionRecord, BoxedError> {
        Ok(SessionRecord {
            principal: self.principal.into_principal()?,
            created_at: decode_timestamp(self.created_at_nanos)?,
            updated_at: decode_timestamp(self.updated_at_nanos)?,
            expires_at: decode_timestamp(self.expires_at_nanos)?,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct StoredPrincipal {
    role: u8,
    display_name: String,
    legal_name: String,
    ref_: String,
    phone: String,
    avatar_url: String,
}

impl StoredPrincipal {
    fn from_principal(principal: &Principal) -> Self {
        Self {
            role: encode_role(&principal.role),
            display_name: principal.display_name.clone(),
            legal_name: principal.legal_name.clone(),
            ref_: principal.ref_.clone(),
            phone: principal.phone.clone(),
            avatar_url: principal.avatar_url.clone(),
        }
    }

    fn into_principal(self) -> Result<Principal, BoxedError> {
        Ok(Principal {
            role: decode_role(self.role)?,
            display_name: self.display_name,
            legal_name: self.legal_name,
            ref_: self.ref_,
            phone: self.phone,
            avatar_url: self.avatar_url,
        })
    }
}

fn encode_role(role: &PrincipalRole) -> u8 {
    match role {
        PrincipalRole::Supplier => 0,
        PrincipalRole::Werka => 1,
        PrincipalRole::Customer => 2,
        PrincipalRole::Admin => 3,
    }
}

fn decode_role(role: u8) -> Result<PrincipalRole, BoxedError> {
    match role {
        0 => Ok(PrincipalRole::Supplier),
        1 => Ok(PrincipalRole::Werka),
        2 => Ok(PrincipalRole::Customer),
        3 => Ok(PrincipalRole::Admin),
        _ => Err("invalid stored session principal role".into()),
    }
}

fn decode_timestamp(timestamp: Option<i128>) -> Result<Option<OffsetDateTime>, BoxedError> {
    timestamp
        .map(OffsetDateTime::from_unix_timestamp_nanos)
        .transpose()
        .map_err(Into::into)
}

impl LmdbSessionStore {
    pub fn open(path: PathBuf, map_size_bytes: usize) -> Result<Self, AppError> {
        std::fs::create_dir_all(&path)?;
        let map_size = map_size_bytes.max(1024 * 1024);
        let env = unsafe {
            // LMDB requires the caller to ensure the environment path is used
            // consistently. This service owns this directory for sessions only.
            EnvOpenOptions::new()
                .map_size(map_size)
                .max_dbs(2)
                .open(&path)
        }
        .map_err(lmdb_error)?;
        let mut wtxn = env.write_txn().map_err(lmdb_error)?;
        let db = env
            .create_database(&mut wtxn, Some("sessions"))
            .map_err(lmdb_error)?;
        wtxn.commit().map_err(lmdb_error)?;

        Ok(Self {
            env,
            db,
            write_lock: Arc::new(Mutex::new(())),
        })
    }
}

#[async_trait]
impl SessionStore for LmdbSessionStore {
    async fn get(&self, token: &str) -> Result<Option<SessionRecord>, AppError> {
        let key = session_key(token);
        let record = {
            let rtxn = self.env.read_txn().map_err(lmdb_error)?;
            self.db.get(&rtxn, &key).map_err(lmdb_error)?
        };
        if record.is_some() {
            return Ok(record);
        }

        let legacy_record = {
            let rtxn = self.env.read_txn().map_err(lmdb_error)?;
            self.db.get(&rtxn, token.as_bytes()).map_err(lmdb_error)?
        };
        if let Some(record) = legacy_record {
            self.put(token, record.clone()).await?;
            self.delete_legacy_key(token).await?;
            return Ok(Some(record));
        }

        Ok(None)
    }

    async fn put(&self, token: &str, record: SessionRecord) -> Result<(), AppError> {
        let key = session_key(token);
        let _guard = self.write_lock.lock().await;
        let mut wtxn = self.env.write_txn().map_err(lmdb_error)?;
        self.db.put(&mut wtxn, &key, &record).map_err(lmdb_error)?;
        wtxn.commit().map_err(lmdb_error)
    }

    async fn delete(&self, token: &str) -> Result<(), AppError> {
        let key = session_key(token);
        let _guard = self.write_lock.lock().await;
        let mut wtxn = self.env.write_txn().map_err(lmdb_error)?;
        self.db.delete(&mut wtxn, &key).map_err(lmdb_error)?;
        self.db
            .delete(&mut wtxn, token.as_bytes())
            .map_err(lmdb_error)?;
        wtxn.commit().map_err(lmdb_error)
    }
}

impl LmdbSessionStore {
    async fn delete_legacy_key(&self, token: &str) -> Result<(), AppError> {
        let _guard = self.write_lock.lock().await;
        let mut wtxn = self.env.write_txn().map_err(lmdb_error)?;
        self.db
            .delete(&mut wtxn, token.as_bytes())
            .map_err(lmdb_error)?;
        wtxn.commit().map_err(lmdb_error)
    }
}

fn session_key(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

fn lmdb_error(error: heed::Error) -> AppError {
    AppError::Storage(format!("lmdb session store failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{
        BytesDecode, BytesEncode, LmdbSessionStore, SessionRecordCodec, SessionStore, session_key,
    };
    use crate::core::auth::models::{Principal, PrincipalRole};
    use crate::core::session::models::SessionRecord;
    use heed::types::Bytes;
    use heed::{Database, EnvOpenOptions};

    #[tokio::test]
    async fn lmdb_get_migrates_legacy_raw_token_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = LmdbSessionStore::open(dir.path().join("sessions.lmdb"), 1024 * 1024)
            .expect("lmdb store");
        let token = "legacy-token";
        let record = SessionRecord::new(principal(), time::OffsetDateTime::now_utc(), None, None);

        {
            let mut wtxn = store.env.write_txn().expect("write txn");
            store
                .db
                .put(&mut wtxn, token.as_bytes(), &record)
                .expect("put legacy session");
            wtxn.commit().expect("commit legacy session");
        }

        let loaded = store.get(token).await.expect("get migrated session");
        assert_eq!(loaded.expect("session").principal.ref_, "admin");

        let key = session_key(token);
        let rtxn = store.env.read_txn().expect("read txn");
        assert!(
            store
                .db
                .get(&rtxn, token.as_bytes())
                .expect("legacy key")
                .is_none()
        );
        assert!(store.db.get(&rtxn, &key).expect("hashed key").is_some());
    }

    #[test]
    fn session_record_codec_round_trips_binary() {
        let record = SessionRecord::new(principal(), time::OffsetDateTime::now_utc(), None, None);
        let bytes = SessionRecordCodec::bytes_encode(&record).expect("encode record");
        let decoded = SessionRecordCodec::bytes_decode(&bytes).expect("decode record");

        assert_eq!(decoded.principal.ref_, "admin");
        assert_eq!(decoded.created_at, record.created_at);
    }

    #[tokio::test]
    async fn lmdb_reads_legacy_json_session_values() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("sessions.lmdb");
        let token = "json-token";
        let key = session_key(token);
        let record = SessionRecord::new(principal(), time::OffsetDateTime::now_utc(), None, None);

        {
            std::fs::create_dir_all(&path).expect("create lmdb dir");
            let env = unsafe {
                EnvOpenOptions::new()
                    .map_size(1024 * 1024)
                    .max_dbs(2)
                    .open(&path)
            }
            .expect("legacy env");
            let mut wtxn = env.write_txn().expect("legacy write txn");
            let db: Database<Bytes, Bytes> = env
                .create_database(&mut wtxn, Some("sessions"))
                .expect("legacy db");
            let json = serde_json::to_vec(&record).expect("legacy json");
            db.put(&mut wtxn, &key, &json).expect("put legacy json");
            wtxn.commit().expect("commit legacy json");
        }

        let store = LmdbSessionStore::open(path, 1024 * 1024).expect("lmdb store");
        let loaded = store.get(token).await.expect("read json session");

        assert_eq!(loaded.expect("session").principal.ref_, "admin");
    }

    fn principal() -> Principal {
        Principal {
            role: PrincipalRole::Admin,
            display_name: "Admin".to_string(),
            legal_name: "Admin".to_string(),
            ref_: "admin".to_string(),
            phone: "+998880000000".to_string(),
            avatar_url: String::new(),
        }
    }
}
