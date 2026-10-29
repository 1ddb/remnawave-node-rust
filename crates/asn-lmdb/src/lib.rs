use std::path::Path;
use std::sync::RwLock;

use heed::types::Bytes;
use heed::{Database, Env, EnvFlags, EnvOpenOptions, RoTxn};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{error, info, warn};

pub const ASN_LMDB_PATH: &str = "/usr/local/share/asn/asn-prefixes.lmdb";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct AsnPrefixes {
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
}

#[derive(Debug, Error)]
pub enum AsnError {
    #[error("lmdb error: {0}")]
    Lmdb(#[from] heed::Error),
    #[error("msgpack error: {0}")]
    Msgpack(#[from] rmp_serde::decode::Error),
    #[error("database not found")]
    MissingDatabase,
}

pub struct AsnLmdb {
    env: Env,
    db: Database<Bytes, Bytes>,
    available: RwLock<bool>,
}

impl AsnLmdb {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AsnError> {
        let env = unsafe {
            EnvOpenOptions::new()
                .max_dbs(1)
                .flags(EnvFlags::READ_ONLY)
                .open(path.as_ref())?
        };
        let rtxn = env.read_txn()?;
        let db = env
            .open_database::<Bytes, Bytes>(&rtxn, None)?
            .ok_or(AsnError::MissingDatabase)?;
        drop(rtxn);
        Ok(Self {
            env,
            db,
            available: RwLock::new(true),
        })
    }

    pub fn open_default() -> Option<Self> {
        if !Path::new(ASN_LMDB_PATH).exists() {
            warn!("{ASN_LMDB_PATH} not found — ASN lookup disabled");
            return None;
        }
        match Self::open(ASN_LMDB_PATH) {
            Ok(v) => {
                info!("ASN LMDB database opened successfully");
                Some(v)
            }
            Err(e) => {
                error!("Failed to open ASN LMDB database: {e}");
                None
            }
        }
    }

    pub fn is_available(&self) -> bool {
        *self.available.read().unwrap()
    }

    pub fn get_by_asn(&self, asn: u32) -> Result<Option<AsnPrefixes>, AsnError> {
        if !self.is_available() {
            return Ok(None);
        }
        let rtxn: RoTxn<'_> = self.env.read_txn()?;

        // Fast path: try the exact ordered-binary encoded key for common sizes.
        for key in encode_ordered_binary_u32(asn) {
            if let Some(bytes) = self.db.get(&rtxn, key.as_slice())? {
                return Ok(Some(rmp_serde::from_slice(bytes)?));
            }
        }

        // Fallback: scan and decode keys. The database is small enough that this
        // only happens when the encoding assumption is wrong.
        for entry in self.db.iter(&rtxn)? {
            let (key, value) = entry?;
            if decode_ordered_binary_u32(key) == Some(asn) {
                return Ok(Some(rmp_serde::from_slice(value)?));
            }
        }
        Ok(None)
    }
}

/// Encode a non-negative integer the way `ordered-binary@1.6` does for top-level
/// keys (see `writeKey`). Returns the candidate lengths used by the encoder.
pub fn encode_ordered_binary_u32(value: u32) -> Vec<Vec<u8>> {
    let bits = (value as f64).to_bits();
    let high_int = (bits >> 32) as u32 as i32;
    let low_int = (bits & 0xffff_ffff) as u32 as i32;

    let length = if (low_int & 0xf) != 0 {
        9
    } else if (low_int & 0xfffff) != 0 {
        8
    } else if low_int != 0 || (high_int & 0xf) != 0 {
        6
    } else {
        4
    };

    let word0 = ((high_int as u32) >> 4) | 0x1000_0000;
    let word1 = ((low_int as u32) >> 4) | ((high_int as u32) << 28);
    let mut bytes = word0.to_be_bytes().to_vec();
    bytes.extend_from_slice(&word1.to_be_bytes());
    bytes.push(((low_int as u32 & 0xf) << 4) as u8);
    bytes.truncate(length);
    vec![bytes]
}

pub fn decode_ordered_binary_u32(key: &[u8]) -> Option<u32> {
    if key.is_empty() {
        return None;
    }
    let control = key[0];
    if control < 8 {
        return None;
    }
    let read_be_i32 = |start: usize| -> i32 {
        let mut value = 0u32;
        for i in 0..4 {
            let byte = key.get(start + i).copied().unwrap_or(0);
            value = (value << 8) | byte as u32;
        }
        value as i32
    };

    let mut high_int = read_be_i32(0).wrapping_shl(4);
    let size = key.len();
    let mut low_int: i32 = 0;
    if size > 4 {
        let mut low_raw = read_be_i32(4);
        high_int |= (low_raw as u32 >> 28) as i32;
        if size <= 6 {
            low_raw &= -0x10000;
        }
        low_int = low_raw.wrapping_shl(4);
        if size > 8 {
            low_int |= (key[8] >> 4) as i32;
        }
    }
    if control < 16 {
        high_int ^= 0x7fff_ffff;
        low_int = !low_int;
    }

    let bits = ((high_int as u32 as u64) << 32) | (low_int as u32 as u64);
    let value = f64::from_bits(bits);
    if value.is_finite() && value >= 0.0 && value <= u32::MAX as f64 {
        Some(value as u32)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_order_binary() {
        for v in [1u32, 13335, 15169, 32934, 4294967295, 65536, 4096] {
            let key = encode_ordered_binary_u32(v).pop().unwrap();
            assert_eq!(decode_ordered_binary_u32(&key), Some(v), "value {v}");
        }
    }
}
