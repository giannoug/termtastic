CREATE TABLE traceroute
(
    id       INTEGER PRIMARY KEY ASC,
    node_key INTEGER NOT NULL,
    datetime INTEGER NOT NULL,
    data     BLOB
);

CREATE INDEX idx_traceroute_node_key_datetime ON traceroute (node_key, datetime DESC);
