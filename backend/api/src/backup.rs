//! Backups: everything the site is, in one file you can carry away.
//!
//! The database is dumped as JSON rather than through `pg_dump` or
//! `mysqldump`, which are not in the image and would tie the feature to one
//! engine anyway. A logical dump restores into any of the three the CMS
//! supports, and reads the same on a laptop as on the server.

use chrono::{DateTime, FixedOffset, Utc};
use flate2::{Compression, write::GzEncoder};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseBackend, EntityTrait, QueryFilter, Statement,
    TransactionTrait,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

use crate::{
    entities::{
        category, language, media, plugin_setting, post, post_category, post_tag, site_settings,
        tag, user,
    },
    error::{AppError, AppResult},
    plugins::{BACKUP_PLUGIN, load, load_s3, save},
    state::AppState,
    storage::MediaStorage,
};

/// The largest archive that can be sent to be restored.
///
/// A whole site with its pictures, and no more: an upload nobody has checked
/// is held in memory while it is read, so this is also how much memory one
/// request can ask for.
pub const MAX_IMPORT_BYTES: usize = 2 * 1024 * 1024 * 1024;

/// Bumped when the dump's shape changes, so a restore knows what it is holding.
const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Destination {
    /// A folder on the server's own disk, inside the data directory.
    Local,
    /// The bucket the S3 plugin is configured with.
    S3,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Schedule {
    #[default]
    Off,
    Hourly,
    Daily,
    Weekly,
}

impl Schedule {
    fn interval(self) -> Option<chrono::TimeDelta> {
        match self {
            Schedule::Off => None,
            Schedule::Hourly => Some(chrono::TimeDelta::hours(1)),
            Schedule::Daily => Some(chrono::TimeDelta::days(1)),
            Schedule::Weekly => Some(chrono::TimeDelta::weeks(1)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackupConfig {
    pub destination: Destination,
    /// Folder the archives are written into, under the data directory or the
    /// bucket depending on the destination.
    pub folder: String,
    /// Whether to carry the uploaded files as well as the database. A media
    /// library on S3 is already somewhere durable; one on local disk is not.
    pub include_media: bool,
    pub schedule: Schedule,
    /// How many archives to keep. Older ones are removed after a new one is
    /// written — a backup that fills the disk stops being a backup.
    pub keep: u32,
    #[serde(default)]
    pub last_run_at: Option<DateTime<FixedOffset>>,
    #[serde(default)]
    pub last_error: Option<String>,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            destination: Destination::Local,
            folder: "backups".to_string(),
            include_media: false,
            schedule: Schedule::Off,
            keep: 7,
            last_run_at: None,
            last_error: None,
        }
    }
}

impl BackupConfig {
    /// The folder, without leading or trailing slashes and with no way out of
    /// itself: this is joined onto a path on disk.
    pub fn clean_folder(&self) -> String {
        self.folder
            .split('/')
            .map(str::trim)
            .filter(|part| !part.is_empty() && *part != "." && *part != "..")
            .collect::<Vec<_>>()
            .join("/")
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BackupFile {
    pub name: String,
    pub size_bytes: u64,
    pub created_at: DateTime<FixedOffset>,
}

pub async fn config(state: &AppState) -> AppResult<(bool, BackupConfig)> {
    Ok(
        load::<BackupConfig>(state.db(), &state.secrets, BACKUP_PLUGIN)
            .await?
            .map(|stored| (stored.enabled, stored.config))
            .unwrap_or((false, BackupConfig::default())),
    )
}

pub async fn store(state: &AppState, enabled: bool, config: &BackupConfig) -> AppResult<()> {
    save(state.db(), &state.secrets, BACKUP_PLUGIN, enabled, config).await
}

/// Dumps every table as JSON.
///
/// Listed one by one on purpose. A backup that quietly misses a table it never
/// heard of is worse than one that fails, and this way adding a table is a
/// decision rather than an oversight.
async fn dump_database(db: &impl ConnectionTrait) -> AppResult<Vec<u8>> {
    async fn rows<E: EntityTrait>(db: &impl ConnectionTrait) -> AppResult<Vec<Value>> {
        Ok(E::find().into_json().all(db).await?)
    }

    let dump = json!({
        "format_version": FORMAT_VERSION,
        "taken_at": Utc::now().to_rfc3339(),
        "tables": {
            "site_settings": rows::<site_settings::Entity>(db).await?,
            "users": rows::<user::Entity>(db).await?,
            "languages": rows::<language::Entity>(db).await?,
            "posts": rows::<post::Entity>(db).await?,
            "categories": rows::<category::Entity>(db).await?,
            "tags": rows::<tag::Entity>(db).await?,
            "post_categories": rows::<post_category::Entity>(db).await?,
            "post_tags": rows::<post_tag::Entity>(db).await?,
            "media": rows::<media::Entity>(db).await?,
            "plugin_settings": rows::<plugin_setting::Entity>(db).await?,
        }
    });

    serde_json::to_vec_pretty(&dump)
        .map_err(|err| AppError::Internal(format!("could not write the dump: {err}")))
}

/// Builds the archive: the dump, and the uploaded files when asked for.
async fn build_archive(state: &AppState, config: &BackupConfig) -> AppResult<Vec<u8>> {
    let dump = dump_database(state.db()).await?;

    let mut archive = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));

    let mut header = tar::Header::new_gnu();
    header.set_size(dump.len() as u64);
    header.set_mode(0o600);
    header.set_cksum();
    archive
        .append_data(&mut header, "database.json", dump.as_slice())
        .map_err(|err| AppError::Internal(format!("could not write the dump: {err}")))?;

    if config.include_media {
        add_media(state, &mut archive).await?;
    }

    archive
        .into_inner()
        .and_then(GzEncoder::finish)
        .map_err(|err| AppError::Internal(format!("could not close the archive: {err}")))
}

async fn add_media(
    state: &AppState,
    archive: &mut tar::Builder<GzEncoder<Vec<u8>>>,
) -> AppResult<()> {
    let items = media::Entity::find().all(state.db()).await?;

    for item in items {
        // Read through whichever backend the file actually lives on, so a
        // library that moved to S3 mid-life still comes out whole.
        let storage = crate::plugins::storage_for(state, &item.storage_backend).await?;
        let Some(bytes) = storage.read(&item.storage_key).await else {
            tracing::warn!(key = %item.storage_key, "media file missing, left out of the backup");
            continue;
        };

        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        archive
            .append_data(
                &mut header,
                format!("media/{}", item.storage_key),
                bytes.as_slice(),
            )
            .map_err(|err| AppError::Internal(format!("could not add {}: {err}", item.filename)))?;
    }

    Ok(())
}

/// Takes a backup and writes it where the settings say.
pub async fn run(state: &AppState) -> AppResult<BackupFile> {
    let (_, mut settings) = config(state).await?;

    let result = write_archive(state, &settings).await;

    settings.last_run_at = Some(Utc::now().fixed_offset());
    settings.last_error = result.as_ref().err().map(ToString::to_string);
    let (enabled, _) = config(state).await?;
    store(state, enabled, &settings).await?;

    result
}

async fn write_archive(state: &AppState, settings: &BackupConfig) -> AppResult<BackupFile> {
    let bytes = build_archive(state, settings).await?;
    let now = Utc::now();
    let name = format!("mavicms-{}.tar.gz", now.format("%Y%m%d-%H%M%S"));
    let folder = settings.clean_folder();

    match settings.destination {
        Destination::Local => {
            let dir = state.data_dir.join(&folder);
            tokio::fs::create_dir_all(&dir)
                .await
                .map_err(|err| AppError::Internal(format!("could not create {folder}: {err}")))?;
            tokio::fs::write(dir.join(&name), &bytes)
                .await
                .map_err(|err| AppError::Internal(format!("could not write {name}: {err}")))?;
        }
        Destination::S3 => {
            let stored = load_s3(state.db(), &state.secrets).await?.ok_or_else(|| {
                AppError::Validation(
                    "the S3 plugin has to be set up before backups can go there".to_string(),
                )
            })?;
            MediaStorage::S3(Box::new(stored.config))
                .put(&format!("{folder}/{name}"), &bytes, "application/gzip")
                .await?;
        }
    }

    prune(state, settings).await;

    Ok(BackupFile {
        name,
        size_bytes: bytes.len() as u64,
        created_at: now.fixed_offset(),
    })
}

/// Removes all but the newest `keep` archives. Best effort: a backup that was
/// written is not undone because tidying up failed.
async fn prune(state: &AppState, settings: &BackupConfig) {
    if settings.keep == 0 {
        return;
    }
    let Ok(existing) = list(state, settings).await else {
        return;
    };

    for old in existing.into_iter().skip(settings.keep as usize) {
        if let Err(err) = delete(state, settings, &old.name).await {
            tracing::warn!(error = %err, name = %old.name, "could not remove an old backup");
        }
    }
}

/// A backup name, and nothing that is about to be joined onto a path and
/// point somewhere else.
fn clean_name(name: &str) -> AppResult<String> {
    let refused =
        name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..");

    if refused {
        return Err(AppError::Validation(
            "that is not a backup name".to_string(),
        ));
    }
    Ok(name.to_string())
}

/// One archive's contents.
pub async fn read(state: &AppState, settings: &BackupConfig, name: &str) -> AppResult<Vec<u8>> {
    let name = clean_name(name)?;
    let folder = settings.clean_folder();

    let bytes = match settings.destination {
        Destination::Local => tokio::fs::read(state.data_dir.join(&folder).join(&name))
            .await
            .ok(),
        Destination::S3 => {
            let stored = load_s3(state.db(), &state.secrets)
                .await?
                .ok_or_else(|| AppError::Validation("the S3 plugin is not set up".to_string()))?;
            MediaStorage::S3(Box::new(stored.config))
                .read(&format!("{folder}/{name}"))
                .await
        }
    };

    bytes.ok_or_else(|| AppError::NotFound("backup".to_string()))
}

/// The archives that exist, newest first.
pub async fn list(state: &AppState, settings: &BackupConfig) -> AppResult<Vec<BackupFile>> {
    let folder = settings.clean_folder();
    let mut files = Vec::new();

    match settings.destination {
        Destination::Local => {
            let dir = state.data_dir.join(&folder);
            let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
                return Ok(files);
            };
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.ends_with(".tar.gz") {
                    continue;
                }
                let Ok(meta) = entry.metadata().await else {
                    continue;
                };
                files.push(BackupFile {
                    name,
                    size_bytes: meta.len(),
                    created_at: meta
                        .modified()
                        .map(|time| DateTime::<Utc>::from(time).fixed_offset())
                        .unwrap_or_else(|_| Utc::now().fixed_offset()),
                });
            }
        }
        Destination::S3 => {
            let Some(stored) = load_s3(state.db(), &state.secrets).await? else {
                return Ok(files);
            };
            for object in stored.config.list(&folder).await? {
                let name = object.0.rsplit('/').next().unwrap_or(&object.0).to_string();
                if !name.ends_with(".tar.gz") {
                    continue;
                }
                files.push(BackupFile {
                    name,
                    size_bytes: object.1,
                    created_at: object.2,
                });
            }
        }
    }

    // Newest first, so "keep the last seven" is a matter of skipping.
    files.sort_by_key(|file| std::cmp::Reverse(file.created_at));
    Ok(files)
}

pub async fn delete(state: &AppState, settings: &BackupConfig, name: &str) -> AppResult<()> {
    let name = &clean_name(name)?;
    let folder = settings.clean_folder();

    match settings.destination {
        Destination::Local => {
            let path = state.data_dir.join(&folder).join(name);
            tokio::fs::remove_file(path)
                .await
                .map_err(|err| AppError::Internal(format!("could not remove {name}: {err}")))?;
        }
        Destination::S3 => {
            let Some(stored) = load_s3(state.db(), &state.secrets).await? else {
                return Err(AppError::Validation("S3 is not set up".to_string()));
            };
            MediaStorage::S3(Box::new(stored.config))
                .delete(&format!("{folder}/{name}"))
                .await;
        }
    }

    Ok(())
}

/// Whether the schedule says another backup is due.
pub fn is_due(config: &BackupConfig) -> bool {
    let Some(interval) = config.schedule.interval() else {
        return false;
    };
    match config.last_run_at {
        // Never run: due now, so switching the schedule on produces a backup
        // rather than a wait of up to a week before anything exists.
        None => true,
        Some(last) => Utc::now().fixed_offset() - last >= interval,
    }
}

/// Runs scheduled backups.
///
/// Woken every few minutes rather than slept until the next due time: the
/// schedule can be changed from the panel at any moment, and a task asleep for
/// a week would not notice.
pub fn spawn_scheduler(state: AppState) {
    const TICK: std::time::Duration = std::time::Duration::from_secs(5 * 60);

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(TICK).await;

            let (enabled, settings) = match config(&state).await {
                Ok(pair) => pair,
                Err(err) => {
                    tracing::warn!(error = %err, "could not read the backup settings");
                    continue;
                }
            };
            if !enabled || !is_due(&settings) {
                continue;
            }

            match run(&state).await {
                Ok(file) => tracing::info!(name = %file.name, "scheduled backup written"),
                // Recorded against the settings by `run`, so the panel shows
                // why the last one did not happen.
                Err(err) => tracing::warn!(error = %err, "scheduled backup failed"),
            }
        }
    });
}

/// What a restore did, so the panel can say it rather than guess.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RestoreReport {
    /// When the archive was taken.
    pub taken_at: String,
    /// Rows written, by table.
    pub tables: std::collections::BTreeMap<String, u64>,
    /// Uploaded files put back.
    pub media_files: u64,
}

/// Puts an archive back into this site.
///
/// Everything the site has is removed first and replaced by what the archive
/// holds — a restore that merged would leave the site as neither what it was
/// nor what was backed up, and nobody could say which posts came from where.
///
/// It is one transaction. A restore that failed halfway would be worse than
/// the state it was trying to repair, so either the site is what the archive
/// says or it is untouched.
pub async fn restore(state: &AppState, bytes: &[u8]) -> AppResult<RestoreReport> {
    let (dump, media_files) = read_archive(bytes)?;

    let version = dump
        .get("format_version")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if version != FORMAT_VERSION as u64 {
        return Err(AppError::Validation(format!(
            "this archive is version {version}, and this server reads version {FORMAT_VERSION}"
        )));
    }

    let tables = dump
        .get("tables")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::Validation("this archive has no tables in it".to_string()))?;

    let db = state.db();
    let transaction = db.begin().await?;
    let mut written = std::collections::BTreeMap::new();

    // Emptied in the order that leaves nothing pointing at a row that has gone,
    // and filled in the reverse.
    for table in EMPTY_ORDER {
        transaction
            .execute_raw(Statement::from_string(
                transaction.get_database_backend(),
                format!("DELETE FROM {table}"),
            ))
            .await?;
    }

    for table in EMPTY_ORDER.iter().rev() {
        let rows = tables.get(*table).and_then(Value::as_array);
        let Some(rows) = rows else { continue };
        written.insert((*table).to_string(), rows.len() as u64);

        if rows.is_empty() {
            continue;
        }
        let types = column_types(&transaction, table).await?;
        for row in rows {
            insert_row(&transaction, table, row, &types).await?;
        }
    }

    transaction.commit().await?;

    let restored_media = put_media_back(state, media_files).await;

    Ok(RestoreReport {
        taken_at: dump
            .get("taken_at")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        tables: written,
        media_files: restored_media,
    })
}

/// Emptied in this order, filled in the reverse: the joining tables first, so
/// nothing is ever left pointing at a row that is no longer there.
const EMPTY_ORDER: [&str; 10] = [
    "post_tags",
    "post_categories",
    "posts",
    "tags",
    "categories",
    "media",
    "languages",
    "plugin_settings",
    "users",
    "site_settings",
];

/// A file out of an archive: where it goes, and what is in it.
type ArchivedFile = (String, Vec<u8>);

/// The dump and the files, out of the archive.
fn read_archive(bytes: &[u8]) -> AppResult<(Value, Vec<ArchivedFile>)> {
    use std::io::Read;

    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut dump = None;
    let mut media = Vec::new();

    let entries = archive
        .entries()
        .map_err(|err| AppError::Validation(format!("this is not an archive: {err}")))?;

    for entry in entries {
        let mut entry =
            entry.map_err(|err| AppError::Validation(format!("unreadable archive: {err}")))?;
        let path = entry
            .path()
            .map_err(|err| AppError::Validation(format!("unreadable name: {err}")))?
            .to_string_lossy()
            .into_owned();

        let mut contents = Vec::new();
        entry
            .read_to_end(&mut contents)
            .map_err(|err| AppError::Validation(format!("could not read {path}: {err}")))?;

        if path == "database.json" {
            dump = Some(serde_json::from_slice(&contents).map_err(|err| {
                AppError::Validation(format!("the dump in this archive is unreadable: {err}"))
            })?);
        } else if let Some(key) = path.strip_prefix("media/") {
            // The key becomes a storage key and, on local storage, a file path.
            if key.split('/').any(|part| part == "..") || key.is_empty() {
                return Err(AppError::Validation(format!(
                    "this archive holds a file with a name that points outside it: {path}"
                )));
            }
            media.push((key.to_string(), contents));
        }
    }

    let dump =
        dump.ok_or_else(|| AppError::Validation("this archive has no dump in it".to_string()))?;
    Ok((dump, media))
}

/// What each column of a table actually is.
///
/// The dump wrote uuids, timestamps and json all as JSON strings, and Postgres
/// will not read a string into any of them on its own — so the insert has to
/// say what each one is. SQLite will, and says nothing here.
async fn column_types(
    db: &impl ConnectionTrait,
    table: &str,
) -> AppResult<std::collections::HashMap<String, String>> {
    if db.get_database_backend() != DatabaseBackend::Postgres {
        return Ok(Default::default());
    }

    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT column_name, data_type FROM information_schema.columns \
             WHERE table_schema = current_schema() AND table_name = $1",
            [table.into()],
        ))
        .await?;

    rows.into_iter()
        .map(|row| {
            Ok((
                row.try_get("", "column_name")?,
                row.try_get("", "data_type")?,
            ))
        })
        .collect()
}

/// The cast a value needs to land in this column, if it needs one.
fn cast_for(kind: Option<&String>, value: &Value) -> String {
    let Some(kind) = kind else {
        return String::new();
    };
    // A number or a boolean is already what it is. Everything else arrives as
    // text — including a null, which Postgres will not accept as an untyped
    // one for a column that is not text.
    if value.is_number() || value.is_boolean() {
        return String::new();
    }
    match kind.as_str() {
        "uuid" | "json" | "jsonb" | "date" => format!("::{kind}"),
        "timestamp with time zone" => "::timestamptz".to_string(),
        "timestamp without time zone" => "::timestamp".to_string(),
        _ => String::new(),
    }
}

async fn insert_row(
    db: &impl ConnectionTrait,
    table: &str,
    row: &Value,
    types: &std::collections::HashMap<String, String>,
) -> AppResult<()> {
    let object = row
        .as_object()
        .ok_or_else(|| AppError::Validation(format!("a row in {table} is not a row")))?;

    let backend = db.get_database_backend();
    let mut columns = Vec::new();
    let mut placeholders = Vec::new();
    let mut values: Vec<sea_orm::Value> = Vec::new();

    for (name, value) in object {
        // The names come from the dump, which this server wrote from its own
        // entities — but an archive is a file someone can hand you, so a name
        // that is not a name does not reach the statement.
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || name.is_empty() {
            return Err(AppError::Validation(format!(
                "{table} has a column name this cannot use: {name}"
            )));
        }
        columns.push(format!("\"{name}\""));
        placeholders.push(match backend {
            DatabaseBackend::Postgres => {
                format!("${}{}", values.len() + 1, cast_for(types.get(name), value))
            }
            _ => "?".to_string(),
        });
        values.push(json_to_value(value));
    }

    if columns.is_empty() {
        return Ok(());
    }

    db.execute_raw(Statement::from_sql_and_values(
        backend,
        format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            columns.join(", "),
            placeholders.join(", ")
        ),
        values,
    ))
    .await?;

    Ok(())
}

/// A JSON value as something the database can take.
///
/// Everything that is not a number or a boolean goes in as text and is cast by
/// the column it lands in — which is what the dump did on the way out, since
/// SeaORM wrote uuids, timestamps and json all as JSON strings.
fn json_to_value(value: &Value) -> sea_orm::Value {
    match value {
        Value::Null => sea_orm::Value::String(None),
        Value::Bool(inner) => (*inner).into(),
        Value::Number(inner) => match inner.as_i64() {
            Some(number) => number.into(),
            None => inner.as_f64().unwrap_or_default().into(),
        },
        Value::String(inner) => inner.clone().into(),
        other => other.to_string().into(),
    }
}

/// Writes the archive's files back wherever this site keeps its media now.
///
/// Best effort by design: the rows are already in, and a site with its posts
/// and a missing picture is a site. Failing the whole restore over one file
/// would leave nothing.
async fn put_media_back(state: &AppState, files: Vec<ArchivedFile>) -> u64 {
    let mut written = 0;

    for (key, bytes) in files {
        let backend = match media::Entity::find()
            .filter(media::Column::StorageKey.eq(key.clone()))
            .one(state.db())
            .await
        {
            Ok(Some(item)) => item.storage_backend,
            _ => crate::storage::LOCAL.to_string(),
        };

        let storage = match crate::plugins::storage_for(state, &backend).await {
            Ok(storage) => storage,
            Err(err) => {
                tracing::warn!(error = %err, key, "no storage to put this file back on");
                continue;
            }
        };

        match storage.put(&key, &bytes, "application/octet-stream").await {
            Ok(_) => written += 1,
            Err(err) => tracing::warn!(error = %err, key, "could not put a file back"),
        }
    }

    written
}
