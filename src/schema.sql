CREATE TABLE IF NOT EXISTS samples (
    stream TEXT NOT NULL,
    start TEXT NOT NULL,
    end TEXT NOT NULL,
    time_ms INTEGER NOT NULL,
    model TEXT NOT NULL,
    kind TEXT NOT NULL,
    histogram TEXT NOT NULL,
    PRIMARY KEY (stream, start, end)
);
CREATE INDEX IF NOT EXISTS samples_time ON samples(time_ms);
CREATE TABLE IF NOT EXISTS cumulative (
    stream TEXT NOT NULL,
    start TEXT NOT NULL,
    end TEXT NOT NULL,
    histogram TEXT NOT NULL,
    updated_ms INTEGER NOT NULL,
    PRIMARY KEY (stream, start)
);
CREATE TABLE IF NOT EXISTS receiver (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    last_received_ms INTEGER NOT NULL
);
