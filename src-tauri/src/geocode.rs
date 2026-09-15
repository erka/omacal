//! Place lookup for the event form's Location field.
//!
//! Photon (OpenStreetMap) turns a typed name into a street address. Parsing
//! and formatting live here, tested against cut-down real payloads; the HTTP
//! round trip is the untested half, the same split as `weather.rs`.

use serde::Serialize;

/// One row the Location field can offer. `label` is what is shown and what
/// picking writes into the event — still a free-text LOCATION string.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlaceHit {
    pub label: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    /// `"history"` | `"search"` — UI may style them the same; tests distinguish.
    pub source: &'static str,
}

pub(crate) fn may_fetch_photon(demo: bool, enabled: bool) -> bool {
    !demo && enabled
}

/// Any `http://` / `https://` in the query — meeting links with passcodes,
/// map pins, tickets. History can still match; Photon never sees these.
pub(crate) fn query_has_url(q: &str) -> bool {
    let lower = q.to_ascii_lowercase();
    lower.contains("https://") || lower.contains("http://")
}

/// Two characters before anything shows — one letter matches half a city
/// and reads as noise, the same floor as the guest field.
pub(crate) fn query_is_searchable(q: &str) -> bool {
    q.trim().chars().count() >= 2
}

/// History first (substring match on `label`), then remote hits whose
/// labels are not already in that list. Remote is assumed already
/// query-relevant. Cap at `limit`.
pub(crate) fn merge_places(
    query: &str,
    history: &[PlaceHit],
    remote: &[PlaceHit],
    limit: usize,
) -> Vec<PlaceHit> {
    let q = query.trim().to_ascii_lowercase();
    let mut out: Vec<PlaceHit> = history
        .iter()
        .filter(|h| h.label.to_ascii_lowercase().contains(&q))
        .cloned()
        .collect();
    for hit in remote {
        if out.len() >= limit {
            break;
        }
        let already = out.iter().any(|h| h.label.eq_ignore_ascii_case(&hit.label));
        if !already {
            out.push(hit.clone());
        }
    }
    out.truncate(limit);
    out
}

/// Photon demo ToS: light personal use; `limit=8` is the politeness layer
/// on the URL itself. Spaces and the rest percent-encoded the same way
/// weather.rs encodes a city name — a crate for this would be a crate for
/// spaces. No `lat`/`lon` bias: home coordinates plus the query is fairly
/// identifying, and nearby ranking is not worth that.
pub(crate) fn photon_url(q: &str) -> String {
    format!(
        "https://photon.komoot.io/api?q={}&limit=8&lang=en",
        percent_encode(q)
    )
}

fn percent_encode(s: &str) -> String {
    s.chars()
        .flat_map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
                vec![c.to_string()]
            } else {
                c.to_string().bytes().map(|b| format!("%{b:02X}")).collect()
            }
        })
        .collect()
}

const DEMO_LABELS: &[&str] = &[
    "Banbury Golf Course, 2626 South Marypost Place, Eagle, Idaho 83616, United States",
    "Room 4A",
    "Room 4A, Sofia, Bulgaria",
    "Sofia Office",
    "Eagle, Idaho, United States",
];

/// Demo never networks. These stand in for Photon, merged with real
/// `known_locations` from the demo DB.
pub(crate) fn demo_canned(query: &str) -> Vec<PlaceHit> {
    let q = query.trim().to_ascii_lowercase();
    DEMO_LABELS
        .iter()
        .filter(|label| label.to_ascii_lowercase().contains(&q))
        .map(|label| PlaceHit {
            label: (*label).to_string(),
            lat: None,
            lon: None,
            source: "search",
        })
        .collect()
}

async fn fetch_photon(url: &str) -> anyhow::Result<String> {
    use anyhow::Context;
    let resp = reqwest::Client::builder()
        .user_agent(concat!("omacal/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(5))
        .build()?
        .get(url)
        .send()
        .await
        .context("photon unreachable")?;
    if !resp.status().is_success() {
        anyhow::bail!("photon answered {}", resp.status());
    }
    Ok(resp.text().await?)
}

#[tauri::command]
pub async fn search_places(
    state: tauri::State<'_, crate::AppState>,
    query: String,
) -> Result<Vec<PlaceHit>, String> {
    let q = query.trim();
    if !query_is_searchable(q) {
        return Ok(Vec::new());
    }
    let history = omacal_store::known_locations(&state.pool)
        .await
        .map_err(|e| crate::errors::user_facing(&e))?
        .into_iter()
        .map(|h| PlaceHit {
            label: h.label,
            lat: None,
            lon: None,
            source: "history",
        })
        .collect::<Vec<_>>();
    let enabled = crate::settings::photon_places(&state.pool).await;
    if !may_fetch_photon(state.demo, enabled) || query_has_url(q) {
        // Demo stands in for Photon only when the user opted in and the
        // query is not a URL — the same two gates as a real fetch.
        let remote = if state.demo && enabled && !query_has_url(q) {
            demo_canned(q)
        } else {
            Vec::new()
        };
        return Ok(merge_places(q, &history, &remote, 8));
    }
    let url = photon_url(q);
    let remote = match fetch_photon(&url).await {
        Ok(raw) => parse_photon(&raw).unwrap_or_default(),
        Err(e) => {
            tracing::debug!("photon lookup failed: {e:#}");
            Vec::new()
        }
    };
    Ok(merge_places(q, &history, &remote, 8))
}

/// Photon GeoJSON → place rows. Garbage and an empty FeatureCollection are
/// `Ok([])`, never an invented address: a lookup miss is a blank list, not a
/// guess the form would then save.
pub(crate) fn parse_photon(raw: &str) -> anyhow::Result<Vec<PlaceHit>> {
    let v: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return Ok(Vec::new()),
    };
    let Some(features) = v.get("features").and_then(|f| f.as_array()) else {
        return Ok(Vec::new());
    };
    let mut hits = Vec::new();
    for feat in features {
        let props = feat.get("properties").unwrap_or(&serde_json::Value::Null);
        let Some(label) = format_label(props) else { continue };
        let (lon, lat) = feat
            .get("geometry")
            .and_then(|g| g.get("coordinates"))
            .and_then(|c| c.as_array())
            .and_then(|c| Some((c.first()?.as_f64()?, c.get(1)?.as_f64()?)))
            .map(|(lon, lat)| (Some(lon), Some(lat)))
            .unwrap_or((None, None));
        hits.push(PlaceHit { label, lat, lon, source: "search" });
    }
    Ok(hits)
}

/// `{name}, {housenumber} {street}, {city}, {state} {postcode}, {country}`
/// with empty parts dropped, and a part that merely repeats the previous
/// one dropped too — Photon often restates the street as the name.
fn format_label(p: &serde_json::Value) -> Option<String> {
    let s = |k: &str| {
        p.get(k)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|t| !t.is_empty())
    };
    let street = match (s("housenumber"), s("street")) {
        (Some(n), Some(st)) => Some(format!("{n} {st}")),
        (None, Some(st)) => Some(st.to_string()),
        (Some(n), None) => Some(n.to_string()),
        (None, None) => None,
    };
    let region = match (s("state"), s("postcode")) {
        (Some(st), Some(pc)) => Some(format!("{st} {pc}")),
        (Some(st), None) => Some(st.to_string()),
        (None, Some(pc)) => Some(pc.to_string()),
        (None, None) => None,
    };
    let mut parts: Vec<String> = Vec::new();
    for part in [
        s("name").map(str::to_string),
        street,
        s("city").map(str::to_string),
        region,
        s("country").map(str::to_string),
    ] {
        let Some(part) = part else { continue };
        if parts.last().is_some_and(|prev| prev.eq_ignore_ascii_case(&part)) {
            continue;
        }
        parts.push(part);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BANBURY: &str = r#"{
      "type":"FeatureCollection",
      "features":[{
        "type":"Feature",
        "properties":{
          "osm_key":"leisure","osm_value":"golf_course","type":"house",
          "housenumber":"2626","name":"Banbury Golf Course",
          "street":"South Marypost Place","city":"Eagle","state":"Idaho",
          "country":"United States","postcode":"83616","countrycode":"US"
        },
        "geometry":{"type":"Point","coordinates":[-116.3655425,43.6718503]}
      }]
    }"#;

    #[test]
    fn banbury_formats_as_name_and_street_address() {
        let hits = parse_photon(BANBURY).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].label,
            "Banbury Golf Course, 2626 South Marypost Place, Eagle, Idaho 83616, United States"
        );
    }

    #[test]
    fn empty_parts_are_omitted_not_comma_runs() {
        let raw = r#"{"type":"FeatureCollection","features":[{
          "type":"Feature",
          "properties":{"name":"Room 4A","city":"Sofia","country":"Bulgaria"},
          "geometry":{"type":"Point","coordinates":[23.3,42.7]}
        }]}"#;
        assert_eq!(parse_photon(raw).unwrap()[0].label, "Room 4A, Sofia, Bulgaria");
    }

    #[test]
    fn garbage_and_empty_are_empty_not_an_error_guess() {
        assert!(parse_photon("not json").unwrap().is_empty());
        assert!(parse_photon(r#"{"type":"FeatureCollection","features":[]}"#).unwrap().is_empty());
    }

    #[test]
    fn may_fetch_photon_is_off_until_opted_in_and_off_in_demo() {
        // Absent / off is every install that predates the setting, and
        // Photon's public server asks for light personal use — so the
        // network hop stays behind an explicit on.
        assert!(!may_fetch_photon(false, false));
        assert!(may_fetch_photon(false, true));
        assert!(!may_fetch_photon(true, true));
        assert!(!may_fetch_photon(true, false));
    }

    #[test]
    fn a_url_in_the_query_never_goes_to_photon() {
        // Zoom/Teams passcodes ride in Location; a map pin is a URL too.
        // History can still match locally; the remote hop is the leak.
        assert!(query_has_url("https://us02web.zoom.us/j/123?pwd=secret"));
        assert!(query_has_url("Join at HTTP://meet.google.com/abc"));
        assert!(query_has_url("Room 4A, https://maps.example.com/pin"));
        assert!(!query_has_url("Room 4A"));
        assert!(!query_has_url("http"));
        assert!(!query_has_url("zoom.us/j/123"));
    }

    fn hit(source: &'static str, label: &str) -> PlaceHit {
        PlaceHit { label: label.to_string(), lat: None, lon: None, source }
    }

    #[test]
    fn history_matches_come_first_and_photon_does_not_duplicate_them() {
        let history = vec![hit("history", "Room 4A, Sofia, Bulgaria")];
        let remote = vec![
            hit("search", "Room 4A, Sofia, Bulgaria"),
            hit(
                "search",
                "Banbury Golf Course, 2626 South Marypost Place, Eagle, Idaho 83616, United States",
            ),
        ];
        let merged = merge_places("room", &history, &remote, 8);
        assert_eq!(merged[0].source, "history");
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn short_queries_do_not_search() {
        assert!(!query_is_searchable("B"));
        assert!(query_is_searchable("Ba"));
    }

    #[test]
    fn merge_caps_at_limit() {
        let history: Vec<PlaceHit> = (0..10)
            .map(|i| hit("history", &format!("History {i} Place")))
            .collect();
        let remote: Vec<PlaceHit> = (0..10)
            .map(|i| hit("search", &format!("Search {i} Place")))
            .collect();
        let merged = merge_places("place", &history, &remote, 8);
        assert_eq!(merged.len(), 8);
        assert!(merged.iter().all(|h| h.source == "history"));
    }

    #[test]
    fn photon_url_encodes_the_query_and_pins_limit_without_a_location_bias() {
        let url = photon_url("Banbury Golf Course");
        assert_eq!(
            url,
            "https://photon.komoot.io/api?q=Banbury%20Golf%20Course&limit=8&lang=en"
        );
        // Home coordinates plus the query is fairly identifying. The
        // request is the typed name only.
        assert!(!url.contains("lat="));
        assert!(!url.contains("lon="));
    }

    #[test]
    fn demo_canned_filters_by_substring_and_includes_banbury() {
        let hits = demo_canned("banbury");
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].label,
            "Banbury Golf Course, 2626 South Marypost Place, Eagle, Idaho 83616, United States"
        );
        assert!(demo_canned("zzzz").is_empty());
        assert!(demo_canned("room").iter().any(|h| h.label.contains("Room 4A")));
    }

}
