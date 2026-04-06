use std::{fs, io::Write, path::Path};
use ed25519_dalek::{self, SigningKey, VerifyingKey};
use rand_core::OsRng;

pub fn setup(name: &str) -> std::io::Result<VerifyingKey> {
    let mut csprng: OsRng = OsRng;
    let signing_key: SigningKey = SigningKey::generate(&mut csprng);
    let verifying_key: VerifyingKey = signing_key.verifying_key();

    let filename = format!("{name}.key");
    let mut file = fs::File::create(&filename)?;
    file.write_all(&signing_key.to_bytes())?;

    Ok(verifying_key)
}

pub fn load_private_key(name: &str) -> Result<SigningKey, String> {
    let path = format!("{name}.key");

    if !Path::new(&path).exists() {
        return Err(format!("No wallet found for '{name}'. Did you build your profile?"));
    }
    let key_bytes = fs::read(path).map_err(|e| format!("Failed to read key file: {e}"))?;
    let bytes: [u8;32] = key_bytes.try_into().map_err(|_| "Key File is corrupted (not 32 bytes)".to_string())?;
    Ok(SigningKey::from_bytes(&bytes))
}

pub fn load_public_key(name: &str) -> Result<VerifyingKey, String> {
    let path = format!("{name}.key");

    if !Path::new(&path).exists() {
        return Err(format!("No wallet found for '{name}'. Did you build your profile?"));
    }
    let key_bytes = fs::read(path).map_err(|e| format!("Failed to read key file: {e}"))?;
    let bytes: [u8;32] = key_bytes.try_into().map_err(|_| "Key File is corrupted (not 32 bytes)".to_string())?;
    let signingkey = SigningKey::from_bytes(&bytes);
    let publickey = signingkey.verifying_key();
    Ok(publickey)
}