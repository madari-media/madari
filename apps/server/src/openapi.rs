//! OpenAPI is generated from the handler annotations and shared Rust DTOs.
use utoipa::{
    Modify, OpenApi,
    openapi::security::{Http, HttpAuthScheme, SecurityScheme},
};

#[derive(OpenApi)]
#[openapi(
    info(title = "Madari Companion API", version = "1.0.0",
        description = "Shared-core companion API. Use a bearer API key for /v1 operations. Media URLs use expiring file-scoped path tickets. Raw streams support byte ranges; FFmpeg output supports seeking with a new start-offset request."),
    paths(
        crate::health, crate::snapshot, crate::events,
        crate::install, crate::set_enabled, crate::remove_addon, crate::reorder,
        crate::query_all, crate::query_one, crate::save_item, crate::remove_item,
        crate::progress, crate::plan, crate::prepare, crate::add_torrent, crate::add_metainfo,
        crate::list_torrents, crate::torrent_details, crate::remove_torrent,
        crate::create_ticket, crate::revoke_ticket, crate::original, crate::transcode
    ),
    components(schemas(madari_model::Error, madari_core::Change)),
    modifiers(&BearerSecurity),
    security(("api_key" = [])),
    tags(
        (name = "Core", description = "Application state and playback contracts"),
        (name = "Addons", description = "Stremio-compatible addon installations and resources"),
        (name = "Torrents", description = "Torrent metadata and lifecycle"),
        (name = "Media", description = "File-scoped tickets and actual media delivery")
    )
)]
struct ApiDoc;

struct BearerSecurity;
impl Modify for BearerSecurity {
    fn modify(&self, document: &mut utoipa::openapi::OpenApi) {
        document
            .components
            .as_mut()
            .expect("components")
            .add_security_scheme(
                "api_key",
                SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
            );
    }
}

pub fn document() -> utoipa::openapi::OpenApi {
    let mut document = ApiDoc::openapi();
    for item in document.paths.paths.values_mut() {
        // HTTP bytes are binary bodies, not JSON integer arrays. This also gives
        // Swagger UI a file chooser for metainfo uploads.
        for operation in [&mut item.get, &mut item.head, &mut item.post]
            .into_iter()
            .flatten()
        {
            if let Some(body) = &mut operation.request_body {
                binary_content(&mut body.content);
            }
            for response in operation.responses.responses.values_mut() {
                if let utoipa::openapi::RefOr::T(response) = response {
                    binary_content(&mut response.content);
                }
            }
        }
        if let Some(head) = &mut item.head {
            head.operation_id = head.operation_id.as_ref().map(|id| format!("{id}_head"));
            for response in head.responses.responses.values_mut() {
                if let utoipa::openapi::RefOr::T(response) = response {
                    response.content.clear();
                }
            }
        }
    }
    document
}

fn binary_content<'a>(
    content: impl IntoIterator<Item = (&'a String, &'a mut utoipa::openapi::Content)>,
) {
    use utoipa::openapi::schema::{KnownFormat, ObjectBuilder, SchemaFormat, Type};
    for (mime, body) in content {
        if mime == "application/octet-stream" || mime == "video/mp4" {
            body.schema = Some(
                ObjectBuilder::new()
                    .schema_type(Type::String)
                    .format(Some(SchemaFormat::KnownFormat(KnownFormat::Binary)))
                    .into(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::collections::HashSet;

    #[test]
    fn generated_contract_has_complete_routes_unique_operations_and_resolved_schemas() {
        let spec = serde_json::to_value(document()).unwrap();
        assert_eq!(spec["openapi"], "3.1.0");
        assert_eq!(
            spec["components"]["securitySchemes"]["api_key"]["scheme"],
            "bearer"
        );
        assert_eq!(spec["security"][0]["api_key"], serde_json::json!([]));
        assert_eq!(
            spec["paths"]["/v1/torrents/metainfo"]["post"]["requestBody"]["content"]["application/octet-stream"]
                ["schema"]["format"],
            "binary"
        );
        let mut ids = HashSet::new();
        let paths = spec["paths"].as_object().unwrap();
        for path in [
            "/v1/health",
            "/v1/snapshot",
            "/v1/events",
            "/v1/addons",
            "/v1/addons/order",
            "/v1/addons/{id}",
            "/v1/query",
            "/v1/addons/{id}/query",
            "/v1/library",
            "/v1/progress",
            "/v1/playback/plan",
            "/v1/playback/prepare",
            "/v1/torrents",
            "/v1/torrents/metainfo",
            "/v1/torrents/{id}",
            "/v1/media",
            "/v1/media/{token}",
            "/media/{token}/original",
            "/media/{token}/transcode",
        ] {
            assert!(paths.contains_key(path), "missing {path}");
        }
        for (path, item) in paths {
            for (method, operation) in item.as_object().unwrap() {
                if !["get", "head", "post", "put", "delete"].contains(&method.as_str()) {
                    continue;
                }
                assert!(
                    ids.insert(operation["operationId"].as_str().unwrap()),
                    "duplicate operation ID"
                );
                if path.starts_with("/media/") {
                    assert_eq!(operation["security"], serde_json::json!([{}]));
                    assert!(
                        operation["parameters"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|p| p["name"] == "token" && p["required"] == true)
                    );
                }
            }
        }
        assert_eq!(ids.len(), 25);
        fn check_refs(value: &Value, root: &Value) {
            match value {
                Value::Object(object) => {
                    if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                        assert!(
                            root.pointer(reference.strip_prefix('#').unwrap()).is_some(),
                            "unresolved schema {reference}"
                        );
                    }
                    object.values().for_each(|v| check_refs(v, root));
                }
                Value::Array(array) => array.iter().for_each(|v| check_refs(v, root)),
                _ => (),
            }
        }
        check_refs(&spec, &spec);
    }
}
