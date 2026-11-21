use std::fs;

fn main() {
    embuild::espidf::sysenv::output();
    wifi_config();
}

/// Lit wifi.toml et transmet le SSID et le mot de passe au code via `env!`.
fn wifi_config() {
    println!("cargo:rerun-if-changed=wifi.toml");
    let text = fs::read_to_string("wifi.toml").unwrap_or_else(|_| {
        panic!("wifi.toml introuvable : copier wifi.toml.example en wifi.toml")
    });
    let table: toml::Table = text
        .parse()
        .unwrap_or_else(|e| panic!("wifi.toml invalide : {e}"));
    for (key, env) in [("ssid", "WIFI_SSID"), ("password", "WIFI_PASS")] {
        let value = table
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("wifi.toml : clé `{key}` manquante ou pas une chaîne"));
        assert!(
            !value.contains('\n'),
            "wifi.toml : `{key}` ne doit pas contenir de retour à la ligne"
        );
        println!("cargo:rustc-env={env}={value}");
    }
}
