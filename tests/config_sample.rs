//! Guards `assets/config.toml.sample` against drifting away from the schema.
//!
//! The sample is the file users copy, so a key that no longer exists costs them
//! a silent misconfiguration. `Config` sets `deny_unknown_fields`, which makes
//! the parse below reject a stale key outright; the value assertions catch the
//! opposite problem, a key that still parses but documents the wrong default.

use sw1nn_lolcommits_rs::{LogOutput, config::Config};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const SAMPLE: &str = "assets/config.toml.sample";

fn sample() -> Result<Config> {
    Ok(toml::from_str(&std::fs::read_to_string(SAMPLE)?)?)
}

#[test]
fn sample_config_populates_every_section() -> Result {
    let config = sample()?;

    assert!(config.client.is_some(), "[client] missing from {SAMPLE}");
    assert!(config.server.is_some(), "[server] missing from {SAMPLE}");
    assert!(config.auth.is_some(), "[auth] missing from {SAMPLE}");
    assert!(
        config.burned_in_chyron.is_some(),
        "[burned_in_chyron] missing from {SAMPLE}"
    );

    Ok(())
}

#[test]
fn sample_client_section_documents_the_defaults() -> Result {
    let client = sample()?.client.ok_or("missing [client]")?;
    let default = sw1nn_lolcommits_rs::config::ClientConfig::default();

    assert_eq!(client.camera_warmup_frames, default.camera_warmup_frames);
    assert_eq!(client.server_url, default.server_url);
    assert_eq!(
        client.server_upload_timeout_secs,
        default.server_upload_timeout_secs
    );
    assert_eq!(client.desktop_notifications, default.desktop_notifications);

    assert_eq!(client.camera_devices.len(), 1);
    assert_eq!(
        client.camera_devices[0].device,
        default.camera_devices[0].device
    );

    Ok(())
}

#[test]
fn sample_server_section_documents_the_defaults() -> Result {
    let server = sample()?.server.ok_or("missing [server]")?;
    let default = sw1nn_lolcommits_rs::config::ServerConfig::default();

    assert_eq!(server.bind_address, default.bind_address);
    assert_eq!(server.bind_port, default.bind_port);
    assert_eq!(server.images_dir, default.images_dir);
    assert_eq!(server.models_dir, default.models_dir);
    assert_eq!(server.static_dir, default.static_dir);
    assert_eq!(server.gallery_title, default.gallery_title);
    assert_eq!(server.burned_in_chyron, default.burned_in_chyron);
    assert_eq!(server.center_person, default.center_person);
    assert_eq!(
        server.max_concurrent_uploads,
        default.max_concurrent_uploads
    );
    assert_eq!(server.log_output, LogOutput::Auto);

    Ok(())
}

#[test]
fn sample_auth_section_documents_the_defaults() -> Result {
    let auth = sample()?.auth.ok_or("missing [auth]")?;
    let default = sw1nn_lolcommits_rs::config::AuthConfig::default();

    assert_eq!(auth.issuer, default.issuer);
    assert_eq!(auth.client_id, default.client_id);
    assert_eq!(auth.required_group, default.required_group);
    assert_eq!(auth.scopes, default.scopes);

    // The endpoint overrides are commented out, so they must derive from the
    // issuer rather than being pinned in the sample.
    assert!(auth.jwks_url.is_none());
    assert!(auth.device_authorization_url.is_none());
    assert!(auth.token_url.is_none());

    Ok(())
}

#[test]
fn sample_chyron_section_documents_the_defaults() -> Result {
    let chyron = sample()?.burned_in_chyron.ok_or("missing [chyron]")?;
    let default = sw1nn_lolcommits_rs::config::BurnedInChyronConfig::default();

    assert_eq!(chyron.default_font_name, default.default_font_name);
    assert_eq!(chyron.chyron_opacity, default.chyron_opacity);
    assert_eq!(chyron.title_font_size, default.title_font_size);
    assert_eq!(chyron.info_font_size, default.info_font_size);

    // The per-element fonts are commented out so they fall back to
    // default_font_name.
    assert!(chyron.message_font_name.is_none());
    assert!(chyron.info_font_name.is_none());
    assert!(chyron.sha_font_name.is_none());
    assert!(chyron.stats_font_name.is_none());

    Ok(())
}
