-- Safe to run on every start: instances share one database.
CREATE TABLE IF NOT EXISTS accounts (
    id            uuid PRIMARY KEY,
    email         text NOT NULL UNIQUE,
    password_hash text NOT NULL,
    quota_bytes   bigint NOT NULL,
    used_bytes    bigint NOT NULL DEFAULT 0 CHECK (used_bytes >= 0),
    created       timestamptz NOT NULL DEFAULT now()
);

-- Only hashes of login tokens: a leaked database does not hand out sessions.
CREATE TABLE IF NOT EXISTS sessions (
    token_hash bytea PRIMARY KEY,
    account    uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    expires    timestamptz NOT NULL
);
CREATE INDEX IF NOT EXISTS sessions_account ON sessions(account);

CREATE TABLE IF NOT EXISTS failed_logins (
    email text NOT NULL,
    at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS failed_logins_email ON failed_logins(email, at);

CREATE TABLE IF NOT EXISTS repos (
    id    uuid PRIMARY KEY,
    owner uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS repos_owner ON repos(owner);

CREATE TABLE IF NOT EXISTS shares (
    repo    uuid NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    account uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    can_write boolean NOT NULL,
    PRIMARY KEY (repo, account)
);
CREATE INDEX IF NOT EXISTS shares_account ON shares(account);

CREATE TABLE IF NOT EXISTS objects (
    repo    uuid NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    name    text NOT NULL,
    size    bigint NOT NULL,
    created timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (repo, name)
);

CREATE TABLE IF NOT EXISTS teams (
    id      uuid PRIMARY KEY,
    name    text NOT NULL,
    created timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS members (
    team    uuid NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    account uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    role    text NOT NULL CHECK (role IN ('admin', 'lead', 'developer', 'contributor', 'viewer')),
    PRIMARY KEY (team, account)
);
CREATE INDEX IF NOT EXISTS members_account ON members(account);

-- Only hashes of invitation codes, as for login tokens.
CREATE TABLE IF NOT EXISTS invitations (
    code_hash  bytea PRIMARY KEY,
    team       uuid NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    email      text NOT NULL,
    role       text NOT NULL,
    expires    timestamptz NOT NULL,
    invited_by uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    used_at    timestamptz
);

ALTER TABLE repos ADD COLUMN IF NOT EXISTS team uuid REFERENCES teams(id) ON DELETE SET NULL;
ALTER TABLE repos ADD COLUMN IF NOT EXISTS name text;
CREATE INDEX IF NOT EXISTS repos_team ON repos(team);

CREATE TABLE IF NOT EXISTS project_roles (
    repo    uuid NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    account uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    role    text NOT NULL CHECK (role IN ('admin', 'lead', 'developer', 'contributor', 'viewer')),
    PRIMARY KEY (repo, account)
);

CREATE TABLE IF NOT EXISTS area_grants (
    repo    uuid NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    account uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    area    text NOT NULL,
    PRIMARY KEY (repo, account, area)
);

CREATE TABLE IF NOT EXISTS locks (
    repo    uuid NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    area    text NOT NULL,
    holder  uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    expires timestamptz NOT NULL,
    PRIMARY KEY (repo, area)
);

CREATE TABLE IF NOT EXISTS comments (
    id           uuid PRIMARY KEY,
    repo         uuid NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    author       uuid,
    author_email text NOT NULL,
    area         text NOT NULL,
    anchor       text NOT NULL,
    body         text NOT NULL,
    created      timestamptz NOT NULL DEFAULT now(),
    resolved     boolean NOT NULL DEFAULT false
);
CREATE INDEX IF NOT EXISTS comments_repo ON comments(repo, created);

CREATE TABLE IF NOT EXISTS notifications (
    id      bigserial PRIMARY KEY,
    account uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    kind    text NOT NULL,
    team    uuid,
    repo    uuid,
    params  text NOT NULL,
    created timestamptz NOT NULL DEFAULT now(),
    read    boolean NOT NULL DEFAULT false
);
CREATE INDEX IF NOT EXISTS notifications_account ON notifications(account, id);

CREATE TABLE IF NOT EXISTS presence (
    repo    uuid NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    account uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    area    text NOT NULL,
    seen    timestamptz NOT NULL,
    PRIMARY KEY (repo, account)
);

-- No foreign keys: the log outlives what it speaks about.
CREATE TABLE IF NOT EXISTS audit (
    seq         bigserial PRIMARY KEY,
    at          text NOT NULL,
    actor       uuid,
    actor_email text NOT NULL,
    team        uuid,
    repo        uuid,
    action      text NOT NULL,
    detail      text NOT NULL,
    hash        text NOT NULL
);
CREATE INDEX IF NOT EXISTS audit_team ON audit(team, seq);

CREATE OR REPLACE FUNCTION audit_is_append_only() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'the audit log cannot be changed';
END;
$$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS audit_no_change ON audit;
CREATE TRIGGER audit_no_change BEFORE UPDATE OR DELETE ON audit
    FOR EACH ROW EXECUTE FUNCTION audit_is_append_only();
DROP TRIGGER IF EXISTS audit_no_truncate ON audit;
CREATE TRIGGER audit_no_truncate BEFORE TRUNCATE ON audit
    FOR EACH STATEMENT EXECUTE FUNCTION audit_is_append_only();
