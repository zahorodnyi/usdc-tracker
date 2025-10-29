CREATE TABLE IF NOT EXISTS usdc_transfers (
    id              BIGSERIAL PRIMARY KEY,
    tx_hash         CHAR(66) NOT NULL,
    log_index       BIGINT NOT NULL,
    block_number    BIGINT NOT NULL,
    from_address    CHAR(42) NOT NULL,
    to_address      CHAR(42) NOT NULL,
    amount          NUMERIC(38,6) NOT NULL,
    block_time      TIMESTAMPTZ NOT NULL,
    created_at      TIMESTAMPTZ DEFAULT now()
);


CREATE UNIQUE INDEX IF NOT EXISTS idx_usdc_transfers_txhash_logindex
    ON usdc_transfers (tx_hash, log_index);

CREATE TABLE IF NOT EXISTS sync_state (
    id              SMALLINT PRIMARY KEY DEFAULT 1,
    last_block      BIGINT NOT NULL DEFAULT 0,
    updated_at      TIMESTAMPTZ DEFAULT now()
);

INSERT INTO sync_state (id, last_block, updated_at)
VALUES (1, 0, now())
    ON CONFLICT (id) DO NOTHING;