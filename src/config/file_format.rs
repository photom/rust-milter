//! The serde file format: the field formats the derives cannot express.
//!
//! See "File format (serde feature)" in `docs/reference/configuration.md`.

/// `Duration` as text with a unit: `ms`, `s`, `m`, or `h`.
pub(super) mod duration {
    use std::time::Duration;

    use serde::de::Error as _;
    use serde::ser::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::config::text;

    pub(crate) fn serialize<S: Serializer>(
        value: &Duration,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match text::format_duration(*value) {
            Ok(text) => serializer.serialize_str(&text),
            Err(reason) => Err(S::Error::custom(reason)),
        }
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Duration, D::Error> {
        let value = String::deserialize(deserializer)?;
        match text::parse_duration(&value) {
            Ok(duration) => Ok(duration),
            Err(reason) => Err(D::Error::custom(format!(
                "invalid duration {value:?}: {reason}"
            ))),
        }
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
        let mut addresses = Vec::new();
        for text in Vec::<String>::deserialize(deserializer)? {
            match ServerConfig::parse_bind_address(&text) {
                Ok(address) => addresses.push(address),
                Err(error) => {
                    return Err(D::Error::custom(format!("bind address {text:?}: {error}")));
                }
            }
        }
        Ok(addresses)
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
            let keepalive =
                KeepaliveConfig::deserialize(de::value::MapAccessDeserializer::new(map))?;
            Ok(Some(keepalive))
        }
    }
}
