CREATE TABLE IF NOT EXISTS github_sync_shard (
  id TEXT PRIMARY KEY,
  device_id TEXT NOT NULL,
  shard_kind TEXT NOT NULL,
  shard_date TEXT,
  content_hash TEXT NOT NULL,
  github_path TEXT NOT NULL,
  imported_at TEXT,
  updated_at TEXT NOT NULL,
  UNIQUE(device_id, shard_kind, shard_date)
);

CREATE INDEX IF NOT EXISTS idx_github_sync_shard_device
  ON github_sync_shard (device_id, shard_kind, shard_date);
