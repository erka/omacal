-- CardDAV contacts (#126), for the attendee field's suggestions only: a name
-- and an address per row, one row per address, because a card with a work and
-- a home mailbox is two people to invite as far as the field is concerned.
--
-- Read-only and derived: every row is replaced from the server on each sync,
-- so nothing here is the user's own writing and losing the table costs a
-- sync. `ON DELETE CASCADE` for the same reason `calendars` has it — a
-- disconnected account leaves nothing behind.
CREATE TABLE contacts (
  id            INTEGER PRIMARY KEY,
  account_id    INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  href          TEXT NOT NULL,
  uid           TEXT,
  display_name  TEXT,
  email         TEXT NOT NULL,
  updated_at    INTEGER NOT NULL,
  UNIQUE(account_id, href, email)
);

CREATE INDEX idx_contacts_email ON contacts(email);
