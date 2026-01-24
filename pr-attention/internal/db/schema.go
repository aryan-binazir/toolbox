package db

const Schema = `
CREATE TABLE IF NOT EXISTS prs (
    pr_id TEXT PRIMARY KEY,
    url TEXT NOT NULL,
    title TEXT NOT NULL,
    repo TEXT NOT NULL,
    number INTEGER NOT NULL,
    first_seen TEXT NOT NULL,
    last_seen TEXT NOT NULL,
    last_updated_at TEXT NOT NULL,
    last_notified_updated_at TEXT,
    current_status TEXT NOT NULL,
    is_active INTEGER DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_prs_active ON prs(is_active);

CREATE TABLE IF NOT EXISTS run_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_at TEXT NOT NULL,
    prs_found INTEGER NOT NULL,
    notifications_sent INTEGER NOT NULL,
    error_message TEXT,
    duration_ms INTEGER
);

CREATE TABLE IF NOT EXISTS backoff_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    consecutive_failures INTEGER DEFAULT 0,
    last_failure_time TEXT
);
`

func InitSchema(db *Database) error {
	_, err := db.conn.Exec(Schema)
	if err != nil {
		return err
	}
	return nil
}
