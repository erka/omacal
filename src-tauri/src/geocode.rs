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

pub(crate) fn may_fetch_photon(demo: bool) -> bool {
    !demo
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
    fn may_fetch_photon_is_off_in_demo() {
        assert!(may_fetch_photon(false));
        assert!(!may_fetch_photon(true));
    }
}
