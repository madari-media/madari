//! Independently implemented Stremio HTTP addon protocol; no Stremio code dependency.
pub mod torrent;
use madari_model::*;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

const SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'!')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'=')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}')
    .add(b'<')
    .add(b'>');

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub version: String,
    pub name: String,
    pub resources: Vec<ResourceDeclaration>,
    pub types: Vec<String>,
    pub id_prefixes: Option<Vec<String>>,
    #[serde(default)]
    pub catalogs: Vec<Catalog>,
    #[serde(default)]
    pub addon_catalogs: Vec<Catalog>,
    #[serde(default)]
    #[cfg_attr(feature = "openapi", schema(value_type = std::collections::BTreeMap<String, serde_json::Value>))]
    pub behavior_hints: Fields,
    #[serde(flatten)]
    #[cfg_attr(feature = "openapi", schema(value_type = std::collections::BTreeMap<String, serde_json::Value>))]
    pub extra: Fields,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResourceDeclaration {
    Name(String),
    Detailed {
        name: String,
        #[serde(default)]
        types: Vec<String>,
        #[serde(rename = "idPrefixes")]
        id_prefixes: Option<Vec<String>>,
        #[serde(flatten)]
        #[cfg_attr(feature = "openapi", schema(value_type = std::collections::BTreeMap<String, serde_json::Value>))]
        extra: Fields,
    },
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize)]
pub struct Catalog {
    #[serde(rename = "type")]
    pub content_type: String,
    pub id: String,
    pub name: Option<String>,
    #[serde(default)]
    pub extra: Vec<Extra>,
    #[serde(flatten)]
    #[cfg_attr(feature = "openapi", schema(value_type = std::collections::BTreeMap<String, serde_json::Value>))]
    pub unknown: Fields,
}

impl<'de> Deserialize<'de> for Catalog {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "type")]
            content_type: String,
            id: String,
            name: Option<String>,
            extra: Option<Vec<Extra>>,
            #[serde(default, rename = "extraSupported")]
            supported: Vec<String>,
            #[serde(default, rename = "extraRequired")]
            required: Vec<String>,
            #[serde(flatten)]
            unknown: Fields,
        }
        let wire = Wire::deserialize(deserializer)?;
        let extra = wire.extra.unwrap_or_else(|| {
            wire.supported
                .into_iter()
                .map(|name| Extra {
                    is_required: wire.required.contains(&name),
                    name,
                    options: None,
                    options_limit: None,
                })
                .collect()
        });
        Ok(Self {
            content_type: wire.content_type,
            id: wire.id,
            name: wire.name,
            extra,
            unknown: wire.unknown,
        })
    }
}

impl Catalog {
    pub fn searchable(&self) -> bool {
        self.extra.iter().any(|e| e.name == "search")
    }
    pub fn search_only(&self) -> bool {
        self.extra
            .iter()
            .any(|e| e.name == "search" && e.is_required)
    }
    pub fn paginated(&self) -> bool {
        self.extra.iter().any(|e| e.name == "skip")
    }
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Extra {
    pub name: String,
    #[serde(default)]
    pub is_required: bool,
    pub options: Option<Vec<String>>,
    pub options_limit: Option<usize>,
}

impl Manifest {
    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty()
            || self.name.is_empty()
            || self.version.is_empty()
            || self.resources.is_empty()
        {
            return Err(Error::new(
                ErrorCode::InvalidResponse,
                "manifest is missing identity or resources",
            ));
        }
        Ok(())
    }

    pub fn supports(&self, request: &ResourceRequest) -> bool {
        if matches!(request.resource, Resource::Catalog | Resource::AddonCatalog) {
            let catalogs = if request.resource == Resource::Catalog {
                &self.catalogs
            } else {
                &self.addon_catalogs
            };
            return catalogs.iter().any(|c| {
                c.id == request.id
                    && c.content_type == request.content_type
                    && c.extra
                        .iter()
                        .all(|e| !e.is_required || request.extra.contains_key(&e.name))
                    && request
                        .extra
                        .iter()
                        .all(|(k, _)| c.extra.iter().any(|e| e.name == *k))
            });
        }
        self.resources.iter().any(|r| {
            let (name, types, prefixes) = match r {
                ResourceDeclaration::Name(name) => (name, &self.types, &self.id_prefixes),
                ResourceDeclaration::Detailed {
                    name,
                    types,
                    id_prefixes,
                    ..
                } => (name, types, id_prefixes),
            };
            name == request.resource.as_str()
                && types.contains(&request.content_type)
                && prefixes
                    .as_ref()
                    .is_none_or(|p| p.is_empty() || p.iter().any(|p| request.id.starts_with(p)))
        })
    }
}

pub fn manifest_url(input: &str) -> Result<Url> {
    let url = Url::parse(input)
        .map_err(|_| Error::new(ErrorCode::InvalidInput, "invalid manifest URL"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Error::new(
            ErrorCode::UnsupportedTransport,
            "only HTTP(S) addon transport is supported",
        ));
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || !url.path().ends_with("/manifest.json")
    {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "expected an HTTP(S) manifest.json URL without userinfo or fragment",
        ));
    }
    Ok(url)
}

pub fn resource_url(manifest: &Url, request: &ResourceRequest) -> Result<Url> {
    if request.id.is_empty()
        || request.content_type.is_empty()
        || [".", ".."].contains(&request.content_type.as_str())
        || (!request.extra.is_empty() && [".", ".."].contains(&request.id.as_str()))
    {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "invalid resource path segment",
        ));
    }
    let prefix = manifest
        .path()
        .strip_suffix("manifest.json")
        .ok_or_else(|| Error::new(ErrorCode::InvalidInput, "invalid manifest path"))?;
    let encode = |s: &str| utf8_percent_encode(s, SEGMENT).to_string();
    let mut path = format!(
        "{}{}/{}/{}",
        prefix,
        request.resource.as_str(),
        encode(&request.content_type),
        encode(&request.id)
    );
    if !request.extra.is_empty() {
        path.push('/');
        path.push_str(
            &request
                .extra
                .iter()
                .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
                .collect::<Vec<_>>()
                .join("&"),
        );
    }
    path.push_str(".json");
    let mut url = manifest.clone();
    url.set_path(&path);
    Ok(url)
}

pub fn parse_response(resource: Resource, value: Value) -> Result<ResourceData> {
    fn field<T: serde::de::DeserializeOwned>(v: &Value, key: &str) -> Result<T> {
        let value = v.get(key).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidResponse,
                "missing resource response field",
            )
        })?;
        serde_json::from_value(value.clone()).map_err(|_| {
            Error::new(
                ErrorCode::InvalidResponse,
                "invalid resource response shape",
            )
        })
    }
    fn list<T: serde::de::DeserializeOwned>(value: &Value, key: &str) -> Result<Vec<T>> {
        match value.get(key) {
            Some(Value::Null) => Ok(Vec::new()),
            Some(Value::Array(items)) => Ok(items
                .iter()
                .filter_map(|item| serde_json::from_value(item.clone()).ok())
                .collect()),
            _ => Err(Error::new(
                ErrorCode::InvalidResponse,
                "missing or invalid resource response list",
            )),
        }
    }
    Ok(match resource {
        Resource::Catalog => ResourceData::Catalog(list(
            &value,
            if value.get("metasDetailed").is_some() {
                "metasDetailed"
            } else {
                "metas"
            },
        )?),
        Resource::Meta => ResourceData::Meta(field(&value, "meta")?),
        Resource::Stream => ResourceData::Stream(list(&value, "streams")?),
        Resource::Subtitles => ResourceData::Subtitles(list(&value, "subtitles")?),
        Resource::AddonCatalog => ResourceData::AddonCatalog(list(&value, "addons")?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn partial_catalogs_null_videos_and_legacy_extras_match_sdk() {
        let response = parse_response(
            Resource::Catalog,
            json!({"metas":[
                {"id":"tt1","type":"movie","name":"Movie","videos":null},
                {"invalid":true},
                {"id":"tt2","type":"series","name":"Show"}
            ]}),
        )
        .unwrap();
        let ResourceData::Catalog(items) = response else {
            panic!("catalog expected")
        };
        assert_eq!(items.len(), 2);
        assert!(items[0].videos.is_empty());
        assert!(
            matches!(parse_response(Resource::Catalog, json!({"metas":null})).unwrap(), ResourceData::Catalog(items) if items.is_empty())
        );
        let manifest: Manifest = serde_json::from_value(json!({"id":"test", "name":"Test", "version":"1", "resources":["meta", "catalog"], "types":["movie"], "idPrefixes":[],
            "catalogs":[{"id":"search", "type":"movie", "extraSupported":["search", "skip"], "extraRequired":["search"]}]})).unwrap();
        assert!(manifest.supports(&ResourceRequest {
            resource: Resource::Meta,
            content_type: "movie".into(),
            id: "tt1".into(),
            extra: Default::default()
        }));
        assert!(manifest.catalogs[0].search_only());
        assert!(manifest.catalogs[0].paginated());
    }

    #[test]
    fn configured_paths_and_query_survive_with_single_encoding() {
        let base =
            manifest_url("https://example.org/a%2Fb/secret/manifest.json?token=x%2Fy").unwrap();
        let request = ResourceRequest {
            resource: Resource::Catalog,
            content_type: "custom/type".into(),
            id: "a/b".into(),
            extra: [("search".into(), "a / b&c+%".into())].into(),
        };
        assert_eq!(
            resource_url(&base, &request).unwrap().as_str(),
            "https://example.org/a%2Fb/secret/catalog/custom%2Ftype/a%2Fb/search=a%20%2F%20b%26c%2B%25.json?token=x%2Fy"
        );
    }

    #[test]
    fn catalog_and_detailed_resource_override_global_filters() {
        let manifest: Manifest = serde_json::from_value(json!({"id":"test", "name":"Test", "version":"1.0.0", "types":["movie"], "idPrefixes":["tt"], "resources":["meta", {"name":"stream", "types":["custom"]}], "catalogs":[{"type":"custom", "id":"all", "extra":[{"name":"search", "isRequired":true}]}]})).unwrap();
        let mut r = ResourceRequest {
            resource: Resource::Stream,
            content_type: "custom".into(),
            id: "opaque".into(),
            extra: Default::default(),
        };
        assert!(manifest.supports(&r));
        r.resource = Resource::Meta;
        assert!(!manifest.supports(&r));
        r.resource = Resource::Catalog;
        r.id = "all".into();
        assert!(!manifest.supports(&r));
        r.extra.insert("search".into(), "hello".into());
        assert!(manifest.supports(&r));
    }

    #[test]
    fn search_only_catalogs_require_query_and_allow_declared_pagination() {
        let manifest: Manifest = serde_json::from_value(json!({"id":"search", "name":"Search only", "version":"1", "resources":["catalog"], "types":["movie"], "catalogs":[{"id":"lookup", "type":"movie", "extra":[{"name":"search", "isRequired":true},{"name":"skip"}]}]})).unwrap();
        let catalog = &manifest.catalogs[0];
        assert!(catalog.searchable() && catalog.search_only() && catalog.paginated());
        let mut request = ResourceRequest {
            resource: Resource::Catalog,
            content_type: "movie".into(),
            id: "lookup".into(),
            extra: Default::default(),
        };
        assert!(!manifest.supports(&request));
        request.extra.insert("search".into(), "A & B".into());
        request.extra.insert("skip".into(), "50".into());
        assert!(manifest.supports(&request));
        assert!(
            resource_url(
                &manifest_url("https://example.com/manifest.json").unwrap(),
                &request
            )
            .unwrap()
            .as_str()
            .contains("search=A%20%26%20B&skip=50")
        );
        request.extra.insert("unsupported".into(), "1".into());
        assert!(!manifest.supports(&request));
    }

    #[test]
    fn invalid_shapes_are_not_silently_empty() {
        assert!(parse_response(Resource::Stream, json!({"error":"down"})).is_err());
        assert!(parse_response(Resource::Meta, json!({"meta":[]})).is_err());
        assert!(matches!(
            parse_response(Resource::Meta, json!({"meta":null})).unwrap(),
            ResourceData::Meta(None)
        ));
    }
}
