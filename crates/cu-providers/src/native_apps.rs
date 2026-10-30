//! Private, host-supplied native bundle metadata. Not helper outbound context.
use cu_core::types::NativeAppInfo;

pub fn parse(input: &str) -> Vec<NativeAppInfo> {
    if input.len() > 4096 {
        return Vec::new();
    }
    let Ok(apps) = serde_json::from_str::<Vec<NativeAppInfo>>(input) else {
        return Vec::new();
    };
    if apps.len() > 8
        || apps.iter().any(|a| {
            a.name.trim().is_empty()
                || a.name.len() > 200
                || a.name.chars().any(char::is_control)
                || a.bundle_id.is_empty()
                || a.bundle_id.len() > 200
                || !a
                    .bundle_id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b".-_".contains(&c))
        })
    {
        return Vec::new();
    }
    apps
}
pub fn from_env() -> Vec<NativeAppInfo> {
    std::env::var("CU_NATIVE_APPS_JSON")
        .ok()
        .map(|s| parse(&s))
        .unwrap_or_default()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_bounded_metadata_is_accepted() {
        assert_eq!(
            parse(r#"[{"name":"WhatsApp","bundle_id":"net.whatsapp.WhatsApp"}]"#).len(),
            1
        );
        for raw in [
            "not json",
            r#"[{"name":"App","bundle_id":"app.test","private":"not allowed"}]"#,
            r#"[{"name":"","bundle_id":"app.test"}]"#,
            r#"[{"name":"App","bundle_id":"x\nsecret"}]"#,
        ] {
            assert!(parse(raw).is_empty());
        }
        assert!(parse(&" ".repeat(4097)).is_empty());
    }
}
