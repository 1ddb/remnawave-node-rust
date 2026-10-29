//! Serde helpers that reproduce the way JavaScript serializes `Date` values
//! (`JSON.stringify` always emits RFC3339 with exactly three fractional digits).

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Deserializer, Serializer};

pub mod rfc3339_millis {
    use super::*;

    pub fn serialize<S>(value: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_rfc3339_opts(SecondsFormat::Millis, true))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        DateTime::parse_from_rfc3339(&raw)
            .map(|value| value.with_timezone(&Utc))
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq)]
    struct Wrapper {
        #[serde(with = "rfc3339_millis")]
        at: DateTime<Utc>,
    }

    #[test]
    fn serializes_with_milliseconds() {
        let wrapper = Wrapper {
            at: Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap(),
        };
        let json = serde_json::to_value(&wrapper).unwrap();
        assert_eq!(json["at"], "2026-01-02T03:04:05.000Z");
    }

    #[test]
    fn deserializes_with_and_without_milliseconds() {
        let with: Wrapper = serde_json::from_value(serde_json::json!({
            "at": "2026-01-02T03:04:05.000Z"
        }))
        .unwrap();
        let without: Wrapper = serde_json::from_value(serde_json::json!({
            "at": "2026-01-02T03:04:05Z"
        }))
        .unwrap();
        assert_eq!(with, without);
    }
}
