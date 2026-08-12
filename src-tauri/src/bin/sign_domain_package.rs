use bloomery::domains::{load_package, sign_domain_package};
use ed25519_dalek::SigningKey;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

const KEY_ID: &str = "bloomery-official-2026";
const PRIVATE_KEY_ENV: &str = "BLOOMERY_OFFICIAL_PRIVATE_KEY_2026";
const PUBLIC_KEY_ENV: &str = "BLOOMERY_OFFICIAL_PUBLIC_KEY_2026";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("domain package signing failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let root = parse_root()?;
    let private_key = env::var(PRIVATE_KEY_ENV)
        .map_err(|_| format!("{PRIVATE_KEY_ENV} is required for domain package signing"))?;
    let public_key = env::var(PUBLIC_KEY_ENV)
        .map_err(|_| format!("{PUBLIC_KEY_ENV} is required for domain package signing"))?;
    let seed = decode_hex_seed(&private_key)?;
    let signing_key = SigningKey::from_bytes(&seed);
    let derived_public_key = hex_bytes(&signing_key.verifying_key().to_bytes());
    if !derived_public_key.eq_ignore_ascii_case(&public_key) {
        return Err(format!("{PRIVATE_KEY_ENV} does not match {PUBLIC_KEY_ENV}"));
    }

    load_package(&root, env!("CARGO_PKG_VERSION"))
        .map_err(|error| format!("validate package before signing: {error}"))?;
    sign_domain_package(&root, &signing_key, KEY_ID)
        .map_err(|error| format!("write signature: {error}"))?;
    println!("Official domain package signature written.");
    Ok(())
}

fn parse_root() -> Result<PathBuf, String> {
    let mut arguments = env::args_os().skip(1);
    let flag = arguments
        .next()
        .ok_or_else(|| "usage: sign_domain_package --root <package-root>".to_string())?;
    if flag != "--root" {
        return Err("usage: sign_domain_package --root <package-root>".to_string());
    }
    let root = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "package root is required".to_string())?;
    if arguments.next().is_some() {
        return Err("usage: sign_domain_package --root <package-root>".to_string());
    }
    Ok(root)
}

fn decode_hex_seed(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "{PRIVATE_KEY_ENV} must be exactly 64 hexadecimal characters"
        ));
    }
    let mut seed = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_value(pair[0])
            .ok_or_else(|| format!("{PRIVATE_KEY_ENV} contains a non-hexadecimal character"))?;
        let low = hex_value(pair[1])
            .ok_or_else(|| format!("{PRIVATE_KEY_ENV} contains a non-hexadecimal character"))?;
        seed[index] = (high << 4) | low;
    }
    Ok(seed)
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
