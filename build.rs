use std::fs;

/// Script de compilation : prépare l'environnement ESP-IDF et injecte les
/// identifiants Wi-Fi.
fn main() {
    embuild::espidf::sysenv::output();
    wifi_config();
}

/// Lit `wifi.toml` et transmet le SSID et le mot de passe au firmware via
/// `env!` (`WIFI_SSID` et `WIFI_PASS`).
///
/// # Panics
///
/// Panique, ce qui arrête la compilation avec un message clair, si le fichier
/// est absent ou n'est pas du TOML valide, ou si une clé manque, n'est pas une
/// chaîne ou contient un retour à la ligne (qui casserait la directive
/// `cargo:rustc-env`).
fn wifi_config() {
    println!("cargo:rerun-if-changed=wifi.toml");
    let text = fs::read_to_string("wifi.toml")
        .unwrap_or_else(|_| panic!("wifi.toml not found: copy wifi.toml.example to wifi.toml"));
    let table: toml::Table = text
        .parse()
        .unwrap_or_else(|e| panic!("invalid wifi.toml: {e}"));
    for (key, env) in [("ssid", "WIFI_SSID"), ("password", "WIFI_PASS")] {
        let value = table
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("wifi.toml: key `{key}` is missing or not a string"));
        assert!(
            !value.contains(['\n', '\r']),
            "wifi.toml: `{key}` must not contain a line break"
        );
        println!("cargo:rustc-env={env}={value}");
    }
}
