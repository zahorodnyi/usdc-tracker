CREATE TABLE IF NOT EXISTS usdc_transfers (
    id              BIGSERIAL PRIMARY KEY,
    tx_hash         CHAR(66) NOT NULL UNIQUE,
    block_number    BIGINT NOT NULL,
    from_address    CHAR(42) NOT NULL,
    to_address      CHAR(42) NOT NULL,
    amount          NUMERIC(38,6) NOT NULL,
    block_time      TIMESTAMPTZ NOT NULL,
    created_at      TIMESTAMPTZ DEFAULT now()
    );

CREATE TABLE IF NOT EXISTS sync_state (
    id              SMALLINT PRIMARY KEY DEFAULT 1,
    last_block      BIGINT NOT NULL,
    updated_at      TIMESTAMPTZ DEFAULT now()
    );
