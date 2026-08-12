use super::signature::DomainTrustStore;
use ed25519_dalek::VerifyingKey;

/// Key identifier for the 2026 official signing key.
///
/// Key ids are versioned by year so that rotation appends a new trusted key
/// instead of replacing the existing one: retiring a key means adding the new
/// `bloomery-official-<year>` entry below while keeping older ids trusted for
/// packages that were signed with them.
const OFFICIAL_KEY_ID_2026: &str = "bloomery-official-2026";

/// Build the trust store seeded with the official Bloomery signing keys.
///
/// Unsigned community packages remain installable as `ThirdPartyUnsigned`;
/// only packages signed by one of the trusted official keys are promoted to
/// `OfficialSigned`. Development builds without a provisioned public key
/// intentionally trust no official package.
pub fn official_trust_store() -> DomainTrustStore {
    trust_store_from_hex(option_env!("BLOOMERY_OFFICIAL_PUBLIC_KEY_2026"))
}

fn trust_store_from_hex(value: Option<&str>) -> DomainTrustStore {
    let Some(value) = value.filter(|value| is_hex_32_byte_key(value)) else {
        return DomainTrustStore::default();
    };
    let Some(bytes) = decode_hex_32(value) else {
        return DomainTrustStore::default();
    };
    let Ok(bytes) = <[u8; 32]>::try_from(bytes.as_slice()) else {
        return DomainTrustStore::default();
    };
    let Ok(key) = VerifyingKey::from_bytes(&bytes) else {
        return DomainTrustStore::default();
    };

    let mut store = DomainTrustStore::default();
    store.add_official_key(OFFICIAL_KEY_ID_2026, key);
    store
}

fn is_hex_32_byte_key(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn decode_hex_32(value: &str) -> Option<Vec<u8>> {
    if !is_hex_32_byte_key(value) {
        return None;
    }
    let mut bytes = Vec::with_capacity(32);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = hex_value(pair[0])?;
        let low = hex_value(pair[1])?;
        bytes.push((high << 4) | low);
    }
    Some(bytes)
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
