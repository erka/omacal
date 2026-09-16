//! vCard, read for exactly two facts: a name and the addresses it answers to.
//!
//! Issue #126 asked for CardDAV contacts, and the narrow version of that ask
//! is the attendee field: a person you have never met with should still be
//! findable by name. So this parser takes `FN`, `N` and `EMAIL` and leaves
//! everything else — photos, addresses, organisations, the whole card — where
//! it found it. A contacts application's parser this is not, deliberately:
//! every property read here is one this app can show, and nothing is stored
//! that nothing displays.
//!
//! vCard 2.1, 3.0 and 4.0 all fold and escape the way iCalendar does, so the
//! line handling mirrors [`crate::ics`]'s rather than inventing a second one.

/// One person, as the attendee field needs them.
#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    /// The card's own `UID` when it has one. Servers that omit it leave the
    /// href to identify the card, which is why this is optional here and the
    /// caller stores both.
    pub uid: Option<String>,
    /// `FN` when the card has one, else a name assembled from `N`. `None`
    /// for a card that names nobody — those are kept anyway when they carry
    /// an address, because an address with no name is still someone to
    /// invite.
    pub name: Option<String>,
    /// Every `EMAIL` on the card, in the order it appeared, lowercased and
    /// de-duplicated. A card with none is not a person this app can invite.
    pub emails: Vec<String>,
}

/// Every card in a body, which may hold one (a `GET`) or hundreds (a
/// `REPORT`). Garbage between cards is skipped rather than fatal: one
/// unreadable card must not cost the address book.
pub fn parse_cards(src: &str) -> Vec<Contact> {
    let mut out = Vec::new();
    let mut current: Option<Card> = None;
    for line in unfold(src) {
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("BEGIN:VCARD") {
            current = Some(Card::default());
            continue;
        }
        if upper.starts_with("END:VCARD") {
            if let Some(card) = current.take() {
                if let Some(contact) = card.finish() {
                    out.push(contact);
                }
            }
            continue;
        }
        if let Some(card) = current.as_mut() {
            card.read(&line);
        }
    }
    out
}

#[derive(Default)]
struct Card {
    uid: Option<String>,
    fn_name: Option<String>,
    n_name: Option<String>,
    emails: Vec<String>,
}

impl Card {
    fn read(&mut self, line: &str) {
        let Some((name, params, value)) = split_property(line) else { return };
        // A `GROUP.PROPERTY` prefix is Apple's, and the group is only there to
        // tie properties together — `item1.EMAIL` is an EMAIL.
        let name = name.rsplit('.').next().unwrap_or(&name).to_ascii_uppercase();
        let value = unescape(value.trim());
        if value.is_empty() {
            return;
        }
        match name.as_str() {
            "UID" => {
                self.uid.get_or_insert(value);
            }
            "FN" => {
                self.fn_name.get_or_insert(value);
            }
            // `N` is Family;Given;Middle;Prefix;Suffix. Read as a fallback for
            // a card with no `FN`, which 2.1 cards often are.
            "N" => {
                let parts: Vec<&str> = value.split(';').map(str::trim).collect();
                let given = parts.get(1).copied().unwrap_or("");
                let family = parts.first().copied().unwrap_or("");
                let joined = [given, family]
                    .iter()
                    .filter(|p| !p.is_empty())
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" ");
                if !joined.is_empty() {
                    self.n_name.get_or_insert(joined);
                }
            }
            "EMAIL" => {
                // A 4.0 card writes `mailto:` here as legally as a bare
                // mailbox; both name the same person.
                let address = value.trim();
                let address = address
                    .strip_prefix("mailto:")
                    .or_else(|| address.strip_prefix("MAILTO:"))
                    .unwrap_or(address)
                    .trim()
                    .to_ascii_lowercase();
                // Quoted-printable is 2.1's, and an encoded address is not one
                // this app can send to; skipping beats storing mojibake.
                let encoded = params
                    .iter()
                    .any(|p| p.eq_ignore_ascii_case("ENCODING=QUOTED-PRINTABLE"));
                if address.contains('@') && !encoded && !self.emails.contains(&address) {
                    self.emails.push(address);
                }
            }
            _ => {}
        }
    }

    fn finish(self) -> Option<Contact> {
        if self.emails.is_empty() {
            return None;
        }
        Some(Contact {
            uid: self.uid,
            name: self.fn_name.or(self.n_name),
            emails: self.emails,
        })
    }
}

/// `NAME;PARAM=x;PARAM=y:value`, with the colon inside a quoted parameter
/// left alone. Returns the property name, its parameters, and the value.
fn split_property(line: &str) -> Option<(String, Vec<String>, &str)> {
    let mut in_quotes = false;
    let mut colon = None;
    for (i, ch) in line.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ':' if !in_quotes => {
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let colon = colon?;
    let (head, value) = line.split_at(colon);
    let mut parts = head.split(';');
    let name = parts.next()?.trim().to_string();
    if name.is_empty() {
        return None;
    }
    Some((name, parts.map(|p| p.trim().to_string()).collect(), &value[1..]))
}

/// vCard escaping: `\n` is a newline, and `\,` `\;` `\\` are the characters
/// themselves. Names carry commas and semicolons often enough to matter.
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') | Some('N') => out.push(' '),
            Some(c) => out.push(c),
            None => {}
        }
    }
    out.trim().to_string()
}

/// The same folding rule as iCalendar: a line beginning with a space or tab
/// continues the one before it.
fn unfold(src: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in src.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(rest) = line.strip_prefix(' ').or_else(|| line.strip_prefix('\t')) {
            if let Some(last) = out.last_mut() {
                last.push_str(rest);
                continue;
            }
        }
        if !line.is_empty() {
            out.push(line.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3.0 card as Nextcloud and Radicale write one.
    #[test]
    fn a_plain_card_gives_a_name_and_an_address() {
        let src = "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:abc-123\r\nFN:Ana Petrova\r\n\
                   N:Petrova;Ana;;;\r\nEMAIL;TYPE=INTERNET;TYPE=WORK:Ana.Petrova@x.com\r\n\
                   TEL;TYPE=CELL:+359888123456\r\nEND:VCARD\r\n";
        assert_eq!(
            parse_cards(src),
            vec![Contact {
                uid: Some("abc-123".into()),
                name: Some("Ana Petrova".into()),
                // Lowercased: the address is the identity everything dedups on.
                emails: vec!["ana.petrova@x.com".into()],
            }]
        );
    }

    /// Apple's own export: `item1.EMAIL` groups, a folded line, and `mailto:`.
    #[test]
    fn apple_groups_folding_and_mailto_all_read_as_one_person() {
        let src = "BEGIN:VCARD\r\nVERSION:4.0\r\nUID:urn:uuid:9\r\n\
                   FN:Ivan Ivanov Pet\r\n rov\r\n\
                   item1.EMAIL;type=INTERNET;type=HOME;type=pref:mailto:IVAN@x.com\r\n\
                   item1.X-ABLabel:_$!<Home>!$_\r\nEND:VCARD\r\n";
        let cards = parse_cards(src);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].name.as_deref(), Some("Ivan Ivanov Petrov"));
        assert_eq!(cards[0].emails, vec!["ivan@x.com".to_string()]);
    }

    /// Several addresses on one card, and the same address twice.
    #[test]
    fn every_address_on_a_card_is_kept_once() {
        let src = "BEGIN:VCARD\nVERSION:3.0\nFN:Petya\nEMAIL:petya@work.com\n\
                   EMAIL;TYPE=HOME:petya@home.com\nEMAIL:PETYA@WORK.COM\nEND:VCARD\n";
        assert_eq!(
            parse_cards(src)[0].emails,
            vec!["petya@work.com".to_string(), "petya@home.com".to_string()]
        );
    }

    /// A 2.1 card with no `FN` still has a name, from `N`.
    #[test]
    fn a_card_without_fn_falls_back_to_n() {
        let src = "BEGIN:VCARD\nVERSION:2.1\nN:Dimitrov;Georgi;;;\nEMAIL:g@d.com\nEND:VCARD\n";
        assert_eq!(parse_cards(src)[0].name.as_deref(), Some("Georgi Dimitrov"));
    }

    /// Escapes: a comma in a name arrives as a comma, not as `\,`.
    #[test]
    fn escaped_punctuation_comes_back_as_itself() {
        let src = "BEGIN:VCARD\nVERSION:3.0\nFN:Petrov\\, Ivan\nEMAIL:i@p.com\nEND:VCARD\n";
        assert_eq!(parse_cards(src)[0].name.as_deref(), Some("Petrov, Ivan"));
    }

    /// **A card nobody can be invited from is not a contact.** Groups,
    /// companies with only a phone number, and the address book's own
    /// metadata cards all land here.
    #[test]
    fn cards_without_an_address_are_left_out() {
        let src = "BEGIN:VCARD\nVERSION:3.0\nFN:Office\nTEL:+1\nEND:VCARD\n\
                   BEGIN:VCARD\nVERSION:3.0\nFN:Real\nEMAIL:real@x.com\nEND:VCARD\n";
        let cards = parse_cards(src);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].name.as_deref(), Some("Real"));
    }

    /// A body of many cards, and one of them broken: the rest still arrive.
    #[test]
    fn a_broken_card_costs_only_itself() {
        let src = "BEGIN:VCARD\nFN:One\nEMAIL:one@x.com\nEND:VCARD\n\
                   NOT A CARD AT ALL\n\
                   BEGIN:VCARD\nFN:Two\nEMAIL:two@x.com\n\
                   BEGIN:VCARD\nFN:Three\nEMAIL:three@x.com\nEND:VCARD\n";
        let names: Vec<_> = parse_cards(src).iter().map(|c| c.name.clone()).collect();
        // The unterminated card is dropped when the next BEGIN replaces it;
        // the ones around it are untouched.
        assert_eq!(names, vec![Some("One".into()), Some("Three".into())]);
    }

    /// Quoted-printable is 2.1's, and an encoded address is not one this app
    /// can send to.
    #[test]
    fn a_quoted_printable_address_is_skipped_rather_than_stored_as_mojibake() {
        let src = "BEGIN:VCARD\nVERSION:2.1\nFN:Encoded\n\
                   EMAIL;ENCODING=QUOTED-PRINTABLE:=41=42@x.com\nEND:VCARD\n";
        assert!(parse_cards(src).is_empty());
    }

    #[test]
    fn nothing_at_all_is_no_contacts_rather_than_a_panic() {
        assert!(parse_cards("").is_empty());
        assert!(parse_cards("garbage").is_empty());
        assert!(parse_cards("BEGIN:VCARD\nEND:VCARD\n").is_empty());
    }
}
