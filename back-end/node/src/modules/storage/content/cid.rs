use cid::Cid;
use multihash::Multihash;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use super::BlockStoreError;

/// SHA2-256 multihash code
const SHA2_256_CODE: u64 = 0x12;

/// DAG-CBOR codec identifier (0x71)
const DAG_CBOR_CODEC: u64 = 0x71;

/// Raw codec identifier (0x55) for raw binary data
const RAW_CODEC: u64 = 0x55;

/// Compute SHA2-256 multihash from data
fn sha256_multihash(data: &[u8]) -> Multihash<64> {
    let digest = Sha256::digest(data);
    Multihash::wrap(SHA2_256_CODE, &digest).expect("SHA256 digest is always 32 bytes")
}

/// Content identifier wrapping CIDv1 with SHA2-256 multihash.
///
/// This is the primary identifier for all content-addressed data in Flow.
/// Two identical pieces of data will always produce the same `ContentId`.
#[derive(Clone)]
pub struct ContentId {
    inner: Cid,
}

impl ContentId {
    /// Create a ContentId from raw bytes using SHA2-256 and RAW codec.
    pub fn from_bytes(data: &[u8]) -> Self {
        let hash = sha256_multihash(data);
        let cid = Cid::new_v1(RAW_CODEC, hash);
        Self { inner: cid }
    }

    /// Create a ContentId from bytes using DAG-CBOR codec.
    ///
    /// Use this for structured IPLD data that has been serialized to DAG-CBOR.
    pub fn from_dag_cbor(data: &[u8]) -> Self {
        let hash = sha256_multihash(data);
        let cid = Cid::new_v1(DAG_CBOR_CODEC, hash);
        Self { inner: cid }
    }

    /// Create a ContentId from an existing CID.
    pub fn from_cid(cid: Cid) -> Self {
        Self { inner: cid }
    }

    /// Parse a ContentId from a string (base32 or base58 encoded).
    pub fn parse(s: &str) -> Result<Self, BlockStoreError> {
        let cid = Cid::from_str(s).map_err(|e| BlockStoreError::InvalidCid(e.to_string()))?;
        Ok(Self { inner: cid })
    }

    /// Get the underlying CID.
    pub fn as_cid(&self) -> &Cid {
        &self.inner
    }

    /// Convert to raw bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.inner.to_bytes()
    }

    /// Parse from raw bytes.
    pub fn from_raw_bytes(bytes: &[u8]) -> Result<Self, BlockStoreError> {
        let cid = Cid::try_from(bytes).map_err(|e| BlockStoreError::InvalidCid(e.to_string()))?;
        Ok(Self { inner: cid })
    }

    /// Get the codec used (e.g., RAW or DAG-CBOR).
    pub fn codec(&self) -> u64 {
        self.inner.codec()
    }

    /// Check if this is a RAW codec CID.
    pub fn is_raw(&self) -> bool {
        self.inner.codec() == RAW_CODEC
    }

    /// Check if this is a DAG-CBOR codec CID.
    pub fn is_dag_cbor(&self) -> bool {
        self.inner.codec() == DAG_CBOR_CODEC
    }

    /// Get the hash bytes (without codec/version prefix).
    pub fn hash_bytes(&self) -> &[u8] {
        self.inner.hash().digest()
    }

    /// Verify that data matches this CID.
    pub fn verify(&self, data: &[u8]) -> bool {
        let expected = if self.is_dag_cbor() {
            Self::from_dag_cbor(data)
        } else {
            Self::from_bytes(data)
        };
        self == &expected
    }
}

impl fmt::Display for ContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl fmt::Debug for ContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContentId")
            .field("cid", &self.to_string())
            .field("codec", &self.codec())
            .finish()
    }
}

impl FromStr for ContentId {
    type Err = BlockStoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl PartialEq for ContentId {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for ContentId {}

impl Hash for ContentId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.hash().digest().hash(state);
    }
}

impl Serialize for ContentId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            serializer.serialize_str(&self.to_string())
        } else {
            serializer.serialize_bytes(&self.to_bytes())
        }
    }
}

impl<'de> Deserialize<'de> for ContentId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{self, Visitor};

        struct ContentIdVisitor;

        impl<'de> Visitor<'de> for ContentIdVisitor {
            type Value = ContentId;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a CID string or bytes")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                ContentId::parse(v).map_err(de::Error::custom)
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                ContentId::from_raw_bytes(v).map_err(de::Error::custom)
            }
        }

        if deserializer.is_human_readable() {
            deserializer.deserialize_str(ContentIdVisitor)
        } else {
            deserializer.deserialize_bytes(ContentIdVisitor)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cid_from_bytes_deterministic() {
        let data = b"hello world";
        let cid1 = ContentId::from_bytes(data);
        let cid2 = ContentId::from_bytes(data);

        assert_eq!(cid1, cid2, "Same data should produce identical CIDs");
    }

    #[test]
    fn test_cid_different_data_different_cid() {
        let cid1 = ContentId::from_bytes(b"hello");
        let cid2 = ContentId::from_bytes(b"world");

        assert_ne!(cid1, cid2, "Different data should produce different CIDs");
    }

    #[test]
    fn test_cid_string_roundtrip() {
        let original = ContentId::from_bytes(b"test data");
        let string = original.to_string();
        let parsed = ContentId::parse(&string).unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn test_cid_bytes_roundtrip() {
        let original = ContentId::from_bytes(b"test data");
        let bytes = original.to_bytes();
        let restored = ContentId::from_raw_bytes(&bytes).unwrap();

        assert_eq!(original, restored);
    }

    #[test]
    fn test_cid_is_raw() {
        let cid = ContentId::from_bytes(b"raw data");
        assert!(cid.is_raw());
        assert!(!cid.is_dag_cbor());
    }

    #[test]
    fn test_cid_is_dag_cbor() {
        let cid = ContentId::from_dag_cbor(b"structured data");
        assert!(cid.is_dag_cbor());
        assert!(!cid.is_raw());
    }

    #[test]
    fn test_cid_verify_valid() {
        let data = b"verify me";
        let cid = ContentId::from_bytes(data);
        assert!(cid.verify(data));
    }

    #[test]
    fn test_cid_verify_invalid() {
        let data = b"original";
        let cid = ContentId::from_bytes(data);
        assert!(!cid.verify(b"modified"));
    }

    #[test]
    fn test_cid_json_serialization() {
        let cid = ContentId::from_bytes(b"json test");
        let json = serde_json::to_string(&cid).unwrap();
        let deserialized: ContentId = serde_json::from_str(&json).unwrap();

        assert_eq!(cid, deserialized);
    }

    #[test]
    fn test_cid_empty_data() {
        let cid = ContentId::from_bytes(b"");
        assert!(!cid.to_string().is_empty());
        assert!(cid.verify(b""));
    }

    #[test]
    fn test_cid_large_data() {
        let data = vec![0u8; 1_000_000]; // 1MB
        let cid = ContentId::from_bytes(&data);
        assert!(cid.verify(&data));
    }

    #[test]
    fn test_cid_hash_consistency() {
        use std::collections::HashSet;

        let cid1 = ContentId::from_bytes(b"hash test");
        let cid2 = ContentId::from_bytes(b"hash test");

        let mut set = HashSet::new();
        set.insert(cid1.clone());

        assert!(set.contains(&cid2));
    }

    #[test]
    fn test_cid_parse_invalid() {
        let result = ContentId::parse("not-a-valid-cid");
        assert!(result.is_err());
    }

    #[test]
    fn test_cid_cbor_serialization() {
        let cid = ContentId::from_bytes(b"cbor test");
        let bytes = serde_ipld_dagcbor::to_vec(&cid).unwrap();
        let deserialized: ContentId = serde_ipld_dagcbor::from_slice(&bytes).unwrap();
        assert_eq!(cid, deserialized);
    }

    #[test]
    fn test_cid_verify_dag_cbor() {
        let data = b"dag-cbor data";
        let cid = ContentId::from_dag_cbor(data);
        assert!(cid.verify(data));
        assert!(!cid.verify(b"wrong data"));
    }

    #[test]
    fn test_cid_different_codec_different_cid() {
        let data = b"same data";
        let raw_cid = ContentId::from_bytes(data);
        let cbor_cid = ContentId::from_dag_cbor(data);

        assert_ne!(
            raw_cid, cbor_cid,
            "Same data with different codec should differ"
        );
        assert_eq!(
            raw_cid.hash_bytes(),
            cbor_cid.hash_bytes(),
            "But hash should be same"
        );
    }

    #[test]
    fn test_cid_from_raw_bytes_invalid() {
        let result = ContentId::from_raw_bytes(&[0, 1, 2, 3]);
        assert!(result.is_err());
    }

    #[test]
    fn test_cid_from_cid() {
        let original = ContentId::from_bytes(b"test");
        let cid = original.as_cid().clone();
        let restored = ContentId::from_cid(cid);
        assert_eq!(original, restored);
    }

    #[test]
    fn test_cid_codec() {
        let raw = ContentId::from_bytes(b"raw");
        let cbor = ContentId::from_dag_cbor(b"cbor");

        assert_eq!(raw.codec(), 0x55); // RAW_CODEC
        assert_eq!(cbor.codec(), 0x71); // DAG_CBOR_CODEC
    }

    #[test]
    fn test_cid_hash_bytes() {
        let cid = ContentId::from_bytes(b"test");
        let hash = cid.hash_bytes();

        assert_eq!(hash.len(), 32, "SHA256 produces 32 bytes");
    }

    #[test]
    fn test_cid_known_vector() {
        // "hello world" with RAW codec should produce a specific CID
        // This ensures compatibility with other IPFS implementations
        let cid = ContentId::from_bytes(b"hello world");
        let expected = "bafkreifzjut3te2nhyekklss27nh3k72ysco7y32koao5eei66wof36n5e";
        assert_eq!(cid.to_string(), expected);
    }

    #[test]
    fn test_cid_debug_format() {
        let cid = ContentId::from_bytes(b"debug test");
        let debug = format!("{:?}", cid);

        assert!(debug.contains("ContentId"));
        assert!(debug.contains("codec"));
    }
}
