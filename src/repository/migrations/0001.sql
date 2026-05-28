CREATE TABLE nodes
(
    key                  INTEGER PRIMARY KEY NOT NULL,
    hops                 INTEGER,
    last_heard           INTEGER,
    snr                  REAL                NOT NULL,
    rssi                 INTEGER,
    is_favorite          INTEGER             NOT NULL,
    is_ignored           INTEGER             NOT NULL,
    is_muted             INTEGER             NOT NULL,
    user_id              TEXT                NOT NULL,
    user_short_name      TEXT                NOT NULL,
    user_long_name       TEXT                NOT NULL,
    user_role            INTEGER             NOT NULL,
    user_hw_model        INTEGER             NOT NULL,
    user_public_key      BLOB                NOT NULL,
    user_is_licensed     INTEGER             NOT NULL,
    user_is_unmessagable INTEGER
) WITHOUT ROWID;

CREATE TABLE telemetry
(
    id       INTEGER PRIMARY KEY ASC,
    node_key INTEGER NOT NULL,
    datetime INTEGER NOT NULL,
    kind     TEXT    NOT NULL,
    data     BLOB
);

CREATE INDEX idx_telemetry_node_key_datetime ON telemetry (node_key, datetime DESC);