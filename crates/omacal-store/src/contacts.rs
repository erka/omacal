//! CardDAV contacts (#126), kept for one purpose: suggesting people in the
//! attendee field.
//!
//! **Derived, never authored.** Every row comes from the account's address
//! books and is replaced wholesale on each sync, so there is nothing here to
//! lose and nothing to write back. That is what lets the sync be a delete and
//! an insert rather than a reconciliation: an address book is a few hundred
//! rows, and a card that changed its address should not leave the old one
//! behind under a stale href.
//!
//! One row per address, not per card. The field invites a mailbox, and a
//! colleague with a work and a personal address is two things to pick between
//! — with the same name on both, which is how the list reads them back.

use sqlx::{Row, SqlitePool};

/// One address from an address book, ready to store.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredContact {
    /// The card's URL on the server. With `email` it identifies the row, so a
    /// card losing one of its two addresses loses that row on the next sync.
    pub href: String,
    /// The card's own `UID`, when it has one. Kept for diagnosis: nothing
    /// reads it yet, and a card with no UID is still a person.
    pub uid: Option<String>,
    pub display_name: Option<String>,
    /// Lowercased, which is the identity the suggestions dedup on.
    pub email: String,
}

/// One person the attendee field can offer, from an address book.
#[derive(Debug, Clone, PartialEq)]
pub struct KnownContact {
    pub email: String,
    pub display_name: Option<String>,
}

/// Replaces everything stored for one account's address books.
///
/// In a transaction, so a sync that fails halfway leaves the previous set
/// intact rather than half of each. Returns how many rows the account now
/// has.
pub async fn replace_account_contacts(
    pool: &SqlitePool,
    account_id: i64,
    contacts: &[StoredContact],
    now_ms: i64,
) -> anyhow::Result<u64> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM contacts WHERE account_id = ?1")
        .bind(account_id)
        .execute(&mut *tx)
        .await?;
    for c in contacts {
        // `OR IGNORE`: two cards in different books can carry the same
        // address, and the pair (href, email) is what makes a row unique.
        sqlx::query(
            "INSERT OR IGNORE INTO contacts (account_id, href, uid, display_name, email, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(account_id)
        .bind(&c.href)
        .bind(c.uid.as_deref())
        .bind(c.display_name.as_deref())
        .bind(c.email.to_lowercase())
        .bind(now_ms)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contacts WHERE account_id = ?1")
        .bind(account_id)
        .fetch_one(pool)
        .await?;
    Ok(n as u64)
}

/// Everyone the address books know, one row per address, named once.
///
/// The user's own account addresses are left out for `known_guests`' reason:
/// a field that suggests inviting yourself is wrong, and every account here
/// is the user. Ordered by name so the list is stable between syncs — the
/// ranking that matters (who you actually meet with) belongs to the history
/// half, which the caller puts first.
pub async fn known_contacts(pool: &SqlitePool) -> anyhow::Result<Vec<KnownContact>> {
    let rows = sqlx::query(
        "SELECT LOWER(email) AS email,
                MAX(NULLIF(display_name, '')) AS display_name
           FROM contacts
          WHERE LOWER(email) NOT IN (SELECT LOWER(email) FROM accounts)
          GROUP BY LOWER(email)
          ORDER BY LOWER(COALESCE(MAX(NULLIF(display_name, '')), email))",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| KnownContact {
            email: r.get("email"),
            display_name: r.get("display_name"),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool_with_account() -> SqlitePool {
        let pool = crate::connect_memory().await.unwrap();
        sqlx::query(
            "INSERT INTO accounts (google_sub, email, created_at) VALUES ('s1','me@x.test',0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn contact(href: &str, name: &str, email: &str) -> StoredContact {
        StoredContact {
            href: href.into(),
            uid: Some(format!("uid-{href}")),
            display_name: Some(name.into()),
            email: email.into(),
        }
    }

    /// A sync replaces the account's set: what the server dropped is gone,
    /// which a merge-only upsert would leave behind for ever.
    #[tokio::test]
    async fn a_sync_replaces_the_accounts_contacts() {
        let pool = pool_with_account().await;
        let first = vec![contact("a.vcf", "Ana", "ana@x.com"), contact("b.vcf", "Boris", "boris@x.com")];
        assert_eq!(replace_account_contacts(&pool, 1, &first, 0).await.unwrap(), 2);

        let second = vec![contact("a.vcf", "Ana Petrova", "ana@x.com")];
        assert_eq!(replace_account_contacts(&pool, 1, &second, 1).await.unwrap(), 1);

        let known = known_contacts(&pool).await.unwrap();
        assert_eq!(known.len(), 1);
        assert_eq!(known[0].display_name.as_deref(), Some("Ana Petrova"), "the newer name");
    }

    /// One card, two addresses: two rows, and both are people to invite.
    #[tokio::test]
    async fn a_card_with_two_addresses_offers_both() {
        let pool = pool_with_account().await;
        let cards = vec![
            contact("p.vcf", "Petya", "petya@work.com"),
            contact("p.vcf", "Petya", "petya@home.com"),
        ];
        replace_account_contacts(&pool, 1, &cards, 0).await.unwrap();
        let known = known_contacts(&pool).await.unwrap();
        assert_eq!(
            known.iter().map(|k| k.email.as_str()).collect::<Vec<_>>(),
            vec!["petya@home.com", "petya@work.com"],
            "both, ordered by the name they share and then the address"
        );
    }

    /// The same address from two books is one suggestion, and stored case
    /// never reaches the list.
    #[tokio::test]
    async fn one_address_is_one_person_however_it_was_written() {
        let pool = pool_with_account().await;
        let cards = vec![
            contact("work/i.vcf", "Ivan", "IVAN@x.com"),
            contact("home/i.vcf", "Ivan Ivanov", "ivan@x.com"),
        ];
        replace_account_contacts(&pool, 1, &cards, 0).await.unwrap();
        let known = known_contacts(&pool).await.unwrap();
        assert_eq!(known.len(), 1);
        assert_eq!(known[0].email, "ivan@x.com");
    }

    /// **Never yourself.** Every account row is the user, and a field that
    /// suggests inviting them to their own meeting is wrong — the same rule
    /// `known_guests` keeps.
    #[tokio::test]
    async fn the_users_own_addresses_are_not_suggestions() {
        let pool = pool_with_account().await;
        let cards = vec![contact("me.vcf", "Me", "ME@x.test"), contact("a.vcf", "Ana", "ana@x.com")];
        replace_account_contacts(&pool, 1, &cards, 0).await.unwrap();
        let known = known_contacts(&pool).await.unwrap();
        assert_eq!(known.iter().map(|k| k.email.as_str()).collect::<Vec<_>>(), vec!["ana@x.com"]);
    }

    /// Disconnecting an account takes its contacts with it.
    #[tokio::test]
    async fn contacts_die_with_their_account() {
        let pool = pool_with_account().await;
        replace_account_contacts(&pool, 1, &[contact("a.vcf", "Ana", "ana@x.com")], 0).await.unwrap();
        sqlx::query("DELETE FROM accounts WHERE id = 1").execute(&pool).await.unwrap();
        assert!(known_contacts(&pool).await.unwrap().is_empty());
    }
}
