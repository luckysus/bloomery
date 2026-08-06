use super::signature::DomainTrustStore;
use ed25519_dalek::VerifyingKey;

/// Key identifier for the 2026 official signing key.
///
/// Key ids are versioned by year so that rotation appends a new trusted key
/// instead of replacing the existing one: retiring a key means adding the new
/// `bloomery-official-<year>` entry below while keeping older ids trusted for
/// packages that were signed with them.
const OFFICIAL_KEY_ID_2026: &str = "bloomery-official-2026";

/// PLACEHOLDER official Ed25519 public key (32-byte compressed point).
///
/// This is a deterministic placeholder generated from the throwaway seed
/// `[0x42; 32]`; it is a syntactically valid Ed25519 verifying key so
/// `VerifyingKey::from_bytes` never panics, but the matching private key is
/// intentionally not a real release secret.
///
/// TODO(release): replace with the genuine official Bloomery public key before
/// shipping. The private half must be generated offline, stored securely, and
/// never committed to this repository.
const OFFICIAL_PUBLIC_KEY_2026: [u8; 32] = [
    33, 82, 248, 209, 155, 121, 29, 36, 69, 50, 66, 225, 95, 46, 171, 108, 183, 207, 250, 123, 106,
    94, 211, 0, 151, 150, 14, 6, 152, 129, 219, 18,
];

/// Build the trust store seeded with the official Bloomery signing keys.
///
/// Unsigned community packages remain installable as `ThirdPartyUnsigned`;
/// only packages signed by one of the trusted official keys are promoted to
/// `OfficialSigned`.
pub fn official_trust_store() -> DomainTrustStore {
    let mut store = DomainTrustStore::default();
    let key = VerifyingKey::from_bytes(&OFFICIAL_PUBLIC_KEY_2026)
        .expect("official Ed25519 public key bytes must be a valid verifying key");
    store.add_official_key(OFFICIAL_KEY_ID_2026, key);
    store
}
