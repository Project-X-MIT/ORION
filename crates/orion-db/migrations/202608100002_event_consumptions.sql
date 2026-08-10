-- Event consumers claim an envelope before applying a side effect.  The
-- primary key makes redelivery safe while retaining the event contract
-- metadata needed to detect an event-id collision.
CREATE TABLE event_consumptions (
    consumer_key TEXT NOT NULL CHECK (length(btrim(consumer_key)) > 0),
    event_id UUID NOT NULL,
    event_type TEXT NOT NULL CHECK (length(btrim(event_type)) > 0),
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    consumed_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (consumer_key, event_id)
);

CREATE INDEX event_consumptions_type_idx
    ON event_consumptions (event_type, schema_version, consumed_at DESC);

CREATE OR REPLACE FUNCTION prevent_event_consumption_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'event_consumptions is append-only';
END;
$$;

CREATE TRIGGER event_consumptions_append_only
BEFORE UPDATE OR DELETE ON event_consumptions
FOR EACH ROW EXECUTE FUNCTION prevent_event_consumption_mutation();
