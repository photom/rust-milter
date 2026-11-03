//! The serde file format: the field formats the derives cannot express.
//!
//! See "File format (serde feature)" in `docs/reference/configuration.md`.

/// `Duration` as text with a unit: `ms`, `s`, `m`, or `h`.
pub(super) mod duration {
    use std::time::Duration;

    use serde::de::Error as _;
    use serde::ser::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};

    pub(crate) fn serialize<S: Serializer>(
        value: &Duration,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format(*value).map_err(S::Error::custom)?)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Duration, D::Error> {
        let text = String::deserialize(deserializer)?;
        parse(&text).map_err(D::Error::custom)
    }

    /// Reads a whole number followed by `ms`, `s`, `m`, or `h`.
    pub(crate) fn parse(text: &str) -> Result<Duration, String> {
        let digits = text.len() - text.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let (number, unit) = text.split_at(digits);
        let invalid =
            || format!("invalid duration {text:?}: write a whole number and ms, s, m, or h");
        let number: u64 = number.parse().map_err(|_| invalid())?;
        let seconds_per_unit = match unit {
            "ms" => return Ok(Duration::from_millis(number)),
            "s" => 1,
            "m" => 60,
            "h" => 3_600,
            _ => return Err(invalid()),
        };
        number
            .checked_mul(seconds_per_unit)
            .map(Duration::from_secs)
            .ok_or_else(invalid)
    }

    /// Writes the duration in the largest unit that represents it exactly.
    pub(crate) fn format(value: Duration) -> Result<String, String> {
        if value.subsec_nanos() % 1_000_000 != 0 {
            return Err(format!(
                "{value:?} has a fraction of a millisecond, which the file format cannot hold"
            ));
        }
        if value.subsec_millis() != 0 {
            return Ok(format!("{}ms", value.as_millis()));
        }
        let seconds = value.as_secs();
        Ok(match seconds {
            0 => "0s".to_owned(),
            s if s % 3_600 == 0 => format!("{}h", s / 3_600),
            s if s % 60 == 0 => format!("{}m", s / 60),
            s => format!("{s}s"),
        })
    }
}

/// `bind_addresses` as text, read with
/// [`ServerConfig::parse_bind_address`](super::ServerConfig::parse_bind_address).
pub(super) mod bind_addresses {
    use std::net::SocketAddr;

    use serde::de::Error as _;
    use serde::ser::SerializeSeq;
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::ServerConfig;

    pub(crate) fn serialize<S: Serializer>(
        addresses: &[SocketAddr],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(addresses.len()))?;
        for address in addresses {
            seq.serialize_element(&address.to_string())?;
        }
        seq.end()
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<SocketAddr>, D::Error> {
        Vec::<String>::deserialize(deserializer)?
            .iter()
            .map(|text| {
                ServerConfig::parse_bind_address(text)
                    .map_err(|error| D::Error::custom(format!("bind address {text:?}: {error}")))
            })
            .collect()
    }
}

/// `keepalive` as `false` for off, or as a table of its settings.
pub(super) mod keepalive {
    use std::fmt;

    use serde::de::{self, MapAccess, Visitor};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::KeepaliveConfig;

    /// Writes `false` for `None`: a missing key would read back as the
    /// default, which has keepalive on.
    pub(crate) fn serialize<S: Serializer>(
        value: &Option<KeepaliveConfig>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            None => serializer.serialize_bool(false),
            Some(keepalive) => keepalive.serialize(serializer),
        }
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<KeepaliveConfig>, D::Error> {
        deserializer.deserialize_any(KeepaliveVisitor)
    }

    struct KeepaliveVisitor;

    impl<'de> Visitor<'de> for KeepaliveVisitor {
        type Value = Option<KeepaliveConfig>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("false, or a table of keepalive settings")
        }

        fn visit_bool<E: de::Error>(self, on: bool) -> Result<Self::Value, E> {
            if on {
                return Err(E::custom(
                    "keepalive = true is not a setting; leave the key out for the defaults",
                ));
            }
            Ok(None)
        }

        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            KeepaliveConfig::deserialize(de::value::MapAccessDeserializer::new(map)).map(Some)
        }
    }
}
